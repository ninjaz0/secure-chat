use base64::engine::general_purpose::STANDARD;
use base64::Engine;
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
use keyring::Entry;
use rusqlite::{params, Connection, OptionalExtension};
use secure_chat_client::{RelayClient, RelayEnvelope};
use secure_chat_core::crypto::{
    decrypt_aead, encrypt_aead, random_bytes, sha256, CipherSuite, Key32,
};
use secure_chat_core::safety::to_hex;
use secure_chat_core::{
    accept_session_as_responder_consuming_prekey, decode_group_control, encode_group_control,
    safety_number, start_session_as_initiator, ApnsPlatform, DeviceKeyMaterial,
    GroupControlMessage, GroupMember, GroupPlainMessage, GroupState, GroupTransportEnvelope,
    Invite, InviteMode, PlainMessage, PublicDeviceIdentity, RatchetSession, ReceiptKind,
    ReceiptRequest, SendRequest, TransportFrame, TransportKind, GROUP_TRANSPORT_KIND,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
const KEYCHAIN_SERVICE: &str = "dev.local.securechat";
const PROFILE_ID: &str = "default";
const ENCRYPTED_TEXT_PREFIX: &str = "enc:v1:";
const TEMP_INVITE_TTL_SECS: u64 = 15 * 60;
const TEMP_CONNECTION_TTL_SECS: u64 = 24 * 60 * 60;
const TEMP_MESSAGE_TTL_SECS: u64 = 10 * 60;
const MAX_TEMP_CONNECTIONS: usize = 32;
const MAX_TEMP_MESSAGES_PER_CONNECTION: usize = 200;
const SNAPSHOT_MESSAGES_PER_THREAD: i64 = 500;
const CONTENT_PREFIX: &str = "securechat-content-v1:";
// The relay limits ciphertext to 1 MiB and Axum also applies a request-body
// limit before the handler sees the decoded ciphertext. A chunk is base64'd in
// the rich-content payload, encrypted, wrapped in a transport frame, then sent
// as JSON, so keep the raw file chunk well below those limits.
const ATTACHMENT_CHUNK_BYTES: usize = 128 * 1024;

#[derive(Debug, Error)]
pub enum DesktopError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
    #[error("keychain error: {0}")]
    Keychain(#[from] keyring::Error),
    #[cfg(target_os = "android")]
    #[error("secret store error: {0}")]
    SecretStore(String),
    #[error("protocol error: {0}")]
    Protocol(#[from] secure_chat_core::CryptoError),
    #[error("client error: {0}")]
    Client(#[from] secure_chat_client::ClientError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("profile is not initialized")]
    MissingProfile,
    #[error("contact not found")]
    ContactNotFound,
    #[error("invalid local data: {0}")]
    InvalidData(String),
    #[error("invite link is invalid or incomplete")]
    InvalidInvite,
    #[error("invite link has expired")]
    ExpiredInvite,
    #[error("this invite link belongs to your current device")]
    SelfInvite,
    #[error("contact name cannot be empty")]
    EmptyContactName,
    #[error("message body cannot be empty")]
    EmptyMessage,
    #[error("file not found")]
    FileNotFound,
    #[error("attachment transfer is incomplete")]
    IncompleteAttachment,
    #[error("group not found")]
    GroupNotFound,
    #[error("group name cannot be empty")]
    EmptyGroupName,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub ready: bool,
    pub profile: Option<AppProfile>,
    pub contacts: Vec<ContactSummary>,
    pub messages: Vec<ChatMessageView>,
    pub groups: Vec<GroupSummary>,
    pub group_messages: Vec<GroupMessageView>,
    pub temporary_connections: Vec<TemporaryConnectionSummary>,
    pub temporary_messages: Vec<TemporaryMessageView>,
    pub stickers: Vec<StickerItemView>,
    pub attachment_transfers: Vec<AttachmentTransferView>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppProfile {
    pub display_name: String,
    pub account_id: String,
    pub device_id: String,
    pub relay_url: String,
    pub invite_uri: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContactSummary {
    pub id: String,
    pub display_name: String,
    pub account_id: String,
    pub device_id: String,
    pub safety_number: String,
    pub verified: bool,
    pub last_message: Option<String>,
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessageView {
    pub id: String,
    pub contact_id: String,
    pub direction: MessageDirection,
    pub body: String,
    pub content: MessageContentView,
    pub status: MessageStatus,
    pub sent_at_unix: u64,
    pub received_at_unix: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupSummary {
    pub id: String,
    pub display_name: String,
    pub member_count: usize,
    pub last_message: Option<String>,
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupMemberView {
    pub display_name: String,
    pub account_id: String,
    pub device_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupMessageView {
    pub id: String,
    pub group_id: String,
    pub sender_display_name: String,
    pub direction: MessageDirection,
    pub body: String,
    pub content: MessageContentView,
    pub status: MessageStatus,
    pub sent_at_unix: u64,
    pub received_at_unix: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttachmentView {
    pub id: String,
    pub kind: String,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub local_path: Option<String>,
    pub transfer_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageContentView {
    pub kind: String,
    pub text: Option<String>,
    pub burn_id: Option<String>,
    pub destroyed: bool,
    pub attachment: Option<AttachmentView>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageDirection {
    Outgoing,
    Incoming,
}

impl MessageDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Outgoing => "outgoing",
            Self::Incoming => "incoming",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "incoming" => Self::Incoming,
            _ => Self::Outgoing,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Sent,
    Delivered,
    Read,
    Received,
    Failed,
}

impl MessageStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Read => "read",
            Self::Received => "received",
            Self::Failed => "failed",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "delivered" => Self::Delivered,
            "read" => Self::Read,
            "received" => Self::Received,
            "failed" => Self::Failed,
            _ => Self::Sent,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InviteResponse {
    pub invite_uri: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvitePreview {
    pub normalized_invite_uri: String,
    pub suggested_display_name: String,
    pub account_id: String,
    pub device_id: String,
    pub relay_hint: Option<String>,
    pub expires_unix: Option<u64>,
    pub safety_number: String,
    pub already_added: bool,
    pub existing_display_name: Option<String>,
    pub verified: bool,
    pub temporary: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiveReport {
    pub received_count: usize,
    pub snapshot: AppSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporaryConnectionSummary {
    pub id: String,
    pub display_name: String,
    pub account_id: String,
    pub device_id: String,
    pub safety_number: String,
    pub last_message: Option<String>,
    pub updated_at_unix: u64,
    pub expires_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporaryMessageView {
    pub id: String,
    pub connection_id: String,
    pub direction: MessageDirection,
    pub body: String,
    pub content: MessageContentView,
    pub status: MessageStatus,
    pub sent_at_unix: u64,
    pub received_at_unix: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporaryInviteResponse {
    pub invite_uri: String,
    pub expires_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporaryStartResponse {
    pub connection_id: String,
    pub snapshot: AppSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StickerItemView {
    pub id: String,
    pub display_name: String,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub local_path: String,
    pub created_at_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttachmentTransferView {
    pub id: String,
    pub thread_kind: String,
    pub thread_id: String,
    pub kind: String,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub received_chunks: u64,
    pub total_chunks: u64,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendAttachmentResponse {
    pub attachment_id: String,
    pub snapshot: AppSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportStickerResponse {
    pub sticker: StickerItemView,
    pub snapshot: AppSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WireContent {
    kind: String,
    text: Option<String>,
    burn_id: Option<String>,
    destroyed: Option<bool>,
    target_burn_id: Option<String>,
    attachment: Option<AttachmentView>,
    attachment_id: Option<String>,
    file_name: Option<String>,
    mime_type: Option<String>,
    size_bytes: Option<u64>,
    sha256: Option<String>,
    chunk_index: Option<u64>,
    total_chunks: Option<u64>,
    data_base64: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThreadKind {
    Contact,
    Group,
    Temporary,
}

impl ThreadKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Contact => "contact",
            Self::Group => "group",
            Self::Temporary => "temporary",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "contact" => Some(Self::Contact),
            "group" => Some(Self::Group),
            "temporary" => Some(Self::Temporary),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ContactRecord {
    id: String,
    display_name: String,
    account_id: String,
    device_id: String,
    invite_uri: Option<String>,
    safety_number: String,
    verified: bool,
    remote_identity_json: String,
    updated_at_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GroupRecord {
    id: String,
    display_name: String,
    epoch: u64,
    secret_nonce: Vec<u8>,
    secret_ciphertext: Vec<u8>,
    updated_at_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TemporaryConnectionRecord {
    id: String,
    display_name: String,
    account_id: String,
    device_id: String,
    invite_uri: Option<String>,
    safety_number: String,
    remote_identity_json: String,
    session_nonce: Option<Vec<u8>>,
    session_ciphertext: Option<Vec<u8>>,
    created_at_unix: u64,
    updated_at_unix: u64,
    expires_unix: u64,
}

pub struct DesktopRuntime {
    data_dir: PathBuf,
    conn: Connection,
}

impl DesktopRuntime {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, DesktopError> {
        let data_dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&data_dir)?;
        let conn = Connection::open(data_dir.join("SecureChat.sqlite3"))?;
        let runtime = Self { data_dir, conn };
        runtime.migrate()?;
        Ok(runtime)
    }

    pub async fn bootstrap(
        data_dir: impl AsRef<Path>,
        display_name: &str,
        relay_url: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.bootstrap_profile(display_name, relay_url).await?;
        runtime.snapshot()
    }

    pub fn snapshot(&self) -> Result<AppSnapshot, DesktopError> {
        let Some(profile) = self.profile_row()? else {
            return Ok(AppSnapshot {
                ready: false,
                profile: None,
                contacts: Vec::new(),
                messages: Vec::new(),
                groups: Vec::new(),
                group_messages: Vec::new(),
                temporary_connections: Vec::new(),
                temporary_messages: Vec::new(),
                stickers: Vec::new(),
                attachment_transfers: Vec::new(),
            });
        };
        let keys = self.load_device_keys()?;
        let invite_uri = Invite::new(&keys, Some(profile.relay_url.clone()), None)?.to_uri()?;
        let storage_key = self.load_storage_key()?;
        let contacts = self.contact_summaries(&storage_key)?;
        let messages = self.message_views(&storage_key)?;
        let groups = self.group_summaries(&storage_key)?;
        let group_messages = self.group_message_views(&storage_key)?;
        let temporary_connections = self.temporary_connection_summaries(&storage_key)?;
        let temporary_messages = self.temporary_message_views(&storage_key)?;
        let stickers = self.sticker_items()?;
        let attachment_transfers = self.attachment_transfers()?;
        Ok(AppSnapshot {
            ready: true,
            profile: Some(AppProfile {
                display_name: profile.display_name,
                account_id: keys.account_id.to_string(),
                device_id: keys.device_id.to_string(),
                relay_url: profile.relay_url,
                invite_uri,
            }),
            contacts,
            messages,
            groups,
            group_messages,
            temporary_connections,
            temporary_messages,
            stickers,
            attachment_transfers,
        })
    }

    pub async fn update_relay(
        data_dir: impl AsRef<Path>,
        relay_url: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        let storage_key = runtime.load_storage_key()?;
        let relay_url = encrypt_text(&storage_key, relay_url)?;
        runtime.conn.execute(
            "UPDATE profile SET relay_url = ?1, updated_at_unix = ?2 WHERE id = ?3",
            params![relay_url, now_unix(), PROFILE_ID],
        )?;
        runtime.register_current_device().await?;
        runtime.snapshot()
    }

    pub fn invite(data_dir: impl AsRef<Path>) -> Result<InviteResponse, DesktopError> {
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        Ok(InviteResponse {
            invite_uri: Invite::new(&keys, Some(profile.relay_url), None)?.to_uri()?,
        })
    }

    pub fn temporary_invite(
        data_dir: impl AsRef<Path>,
    ) -> Result<TemporaryInviteResponse, DesktopError> {
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        let expires_unix = now_unix() + TEMP_INVITE_TTL_SECS;
        Ok(TemporaryInviteResponse {
            invite_uri: Invite::temporary(&keys, Some(profile.relay_url), Some(expires_unix))?
                .to_uri()?,
            expires_unix,
        })
    }

    pub fn preview_invite(
        data_dir: impl AsRef<Path>,
        invite_text: &str,
    ) -> Result<InvitePreview, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        runtime.preview_invite_inner(invite_text)
    }

    pub fn add_contact(
        data_dir: impl AsRef<Path>,
        display_name: &str,
        invite_uri: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        let preview = runtime.preview_invite_inner(invite_uri)?;
        if preview.temporary {
            return Err(DesktopError::InvalidInvite);
        }
        let display_name = display_name.trim();
        let display_name = if display_name.is_empty() {
            preview.suggested_display_name.as_str()
        } else {
            display_name
        };
        runtime.add_contact_inner(display_name, &preview.normalized_invite_uri)?;
        runtime.snapshot()
    }

    pub fn update_contact_display_name(
        data_dir: impl AsRef<Path>,
        contact_id: &str,
        display_name: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err(DesktopError::EmptyContactName);
        }
        let updated = runtime.conn.execute(
            "UPDATE contacts SET display_name = ?1, updated_at_unix = ?2 WHERE id = ?3",
            params![display_name, now_unix(), contact_id],
        )?;
        if updated == 0 {
            return Err(DesktopError::ContactNotFound);
        }
        runtime.snapshot()
    }

    pub fn delete_contact(
        data_dir: impl AsRef<Path>,
        contact_id: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        runtime.delete_contact_inner(contact_id)?;
        runtime.snapshot()
    }

    pub fn import_sticker(
        data_dir: impl AsRef<Path>,
        file_path: &str,
        display_name: &str,
    ) -> Result<ImportStickerResponse, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        let sticker = runtime.import_sticker_inner(file_path, display_name)?;
        Ok(ImportStickerResponse {
            sticker,
            snapshot: runtime.snapshot()?,
        })
    }

    pub fn delete_sticker(
        data_dir: impl AsRef<Path>,
        sticker_id: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        runtime.delete_sticker_inner(sticker_id)?;
        runtime.snapshot()
    }

    pub fn start_temporary_connection(
        data_dir: impl AsRef<Path>,
        invite_uri: &str,
    ) -> Result<TemporaryStartResponse, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        let preview = runtime.preview_invite_inner(invite_uri)?;
        if !preview.temporary {
            return Err(DesktopError::InvalidInvite);
        }
        let connection =
            runtime.create_or_update_temporary_connection(&preview.normalized_invite_uri)?;
        Ok(TemporaryStartResponse {
            connection_id: connection.id,
            snapshot: runtime.snapshot()?,
        })
    }

    pub async fn send_temporary_message(
        data_dir: impl AsRef<Path>,
        connection_id: &str,
        body: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        if body.trim().is_empty() {
            return Err(DesktopError::EmptyMessage);
        }
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        let storage_key = runtime.load_storage_key()?;
        let connection = runtime
            .temporary_connection(connection_id)?
            .ok_or(DesktopError::ContactNotFound)?;
        if connection.expires_unix <= now_unix() {
            runtime.delete_temporary_connection(connection_id)?;
            return Err(DesktopError::ExpiredInvite);
        }
        let relay = RelayClient::new(profile.relay_url);
        relay.register_device(&keys).await?;

        let mut session = runtime.load_temporary_session(&connection)?;
        let initial = if session.is_some() {
            None
        } else if let Some(invite_uri) = &connection.invite_uri {
            let invite = Invite::from_uri(invite_uri)?;
            validate_invite_for_local_device(&invite, &keys)?;
            let (initial, created_session) =
                start_session_as_initiator(&keys, &invite.bundle, CipherSuite::default())?;
            session = Some(created_session);
            Some(initial)
        } else {
            None
        };
        let mut session = session.ok_or(DesktopError::ContactNotFound)?;
        let wire = session.encrypt(PlainMessage {
            sent_at_unix: now_unix(),
            body: body.to_string(),
        })?;
        let envelope = RelayEnvelope {
            temporary: true,
            initial,
            wire,
        };
        let payload = serde_json::to_vec(&envelope)?;
        let frame = TransportFrame::protect(&payload, &padding_profile(payload.len()))?;
        let relay_ciphertext = serde_json::to_vec(&frame)?;
        let sent = relay
            .send(
                &keys,
                SendRequest {
                    sender_account_id: Some(keys.account_id),
                    sender_device_id: Some(keys.device_id),
                    to_account_id: session.remote_identity.account_id,
                    to_device_id: session.remote_identity.device_id,
                    transport_kind: TransportKind::WebSocketTls,
                    sealed_sender: None,
                    ciphertext: relay_ciphertext.clone(),
                    expires_unix: Some(now_unix() + TEMP_MESSAGE_TTL_SECS),
                    auth: None,
                },
            )
            .await?;
        runtime.save_temporary_session(&connection.id, &session)?;
        runtime.insert_temporary_message(
            &connection.id,
            MessageDirection::Outgoing,
            body,
            MessageStatus::Sent,
            Some(relay_ciphertext),
            Some(sent.id.to_string()),
            &storage_key,
        )?;
        runtime.snapshot()
    }

    pub fn end_temporary_connection(
        data_dir: impl AsRef<Path>,
        connection_id: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        runtime.delete_temporary_connection(connection_id)?;
        runtime.snapshot()
    }

    pub async fn send_message(
        data_dir: impl AsRef<Path>,
        contact_id: &str,
        body: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        if body.trim().is_empty() {
            return Err(DesktopError::EmptyMessage);
        }
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        let storage_key = runtime.load_storage_key()?;
        let contact = runtime
            .contact(contact_id)?
            .ok_or(DesktopError::ContactNotFound)?;
        let (relay_ciphertext, remote_message_id) = runtime
            .send_contact_plaintext(
                &profile,
                &keys,
                &contact,
                body,
                Some(now_unix() + 7 * 24 * 60 * 60),
            )
            .await?;
        runtime.insert_message(
            contact_id,
            MessageDirection::Outgoing,
            body,
            MessageStatus::Sent,
            Some(relay_ciphertext),
            Some(remote_message_id),
            &storage_key,
        )?;
        runtime.snapshot()
    }

    pub async fn send_burn_message(
        data_dir: impl AsRef<Path>,
        thread_kind: &str,
        thread_id: &str,
        body: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        if body.trim().is_empty() {
            return Err(DesktopError::EmptyMessage);
        }
        let runtime = Self::open(data_dir)?;
        let payload = encode_wire_content(&WireContent {
            kind: "burn".to_string(),
            text: Some(body.to_string()),
            burn_id: Some(Uuid::new_v4().to_string()),
            destroyed: Some(false),
            target_burn_id: None,
            attachment: None,
            attachment_id: None,
            file_name: None,
            mime_type: None,
            size_bytes: None,
            sha256: None,
            chunk_index: None,
            total_chunks: None,
            data_base64: None,
        })?;
        runtime
            .send_thread_payload(
                ThreadKind::from_str(thread_kind)
                    .ok_or(DesktopError::InvalidData("invalid thread kind".to_string()))?,
                thread_id,
                &payload,
                true,
            )
            .await?;
        runtime.snapshot()
    }

    pub async fn open_burn_message(
        data_dir: impl AsRef<Path>,
        thread_kind: &str,
        thread_id: &str,
        message_id: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        let kind = ThreadKind::from_str(thread_kind)
            .ok_or_else(|| DesktopError::InvalidData("invalid thread kind".to_string()))?;
        if let Some(burn_id) = runtime.destroy_local_burn_message(kind, message_id)? {
            let payload = encode_wire_content(&WireContent {
                kind: "destroy".to_string(),
                text: None,
                burn_id: None,
                destroyed: None,
                target_burn_id: Some(burn_id),
                attachment: None,
                attachment_id: None,
                file_name: None,
                mime_type: None,
                size_bytes: None,
                sha256: None,
                chunk_index: None,
                total_chunks: None,
                data_base64: None,
            })?;
            runtime
                .send_thread_payload(kind, thread_id, &payload, false)
                .await?;
        }
        runtime.snapshot()
    }

    pub async fn send_attachment(
        data_dir: impl AsRef<Path>,
        thread_kind: &str,
        thread_id: &str,
        file_path: &str,
        kind: &str,
    ) -> Result<SendAttachmentResponse, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        let thread_kind = ThreadKind::from_str(thread_kind)
            .ok_or_else(|| DesktopError::InvalidData("invalid thread kind".to_string()))?;
        let attachment_id = runtime
            .send_attachment_inner(thread_kind, thread_id, Path::new(file_path), kind)
            .await?;
        Ok(SendAttachmentResponse {
            attachment_id,
            snapshot: runtime.snapshot()?,
        })
    }

    pub async fn create_group(
        data_dir: impl AsRef<Path>,
        display_name: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err(DesktopError::EmptyGroupName);
        }
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        let storage_key = runtime.load_storage_key()?;
        let group = GroupState::create(display_name, profile.display_name, keys.public_identity())?;
        runtime.save_group_state(&group, &storage_key)?;
        runtime.snapshot()
    }

    pub async fn add_group_member(
        data_dir: impl AsRef<Path>,
        group_id: &str,
        contact_id: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        let storage_key = runtime.load_storage_key()?;
        let mut group = runtime
            .load_group_state(group_id, &storage_key)?
            .ok_or(DesktopError::GroupNotFound)?;
        let contact = runtime
            .contact(contact_id)?
            .ok_or(DesktopError::ContactNotFound)?;
        let remote: PublicDeviceIdentity = serde_json::from_str(&contact.remote_identity_json)?;
        let welcome = group.add_member(contact.display_name.clone(), remote)?;
        runtime.save_group_state(&group, &storage_key)?;
        let control = GroupControlMessage::Welcome(welcome);
        let body = encode_group_control(&control)?;
        let _ = runtime
            .send_contact_plaintext(
                &profile,
                &keys,
                &contact,
                &body,
                Some(now_unix() + 7 * 24 * 60 * 60),
            )
            .await?;
        runtime.snapshot()
    }

    pub async fn send_group_message(
        data_dir: impl AsRef<Path>,
        group_id: &str,
        body: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        if body.trim().is_empty() {
            return Err(DesktopError::EmptyMessage);
        }
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        let storage_key = runtime.load_storage_key()?;
        let group = runtime
            .load_group_state(group_id, &storage_key)?
            .ok_or(DesktopError::GroupNotFound)?;
        let relay = RelayClient::new(profile.relay_url);
        relay.register_device(&keys).await?;
        let wire = group.encrypt_message(
            &keys.public_identity(),
            GroupPlainMessage {
                sent_at_unix: now_unix(),
                body: body.to_string(),
            },
        )?;
        let envelope = group.transport_envelope(wire);
        let payload = serde_json::to_vec(&envelope)?;
        let frame = TransportFrame::protect(&payload, &padding_profile(payload.len()))?;
        let relay_ciphertext = serde_json::to_vec(&frame)?;
        let mut remote_message_ids = Vec::new();
        for member in &group.members {
            if member.identity.device_id == keys.device_id {
                continue;
            }
            let sent = relay
                .send(
                    &keys,
                    SendRequest {
                        sender_account_id: Some(keys.account_id),
                        sender_device_id: Some(keys.device_id),
                        to_account_id: member.identity.account_id,
                        to_device_id: member.identity.device_id,
                        transport_kind: TransportKind::WebSocketTls,
                        sealed_sender: None,
                        ciphertext: relay_ciphertext.clone(),
                        expires_unix: Some(now_unix() + 7 * 24 * 60 * 60),
                        auth: None,
                    },
                )
                .await?;
            remote_message_ids.push(sent.id.to_string());
        }
        runtime.insert_group_message(
            group_id,
            keys.device_id,
            "You",
            MessageDirection::Outgoing,
            body,
            MessageStatus::Sent,
            Some(relay_ciphertext),
            remote_message_ids.first().cloned(),
            &storage_key,
        )?;
        runtime.snapshot()
    }

    pub async fn register_push_token(
        data_dir: impl AsRef<Path>,
        token: &str,
        platform: ApnsPlatform,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        RelayClient::new(profile.relay_url)
            .register_apns_token(&keys, token, platform)
            .await?;
        runtime.snapshot()
    }

    pub async fn receive(data_dir: impl AsRef<Path>) -> Result<ReceiveReport, DesktopError> {
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let mut keys = runtime.load_device_keys()?;
        let storage_key = runtime.load_storage_key()?;
        let relay = RelayClient::new(&profile.relay_url);
        relay.register_device(&keys).await?;
        runtime.apply_receipts(relay.drain_receipts(&keys).await?)?;
        let queued = relay.drain(&keys).await?;
        let mut received_count = 0usize;
        let mut keys_changed = false;

        for item in queued {
            let frame: TransportFrame = serde_json::from_slice(&item.ciphertext)?;
            let exposed = frame.expose()?;
            if let Ok(envelope) = serde_json::from_slice::<GroupTransportEnvelope>(&exposed) {
                if envelope.kind == GROUP_TRANSPORT_KIND {
                    if let Some(group) =
                        runtime.load_group_state(&envelope.group_id.to_string(), &storage_key)?
                    {
                        let plain = group.decrypt_message(&envelope.wire)?;
                        let sender_display_name = group
                            .members
                            .iter()
                            .find(|member| {
                                member.identity.device_id == envelope.wire.sender_device_id
                            })
                            .map(|member| member.display_name.as_str())
                            .unwrap_or("Group member");
                        if !runtime.handle_incoming_payload(
                            ThreadKind::Group,
                            &envelope.group_id.to_string(),
                            &plain.body,
                            Some(envelope.wire.sender_device_id),
                            Some(sender_display_name),
                            &storage_key,
                        )? {
                            runtime.insert_group_message(
                                &envelope.group_id.to_string(),
                                envelope.wire.sender_device_id,
                                sender_display_name,
                                MessageDirection::Incoming,
                                &plain.body,
                                MessageStatus::Received,
                                Some(item.ciphertext),
                                Some(item.id.to_string()),
                                &storage_key,
                            )?;
                        }
                        if let (Some(sender_account_id), Some(sender_device_id)) =
                            (item.sender_account_id, item.sender_device_id)
                        {
                            let _ = relay
                                .send_receipt(
                                    &keys,
                                    ReceiptRequest {
                                        message_id: item.id,
                                        from_account_id: keys.account_id,
                                        from_device_id: keys.device_id,
                                        to_account_id: sender_account_id,
                                        to_device_id: sender_device_id,
                                        kind: ReceiptKind::Read,
                                        at_unix: now_unix(),
                                        auth: None,
                                    },
                                )
                                .await;
                        }
                        received_count += 1;
                    }
                    continue;
                }
            }
            let envelope: RelayEnvelope = serde_json::from_slice(&exposed)?;
            let remote_device_id = envelope.wire.sender_device_id;
            let mut contact = if envelope.temporary {
                None
            } else {
                runtime.contact_by_device(&remote_device_id.to_string())?
            };
            let mut temporary_connection = if envelope.temporary {
                runtime.temporary_connection_by_device(&remote_device_id.to_string())?
            } else {
                None
            };
            let mut session = if let Some(contact) = &contact {
                runtime.load_session(&contact.id)?
            } else if let Some(connection) = &temporary_connection {
                runtime.load_temporary_session(connection)?
            } else {
                None
            };
            let new_incoming_session = session.is_none();
            if session.is_none() {
                let initial = envelope
                    .initial
                    .as_ref()
                    .ok_or(DesktopError::ContactNotFound)?;
                let accepted = accept_session_as_responder_consuming_prekey(&mut keys, initial)?;
                if accepted.remote_identity.device_id != remote_device_id {
                    return Err(DesktopError::InvalidData(
                        "initial identity does not match message sender".to_string(),
                    ));
                }
                runtime.save_device_keys(&keys)?;
                keys_changed = true;
                session = Some(accepted);
            }
            let mut session = session.ok_or(DesktopError::ContactNotFound)?;
            let plain = session.decrypt(&envelope.wire)?;
            let handled_control = runtime.handle_group_control(&plain.body, &storage_key)?;
            if contact.is_none() && !envelope.temporary && new_incoming_session {
                contact = Some(runtime.create_incoming_contact(&keys, &session.remote_identity)?);
            }
            if let Some(contact) = contact {
                runtime.save_session(&contact.id, &session)?;
                if !handled_control
                    && !runtime.handle_incoming_payload(
                        ThreadKind::Contact,
                        &contact.id,
                        &plain.body,
                        None,
                        None,
                        &storage_key,
                    )?
                {
                    runtime.insert_message(
                        &contact.id,
                        MessageDirection::Incoming,
                        &plain.body,
                        MessageStatus::Received,
                        Some(item.ciphertext),
                        Some(item.id.to_string()),
                        &storage_key,
                    )?;
                }
            } else {
                if temporary_connection.is_none() && new_incoming_session {
                    temporary_connection =
                        Some(runtime.create_incoming_temporary_connection(
                            &keys,
                            &session.remote_identity,
                        )?);
                }
                let connection = temporary_connection.ok_or(DesktopError::ContactNotFound)?;
                runtime.save_temporary_session(&connection.id, &session)?;
                if !handled_control
                    && !runtime.handle_incoming_payload(
                        ThreadKind::Temporary,
                        &connection.id,
                        &plain.body,
                        None,
                        None,
                        &storage_key,
                    )?
                {
                    runtime.insert_temporary_message(
                        &connection.id,
                        MessageDirection::Incoming,
                        &plain.body,
                        MessageStatus::Received,
                        Some(item.ciphertext),
                        Some(item.id.to_string()),
                        &storage_key,
                    )?;
                }
            }
            if let (Some(sender_account_id), Some(sender_device_id)) =
                (item.sender_account_id, item.sender_device_id)
            {
                let _ = relay
                    .send_receipt(
                        &keys,
                        ReceiptRequest {
                            message_id: item.id,
                            from_account_id: keys.account_id,
                            from_device_id: keys.device_id,
                            to_account_id: sender_account_id,
                            to_device_id: sender_device_id,
                            kind: ReceiptKind::Read,
                            at_unix: now_unix(),
                            auth: None,
                        },
                    )
                    .await;
            }
            received_count += 1;
        }

        if keys_changed {
            runtime.save_device_keys(&keys)?;
            relay.register_device(&keys).await?;
        }

        Ok(ReceiveReport {
            received_count,
            snapshot: runtime.snapshot()?,
        })
    }

    pub async fn p2p_probe(
        data_dir: impl AsRef<Path>,
    ) -> Result<secure_chat_client::P2pProbeReport, DesktopError> {
        let runtime = Self::open(data_dir)?;
        let profile = runtime.ensure_profile()?;
        let keys = runtime.load_device_keys()?;
        Ok(secure_chat_client::run_p2p_probe_against(&profile.relay_url, &keys).await?)
    }

    async fn bootstrap_profile(
        &self,
        display_name: &str,
        relay_url: &str,
    ) -> Result<(), DesktopError> {
        if self.profile_row()?.is_none() {
            let keys = DeviceKeyMaterial::generate(16);
            let storage_key = random_bytes::<32>();
            self.save_device_keys(&keys)?;
            self.save_storage_key(&storage_key)?;
            let relay_url = encrypt_text(&storage_key, relay_url)?;
            self.conn.execute(
                "INSERT INTO profile (id, display_name, relay_url, created_at_unix, updated_at_unix)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![PROFILE_ID, display_name, relay_url, now_unix()],
            )?;
        } else {
            let storage_key = self.load_storage_key()?;
            let relay_url = encrypt_text(&storage_key, relay_url)?;
            self.conn.execute(
                "UPDATE profile SET display_name = ?1, relay_url = ?2, updated_at_unix = ?3 WHERE id = ?4",
                params![display_name, relay_url, now_unix(), PROFILE_ID],
            )?;
        }
        self.register_current_device().await
    }

    async fn register_current_device(&self) -> Result<(), DesktopError> {
        let profile = self.ensure_profile()?;
        let keys = self.load_device_keys()?;
        RelayClient::new(profile.relay_url)
            .register_device(&keys)
            .await?;
        Ok(())
    }

    async fn send_contact_plaintext(
        &self,
        profile: &ProfileRow,
        keys: &DeviceKeyMaterial,
        contact: &ContactRecord,
        body: &str,
        expires_unix: Option<u64>,
    ) -> Result<(Vec<u8>, String), DesktopError> {
        let relay = RelayClient::new(&profile.relay_url);
        relay.register_device(keys).await?;
        let mut session = self.load_session(&contact.id)?;
        let initial = if session.is_some() {
            None
        } else if let Some(invite_uri) = &contact.invite_uri {
            let invite = Invite::from_uri(invite_uri)?;
            let (initial, created_session) =
                start_session_as_initiator(keys, &invite.bundle, CipherSuite::default())?;
            session = Some(created_session);
            Some(initial)
        } else {
            None
        };
        let mut session = session.ok_or(DesktopError::ContactNotFound)?;
        let wire = session.encrypt(PlainMessage {
            sent_at_unix: now_unix(),
            body: body.to_string(),
        })?;
        let envelope = RelayEnvelope {
            temporary: false,
            initial,
            wire,
        };
        let payload = serde_json::to_vec(&envelope)?;
        let frame = TransportFrame::protect(&payload, &padding_profile(payload.len()))?;
        let relay_ciphertext = serde_json::to_vec(&frame)?;
        let sent = relay
            .send(
                keys,
                SendRequest {
                    sender_account_id: Some(keys.account_id),
                    sender_device_id: Some(keys.device_id),
                    to_account_id: session.remote_identity.account_id,
                    to_device_id: session.remote_identity.device_id,
                    transport_kind: TransportKind::WebSocketTls,
                    sealed_sender: None,
                    ciphertext: relay_ciphertext.clone(),
                    expires_unix,
                    auth: None,
                },
            )
            .await?;
        self.save_session(&contact.id, &session)?;
        Ok((relay_ciphertext, sent.id.to_string()))
    }

    async fn send_temporary_plaintext(
        &self,
        profile: &ProfileRow,
        keys: &DeviceKeyMaterial,
        connection: &TemporaryConnectionRecord,
        body: &str,
    ) -> Result<(Vec<u8>, String), DesktopError> {
        if connection.expires_unix <= now_unix() {
            self.delete_temporary_connection(&connection.id)?;
            return Err(DesktopError::ExpiredInvite);
        }
        let relay = RelayClient::new(&profile.relay_url);
        relay.register_device(keys).await?;
        let mut session = self.load_temporary_session(connection)?;
        let initial = if session.is_some() {
            None
        } else if let Some(invite_uri) = &connection.invite_uri {
            let invite = Invite::from_uri(invite_uri)?;
            validate_invite_for_local_device(&invite, keys)?;
            let (initial, created_session) =
                start_session_as_initiator(keys, &invite.bundle, CipherSuite::default())?;
            session = Some(created_session);
            Some(initial)
        } else {
            None
        };
        let mut session = session.ok_or(DesktopError::ContactNotFound)?;
        let wire = session.encrypt(PlainMessage {
            sent_at_unix: now_unix(),
            body: body.to_string(),
        })?;
        let envelope = RelayEnvelope {
            temporary: true,
            initial,
            wire,
        };
        let payload = serde_json::to_vec(&envelope)?;
        let frame = TransportFrame::protect(&payload, &padding_profile(payload.len()))?;
        let relay_ciphertext = serde_json::to_vec(&frame)?;
        let sent = relay
            .send(
                keys,
                SendRequest {
                    sender_account_id: Some(keys.account_id),
                    sender_device_id: Some(keys.device_id),
                    to_account_id: session.remote_identity.account_id,
                    to_device_id: session.remote_identity.device_id,
                    transport_kind: TransportKind::WebSocketTls,
                    sealed_sender: None,
                    ciphertext: relay_ciphertext.clone(),
                    expires_unix: Some(now_unix() + TEMP_MESSAGE_TTL_SECS),
                    auth: None,
                },
            )
            .await?;
        self.save_temporary_session(&connection.id, &session)?;
        Ok((relay_ciphertext, sent.id.to_string()))
    }

    async fn send_group_plaintext(
        &self,
        profile: &ProfileRow,
        keys: &DeviceKeyMaterial,
        group: &GroupState,
        body: &str,
    ) -> Result<(Vec<u8>, Option<String>), DesktopError> {
        let relay = RelayClient::new(&profile.relay_url);
        relay.register_device(keys).await?;
        let wire = group.encrypt_message(
            &keys.public_identity(),
            GroupPlainMessage {
                sent_at_unix: now_unix(),
                body: body.to_string(),
            },
        )?;
        let envelope = group.transport_envelope(wire);
        let payload = serde_json::to_vec(&envelope)?;
        let frame = TransportFrame::protect(&payload, &padding_profile(payload.len()))?;
        let relay_ciphertext = serde_json::to_vec(&frame)?;
        let mut remote_message_ids = Vec::new();
        for member in &group.members {
            if member.identity.device_id == keys.device_id {
                continue;
            }
            let sent = relay
                .send(
                    keys,
                    SendRequest {
                        sender_account_id: Some(keys.account_id),
                        sender_device_id: Some(keys.device_id),
                        to_account_id: member.identity.account_id,
                        to_device_id: member.identity.device_id,
                        transport_kind: TransportKind::WebSocketTls,
                        sealed_sender: None,
                        ciphertext: relay_ciphertext.clone(),
                        expires_unix: Some(now_unix() + 7 * 24 * 60 * 60),
                        auth: None,
                    },
                )
                .await?;
            remote_message_ids.push(sent.id.to_string());
        }
        Ok((relay_ciphertext, remote_message_ids.first().cloned()))
    }

    async fn send_thread_payload(
        &self,
        thread_kind: ThreadKind,
        thread_id: &str,
        body: &str,
        insert_visible: bool,
    ) -> Result<(), DesktopError> {
        let profile = self.ensure_profile()?;
        let keys = self.load_device_keys()?;
        let storage_key = self.load_storage_key()?;
        match thread_kind {
            ThreadKind::Contact => {
                let contact = self
                    .contact(thread_id)?
                    .ok_or(DesktopError::ContactNotFound)?;
                let (relay_ciphertext, remote_message_id) = self
                    .send_contact_plaintext(
                        &profile,
                        &keys,
                        &contact,
                        body,
                        Some(now_unix() + 7 * 24 * 60 * 60),
                    )
                    .await?;
                if insert_visible {
                    self.insert_message(
                        thread_id,
                        MessageDirection::Outgoing,
                        body,
                        MessageStatus::Sent,
                        Some(relay_ciphertext),
                        Some(remote_message_id),
                        &storage_key,
                    )?;
                }
            }
            ThreadKind::Temporary => {
                let connection = self
                    .temporary_connection(thread_id)?
                    .ok_or(DesktopError::ContactNotFound)?;
                let (relay_ciphertext, remote_message_id) = self
                    .send_temporary_plaintext(&profile, &keys, &connection, body)
                    .await?;
                if insert_visible {
                    self.insert_temporary_message(
                        thread_id,
                        MessageDirection::Outgoing,
                        body,
                        MessageStatus::Sent,
                        Some(relay_ciphertext),
                        Some(remote_message_id),
                        &storage_key,
                    )?;
                }
            }
            ThreadKind::Group => {
                let group = self
                    .load_group_state(thread_id, &storage_key)?
                    .ok_or(DesktopError::GroupNotFound)?;
                let (relay_ciphertext, remote_message_id) = self
                    .send_group_plaintext(&profile, &keys, &group, body)
                    .await?;
                if insert_visible {
                    self.insert_group_message(
                        thread_id,
                        keys.device_id,
                        "You",
                        MessageDirection::Outgoing,
                        body,
                        MessageStatus::Sent,
                        Some(relay_ciphertext),
                        remote_message_id,
                        &storage_key,
                    )?;
                }
            }
        }
        Ok(())
    }

    async fn send_attachment_inner(
        &self,
        thread_kind: ThreadKind,
        thread_id: &str,
        source_path: &Path,
        kind: &str,
    ) -> Result<String, DesktopError> {
        if !source_path.is_file() {
            return Err(DesktopError::FileNotFound);
        }
        let metadata = fs::metadata(source_path)?;
        let size_bytes = metadata.len();
        let attachment_id = Uuid::new_v4().to_string();
        let file_name = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .map(sanitize_file_name)
            .unwrap_or_else(|| format!("attachment-{attachment_id}"));
        let mime_type = guess_mime_type(&file_name);
        let sha256 = sha256_file(source_path)?;
        let total_chunks = size_bytes.max(1).div_ceil(ATTACHMENT_CHUNK_BYTES as u64);
        let mut file = fs::File::open(source_path)?;
        let mut buffer = vec![0u8; ATTACHMENT_CHUNK_BYTES];
        let mut chunk_index = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 && chunk_index > 0 {
                break;
            }
            let payload = encode_wire_content(&WireContent {
                kind: normalize_attachment_kind(kind).to_string(),
                text: None,
                burn_id: None,
                destroyed: None,
                target_burn_id: None,
                attachment: None,
                attachment_id: Some(attachment_id.clone()),
                file_name: Some(file_name.clone()),
                mime_type: Some(mime_type.clone()),
                size_bytes: Some(size_bytes),
                sha256: Some(sha256.clone()),
                chunk_index: Some(chunk_index),
                total_chunks: Some(total_chunks),
                data_base64: Some(STANDARD.encode(&buffer[..read])),
            })?;
            self.send_thread_payload(thread_kind, thread_id, &payload, false)
                .await?;
            chunk_index += 1;
            if read == 0 || chunk_index >= total_chunks {
                break;
            }
        }
        let local_path = self.copy_attachment_file(&attachment_id, source_path, &file_name)?;
        let attachment = AttachmentView {
            id: attachment_id.clone(),
            kind: normalize_attachment_kind(kind).to_string(),
            file_name,
            mime_type,
            size_bytes,
            sha256,
            local_path: Some(local_path.to_string_lossy().to_string()),
            transfer_status: "complete".to_string(),
        };
        self.record_attachment(thread_kind, thread_id, &attachment, "complete")?;
        let body = encode_wire_content(&WireContent {
            kind: attachment.kind.clone(),
            text: None,
            burn_id: None,
            destroyed: None,
            target_burn_id: None,
            attachment: Some(attachment),
            attachment_id: None,
            file_name: None,
            mime_type: None,
            size_bytes: None,
            sha256: None,
            chunk_index: None,
            total_chunks: None,
            data_base64: None,
        })?;
        self.insert_local_thread_message(
            thread_kind,
            thread_id,
            MessageDirection::Outgoing,
            &body,
        )?;
        Ok(attachment_id)
    }

    fn insert_local_thread_message(
        &self,
        thread_kind: ThreadKind,
        thread_id: &str,
        direction: MessageDirection,
        body: &str,
    ) -> Result<(), DesktopError> {
        let storage_key = self.load_storage_key()?;
        match thread_kind {
            ThreadKind::Contact => self.insert_message(
                thread_id,
                direction,
                body,
                if direction == MessageDirection::Incoming {
                    MessageStatus::Received
                } else {
                    MessageStatus::Sent
                },
                None,
                None,
                &storage_key,
            ),
            ThreadKind::Temporary => self.insert_temporary_message(
                thread_id,
                direction,
                body,
                if direction == MessageDirection::Incoming {
                    MessageStatus::Received
                } else {
                    MessageStatus::Sent
                },
                None,
                None,
                &storage_key,
            ),
            ThreadKind::Group => {
                let keys = self.load_device_keys()?;
                self.insert_group_message(
                    thread_id,
                    keys.device_id,
                    if direction == MessageDirection::Outgoing {
                        "You"
                    } else {
                        "Group member"
                    },
                    direction,
                    body,
                    if direction == MessageDirection::Incoming {
                        MessageStatus::Received
                    } else {
                        MessageStatus::Sent
                    },
                    None,
                    None,
                    &storage_key,
                )
            }
        }
    }

    fn migrate(&self) -> Result<(), DesktopError> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS profile (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                relay_url TEXT NOT NULL,
                created_at_unix INTEGER NOT NULL,
                updated_at_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS contacts (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                account_id TEXT NOT NULL,
                device_id TEXT NOT NULL UNIQUE,
                invite_uri TEXT,
                safety_number TEXT NOT NULL,
                verified INTEGER NOT NULL DEFAULT 0,
                remote_identity_json TEXT NOT NULL,
                created_at_unix INTEGER NOT NULL,
                updated_at_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                contact_id TEXT PRIMARY KEY,
                remote_device_id TEXT NOT NULL UNIQUE,
                session_nonce BLOB NOT NULL,
                session_ciphertext BLOB NOT NULL,
                updated_at_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                contact_id TEXT NOT NULL,
                direction TEXT NOT NULL,
                body_nonce BLOB NOT NULL,
                body_ciphertext BLOB NOT NULL,
                relay_ciphertext BLOB,
                remote_message_id TEXT,
                status TEXT NOT NULL,
                sent_at_unix INTEGER NOT NULL,
                received_at_unix INTEGER
            );
            CREATE TABLE IF NOT EXISTS groups (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                secret_nonce BLOB NOT NULL,
                secret_ciphertext BLOB NOT NULL,
                created_at_unix INTEGER NOT NULL,
                updated_at_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS group_members (
                group_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                display_name TEXT NOT NULL,
                identity_json TEXT NOT NULL,
                PRIMARY KEY(group_id, device_id)
            );
            CREATE TABLE IF NOT EXISTS group_messages (
                id TEXT PRIMARY KEY,
                group_id TEXT NOT NULL,
                sender_device_id TEXT NOT NULL,
                sender_display_name TEXT NOT NULL,
                direction TEXT NOT NULL,
                body_nonce BLOB NOT NULL,
                body_ciphertext BLOB NOT NULL,
                relay_ciphertext BLOB,
                remote_message_id TEXT,
                status TEXT NOT NULL,
                sent_at_unix INTEGER NOT NULL,
                received_at_unix INTEGER
            );
            CREATE TABLE IF NOT EXISTS temporary_connections (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                account_id TEXT NOT NULL,
                device_id TEXT NOT NULL UNIQUE,
                invite_uri TEXT,
                safety_number TEXT NOT NULL,
                remote_identity_json TEXT NOT NULL,
                session_nonce BLOB,
                session_ciphertext BLOB,
                created_at_unix INTEGER NOT NULL,
                updated_at_unix INTEGER NOT NULL,
                expires_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS temporary_messages (
                id TEXT PRIMARY KEY,
                connection_id TEXT NOT NULL,
                direction TEXT NOT NULL,
                body_nonce BLOB NOT NULL,
                body_ciphertext BLOB NOT NULL,
                relay_ciphertext BLOB,
                remote_message_id TEXT,
                status TEXT NOT NULL,
                sent_at_unix INTEGER NOT NULL,
                received_at_unix INTEGER
            );
            CREATE TABLE IF NOT EXISTS attachments (
                id TEXT PRIMARY KEY,
                thread_kind TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                sha256 TEXT NOT NULL,
                local_path TEXT,
                status TEXT NOT NULL,
                created_at_unix INTEGER NOT NULL,
                updated_at_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS attachment_chunks (
                attachment_id TEXT NOT NULL,
                thread_kind TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                sha256 TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                total_chunks INTEGER NOT NULL,
                chunk_path TEXT NOT NULL,
                created_at_unix INTEGER NOT NULL,
                PRIMARY KEY(attachment_id, chunk_index)
            );
            CREATE TABLE IF NOT EXISTS sticker_packs (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                created_at_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS stickers (
                id TEXT PRIMARY KEY,
                pack_id TEXT,
                display_name TEXT NOT NULL,
                file_name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                sha256 TEXT NOT NULL,
                local_path TEXT NOT NULL,
                created_at_unix INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_contacts_updated ON contacts(updated_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_contact_sent ON messages(contact_id, sent_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_groups_updated ON groups(updated_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_group_messages_group_sent ON group_messages(group_id, sent_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_temporary_connections_updated ON temporary_connections(updated_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_temporary_messages_connection_sent ON temporary_messages(connection_id, sent_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_attachments_thread ON attachments(thread_kind, thread_id, updated_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_attachment_chunks_thread ON attachment_chunks(thread_kind, thread_id, attachment_id);
            CREATE INDEX IF NOT EXISTS idx_stickers_created ON stickers(created_at_unix DESC);
            "#,
        )?;
        self.ensure_sessions_schema()?;
        self.ensure_message_content_columns()?;
        self.encrypt_existing_metadata_if_possible()?;
        self.delete_expired_temporary_connections()?;
        Ok(())
    }

    fn ensure_message_content_columns(&self) -> Result<(), DesktopError> {
        for table in ["messages", "group_messages", "temporary_messages"] {
            self.ensure_column(table, "content_kind", "TEXT NOT NULL DEFAULT 'text'")?;
            self.ensure_column(table, "attachment_id", "TEXT")?;
            self.ensure_column(table, "burn_id", "TEXT")?;
            self.ensure_column(table, "burn_destroyed", "INTEGER NOT NULL DEFAULT 0")?;
        }
        Ok(())
    }

    fn ensure_column(
        &self,
        table: &str,
        column: &str,
        definition: &str,
    ) -> Result<(), DesktopError> {
        let mut statement = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|value| value == column) {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition};"
            ))?;
        }
        Ok(())
    }

    fn ensure_sessions_schema(&self) -> Result<(), DesktopError> {
        let mut statement = self.conn.prepare("PRAGMA table_info(sessions)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|column| column == "session_ciphertext") {
            self.conn.execute_batch(
                r#"
                DROP TABLE IF EXISTS sessions;
                CREATE TABLE sessions (
                    contact_id TEXT PRIMARY KEY,
                    remote_device_id TEXT NOT NULL UNIQUE,
                    session_nonce BLOB NOT NULL,
                    session_ciphertext BLOB NOT NULL,
                    updated_at_unix INTEGER NOT NULL
                );
                "#,
            )?;
        }
        Ok(())
    }

    fn encrypt_existing_metadata_if_possible(&self) -> Result<(), DesktopError> {
        let Ok(storage_key) = self.load_storage_key() else {
            return Ok(());
        };

        let profile_rows = {
            let mut statement = self
                .conn
                .prepare("SELECT id, relay_url FROM profile WHERE relay_url NOT LIKE ?1")?;
            let rows = statement
                .query_map(params![format!("{ENCRYPTED_TEXT_PREFIX}%")], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for (id, relay_url) in profile_rows {
            let encrypted = encrypt_text(&storage_key, &relay_url)?;
            self.conn.execute(
                "UPDATE profile SET relay_url = ?1 WHERE id = ?2",
                params![encrypted, id],
            )?;
        }

        let contact_rows = {
            let mut statement = self.conn.prepare(
                "SELECT id, invite_uri, remote_identity_json FROM contacts
                 WHERE (invite_uri IS NOT NULL AND invite_uri NOT LIKE ?1)
                    OR remote_identity_json NOT LIKE ?1",
            )?;
            let rows = statement
                .query_map(params![format!("{ENCRYPTED_TEXT_PREFIX}%")], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for (id, invite_uri, remote_identity_json) in contact_rows {
            let encrypted_invite_uri = invite_uri
                .as_deref()
                .map(|value| {
                    if is_encrypted_text(value) {
                        Ok(value.to_string())
                    } else {
                        encrypt_text(&storage_key, value)
                    }
                })
                .transpose()?;
            let encrypted_remote_identity_json = if is_encrypted_text(&remote_identity_json) {
                remote_identity_json
            } else {
                encrypt_text(&storage_key, &remote_identity_json)?
            };
            self.conn.execute(
                "UPDATE contacts SET invite_uri = ?1, remote_identity_json = ?2 WHERE id = ?3",
                params![encrypted_invite_uri, encrypted_remote_identity_json, id],
            )?;
        }

        Ok(())
    }

    fn profile_row(&self) -> Result<Option<ProfileRow>, DesktopError> {
        let row = self
            .conn
            .query_row(
                "SELECT display_name, relay_url FROM profile WHERE id = ?1",
                params![PROFILE_ID],
                |row| {
                    Ok(ProfileRow {
                        display_name: row.get(0)?,
                        relay_url: row.get(1)?,
                    })
                },
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let storage_key = self.load_storage_key()?;
        Ok(Some(ProfileRow {
            display_name: row.display_name,
            relay_url: decrypt_text_if_needed(&storage_key, &row.relay_url)?,
        }))
    }

    fn ensure_profile(&self) -> Result<ProfileRow, DesktopError> {
        self.profile_row()?.ok_or(DesktopError::MissingProfile)
    }

    fn add_contact_inner(
        &self,
        display_name: &str,
        invite_uri: &str,
    ) -> Result<ContactRecord, DesktopError> {
        let keys = self.load_device_keys()?;
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err(DesktopError::EmptyContactName);
        }
        let (invite_uri, invite) = decode_invite(invite_uri)?;
        validate_invite_for_local_device(&invite, &keys)?;
        let remote = invite.bundle.identity.clone();
        let safety = safety_number(&[keys.public_identity()], std::slice::from_ref(&remote));
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let storage_key = self.load_storage_key()?;
        let invite_uri = encrypt_text(&storage_key, &invite_uri)?;
        let remote_identity_json = encrypt_text(&storage_key, &serde_json::to_string(&remote)?)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO contacts
             (id, display_name, account_id, device_id, invite_uri, safety_number, verified, remote_identity_json, created_at_unix, updated_at_unix)
             VALUES (
                COALESCE((SELECT id FROM contacts WHERE device_id = ?4), ?1),
                ?2, ?3, ?4, ?5, ?6,
                COALESCE((SELECT verified FROM contacts WHERE device_id = ?4), 0),
                ?7,
                COALESCE((SELECT created_at_unix FROM contacts WHERE device_id = ?4), ?8),
                ?8
             )",
            params![
                id,
                display_name,
                remote.account_id.to_string(),
                remote.device_id.to_string(),
                invite_uri,
                safety.number,
                remote_identity_json,
                now
            ],
        )?;
        self.contact_by_device(&remote.device_id.to_string())?
            .ok_or(DesktopError::ContactNotFound)
    }

    fn create_incoming_contact(
        &self,
        keys: &DeviceKeyMaterial,
        remote: &PublicDeviceIdentity,
    ) -> Result<ContactRecord, DesktopError> {
        let suffix = remote
            .device_id
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        let display_name = format!("Contact {suffix}");
        let safety = safety_number(&[keys.public_identity()], std::slice::from_ref(remote));
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let storage_key = self.load_storage_key()?;
        let remote_identity_json = encrypt_text(&storage_key, &serde_json::to_string(remote)?)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO contacts
             (id, display_name, account_id, device_id, invite_uri, safety_number, verified, remote_identity_json, created_at_unix, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, 0, ?6, ?7, ?7)",
            params![
                id,
                display_name,
                remote.account_id.to_string(),
                remote.device_id.to_string(),
                safety.number,
                remote_identity_json,
                now
            ],
        )?;
        self.contact_by_device(&remote.device_id.to_string())?
            .ok_or(DesktopError::ContactNotFound)
    }

    fn preview_invite_inner(&self, invite_text: &str) -> Result<InvitePreview, DesktopError> {
        let keys = self.load_device_keys()?;
        let (normalized_invite_uri, invite) = decode_invite(invite_text)?;
        validate_invite_for_local_device(&invite, &keys)?;
        let remote = invite.bundle.identity.clone();
        let existing = self.contact_by_device(&remote.device_id.to_string())?;
        let safety = safety_number(&[keys.public_identity()], std::slice::from_ref(&remote));
        let short_device = remote
            .device_id
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        let suggested_display_name = existing
            .as_ref()
            .map(|contact| contact.display_name.clone())
            .unwrap_or_else(|| format!("Contact {short_device}"));

        Ok(InvitePreview {
            normalized_invite_uri,
            suggested_display_name,
            account_id: remote.account_id.to_string(),
            device_id: remote.device_id.to_string(),
            relay_hint: invite.relay_hint,
            expires_unix: invite.expires_unix,
            safety_number: safety.number,
            already_added: existing.is_some(),
            existing_display_name: existing
                .as_ref()
                .map(|contact| contact.display_name.clone()),
            verified: existing
                .as_ref()
                .map(|contact| contact.verified)
                .unwrap_or(false),
            temporary: invite.mode == InviteMode::Temporary,
        })
    }

    fn create_or_update_temporary_connection(
        &self,
        invite_uri: &str,
    ) -> Result<TemporaryConnectionRecord, DesktopError> {
        let keys = self.load_device_keys()?;
        let (invite_uri, invite) = decode_invite(invite_uri)?;
        validate_invite_for_local_device(&invite, &keys)?;
        if invite.mode != InviteMode::Temporary {
            return Err(DesktopError::InvalidInvite);
        }
        let remote = invite.bundle.identity.clone();
        let safety = safety_number(&[keys.public_identity()], std::slice::from_ref(&remote));
        let suffix = remote
            .device_id
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        let display_name = format!("Temporary {suffix}");
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let expires_unix = invite
            .expires_unix
            .unwrap_or(now + TEMP_CONNECTION_TTL_SECS)
            .min(now + TEMP_CONNECTION_TTL_SECS);
        let storage_key = self.load_storage_key()?;
        let invite_uri = encrypt_text(&storage_key, &invite_uri)?;
        let remote_identity_json = encrypt_text(&storage_key, &serde_json::to_string(&remote)?)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO temporary_connections
             (id, display_name, account_id, device_id, invite_uri, safety_number, remote_identity_json, session_nonce, session_ciphertext, created_at_unix, updated_at_unix, expires_unix)
             VALUES (
                COALESCE((SELECT id FROM temporary_connections WHERE device_id = ?4), ?1),
                ?2, ?3, ?4, ?5, ?6, ?7,
                COALESCE((SELECT session_nonce FROM temporary_connections WHERE device_id = ?4), NULL),
                COALESCE((SELECT session_ciphertext FROM temporary_connections WHERE device_id = ?4), NULL),
                COALESCE((SELECT created_at_unix FROM temporary_connections WHERE device_id = ?4), ?8),
                ?8, ?9
             )",
            params![
                id,
                display_name,
                remote.account_id.to_string(),
                remote.device_id.to_string(),
                invite_uri,
                safety.number,
                remote_identity_json,
                now,
                expires_unix
            ],
        )?;
        self.prune_temporary_connections()?;
        self.temporary_connection_by_device(&remote.device_id.to_string())?
            .ok_or(DesktopError::ContactNotFound)
    }

    fn create_incoming_temporary_connection(
        &self,
        keys: &DeviceKeyMaterial,
        remote: &PublicDeviceIdentity,
    ) -> Result<TemporaryConnectionRecord, DesktopError> {
        let suffix = remote
            .device_id
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        let display_name = format!("Temporary {suffix}");
        let safety = safety_number(&[keys.public_identity()], std::slice::from_ref(remote));
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let storage_key = self.load_storage_key()?;
        let remote_identity_json = encrypt_text(&storage_key, &serde_json::to_string(remote)?)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO temporary_connections
             (id, display_name, account_id, device_id, invite_uri, safety_number, remote_identity_json, session_nonce, session_ciphertext, created_at_unix, updated_at_unix, expires_unix)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, NULL, NULL, ?7, ?7, ?8)",
            params![
                id,
                display_name,
                remote.account_id.to_string(),
                remote.device_id.to_string(),
                safety.number,
                remote_identity_json,
                now,
                now + TEMP_CONNECTION_TTL_SECS
            ],
        )?;
        self.prune_temporary_connections()?;
        self.temporary_connection_by_device(&remote.device_id.to_string())?
            .ok_or(DesktopError::ContactNotFound)
    }

    fn contact(&self, id: &str) -> Result<Option<ContactRecord>, DesktopError> {
        self.contact_query("WHERE id = ?1", id)
    }

    fn contact_by_device(&self, device_id: &str) -> Result<Option<ContactRecord>, DesktopError> {
        self.contact_query("WHERE device_id = ?1", device_id)
    }

    fn contact_query(
        &self,
        predicate: &str,
        value: &str,
    ) -> Result<Option<ContactRecord>, DesktopError> {
        let sql = format!(
            "SELECT id, display_name, account_id, device_id, invite_uri, safety_number, verified, remote_identity_json, updated_at_unix
             FROM contacts {predicate}"
        );
        let row = self
            .conn
            .query_row(&sql, params![value], |row| {
                Ok(ContactRecord {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    account_id: row.get(2)?,
                    device_id: row.get(3)?,
                    invite_uri: row.get(4)?,
                    safety_number: row.get(5)?,
                    verified: row.get::<_, i64>(6)? != 0,
                    remote_identity_json: row.get(7)?,
                    updated_at_unix: row.get::<_, i64>(8)? as u64,
                })
            })
            .optional()?;
        let Some(mut row) = row else {
            return Ok(None);
        };
        let storage_key = self.load_storage_key()?;
        row.invite_uri = row
            .invite_uri
            .as_deref()
            .map(|value| decrypt_text_if_needed(&storage_key, value))
            .transpose()?;
        row.remote_identity_json = decrypt_text_if_needed(&storage_key, &row.remote_identity_json)?;
        Ok(Some(row))
    }

    fn temporary_connection(
        &self,
        id: &str,
    ) -> Result<Option<TemporaryConnectionRecord>, DesktopError> {
        self.temporary_connection_query("WHERE id = ?1", id)
    }

    fn temporary_connection_by_device(
        &self,
        device_id: &str,
    ) -> Result<Option<TemporaryConnectionRecord>, DesktopError> {
        self.temporary_connection_query("WHERE device_id = ?1", device_id)
    }

    fn temporary_connection_query(
        &self,
        predicate: &str,
        value: &str,
    ) -> Result<Option<TemporaryConnectionRecord>, DesktopError> {
        let sql = format!(
            "SELECT id, display_name, account_id, device_id, invite_uri, safety_number, remote_identity_json, session_nonce, session_ciphertext, created_at_unix, updated_at_unix, expires_unix
             FROM temporary_connections {predicate}"
        );
        let row = self
            .conn
            .query_row(&sql, params![value], |row| {
                Ok(TemporaryConnectionRecord {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    account_id: row.get(2)?,
                    device_id: row.get(3)?,
                    invite_uri: row.get(4)?,
                    safety_number: row.get(5)?,
                    remote_identity_json: row.get(6)?,
                    session_nonce: row.get(7)?,
                    session_ciphertext: row.get(8)?,
                    created_at_unix: row.get::<_, i64>(9)? as u64,
                    updated_at_unix: row.get::<_, i64>(10)? as u64,
                    expires_unix: row.get::<_, i64>(11)? as u64,
                })
            })
            .optional()?;
        let Some(mut row) = row else {
            return Ok(None);
        };
        let storage_key = self.load_storage_key()?;
        row.invite_uri = row
            .invite_uri
            .as_deref()
            .map(|value| decrypt_text_if_needed(&storage_key, value))
            .transpose()?;
        row.remote_identity_json = decrypt_text_if_needed(&storage_key, &row.remote_identity_json)?;
        Ok(Some(row))
    }

    fn contact_summaries(&self, storage_key: &Key32) -> Result<Vec<ContactSummary>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT id, display_name, account_id, device_id, safety_number, verified, updated_at_unix
             FROM contacts ORDER BY updated_at_unix DESC, display_name ASC",
        )?;
        let contacts = statement
            .query_map([], |row| {
                Ok(ContactSummary {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    account_id: row.get(2)?,
                    device_id: row.get(3)?,
                    safety_number: row.get(4)?,
                    verified: row.get::<_, i64>(5)? != 0,
                    last_message: None,
                    updated_at_unix: row.get::<_, i64>(6)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        contacts
            .into_iter()
            .map(|mut contact| {
                contact.last_message = self.last_message(&contact.id, storage_key)?;
                Ok(contact)
            })
            .collect()
    }

    fn group_summaries(&self, storage_key: &Key32) -> Result<Vec<GroupSummary>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT id, display_name, updated_at_unix FROM groups ORDER BY updated_at_unix DESC, display_name ASC",
        )?;
        let groups = statement
            .query_map([], |row| {
                Ok(GroupSummary {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    member_count: 0,
                    last_message: None,
                    updated_at_unix: row.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        groups
            .into_iter()
            .map(|mut group| {
                group.member_count = self.group_member_count(&group.id)?;
                group.last_message = self.last_group_message(&group.id, storage_key)?;
                Ok(group)
            })
            .collect()
    }

    fn group_message_views(
        &self,
        storage_key: &Key32,
    ) -> Result<Vec<GroupMessageView>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT id, group_id, sender_display_name, direction, body_nonce, body_ciphertext, status, sent_at_unix, received_at_unix
             FROM (
                 SELECT id, group_id, sender_display_name, direction, body_nonce, body_ciphertext, status, sent_at_unix, received_at_unix,
                        ROW_NUMBER() OVER (PARTITION BY group_id ORDER BY sent_at_unix DESC, id DESC) AS row_num
                 FROM group_messages
             )
             WHERE row_num <= ?1
             ORDER BY group_id ASC, sent_at_unix ASC, id ASC",
        )?;
        let rows = statement
            .query_map(params![SNAPSHOT_MESSAGES_PER_THREAD], |row| {
                let nonce: Vec<u8> = row.get(4)?;
                let ciphertext: Vec<u8> = row.get(5)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    nonce,
                    ciphertext,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            })?
            .map(|result| {
                let (
                    id,
                    group_id,
                    sender_display_name,
                    direction,
                    nonce,
                    ciphertext,
                    status,
                    sent_at,
                    received_at,
                ) = result?;
                let raw_body = decrypt_body(storage_key, &nonce, &ciphertext)?;
                let content = message_content_view(&raw_body);
                let body = message_display_text(&content);
                Ok(GroupMessageView {
                    id,
                    group_id,
                    sender_display_name,
                    direction: MessageDirection::from_str(&direction),
                    body,
                    content,
                    status: MessageStatus::from_str(&status),
                    sent_at_unix: sent_at as u64,
                    received_at_unix: received_at.map(|value| value as u64),
                })
            })
            .collect();
        rows
    }

    fn group_member_count(&self, group_id: &str) -> Result<usize, DesktopError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM group_members WHERE group_id = ?1",
            params![group_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    fn temporary_connection_summaries(
        &self,
        storage_key: &Key32,
    ) -> Result<Vec<TemporaryConnectionSummary>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT id, display_name, account_id, device_id, safety_number, updated_at_unix, expires_unix
             FROM temporary_connections ORDER BY updated_at_unix DESC, display_name ASC",
        )?;
        let connections = statement
            .query_map([], |row| {
                Ok(TemporaryConnectionSummary {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    account_id: row.get(2)?,
                    device_id: row.get(3)?,
                    safety_number: row.get(4)?,
                    last_message: None,
                    updated_at_unix: row.get::<_, i64>(5)? as u64,
                    expires_unix: row.get::<_, i64>(6)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        connections
            .into_iter()
            .map(|mut connection| {
                connection.last_message =
                    self.last_temporary_message(&connection.id, storage_key)?;
                Ok(connection)
            })
            .collect()
    }

    fn message_views(&self, storage_key: &Key32) -> Result<Vec<ChatMessageView>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT id, contact_id, direction, body_nonce, body_ciphertext, status, sent_at_unix, received_at_unix
             FROM (
                 SELECT id, contact_id, direction, body_nonce, body_ciphertext, status, sent_at_unix, received_at_unix,
                        ROW_NUMBER() OVER (PARTITION BY contact_id ORDER BY sent_at_unix DESC, id DESC) AS row_num
                 FROM messages
             )
             WHERE row_num <= ?1
             ORDER BY contact_id ASC, sent_at_unix ASC, id ASC",
        )?;
        let rows = statement
            .query_map(params![SNAPSHOT_MESSAGES_PER_THREAD], |row| {
                let nonce: Vec<u8> = row.get(3)?;
                let ciphertext: Vec<u8> = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    nonce,
                    ciphertext,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })?
            .map(|result| {
                let (id, contact_id, direction, nonce, ciphertext, status, sent_at, received_at) =
                    result?;
                let raw_body = decrypt_body(storage_key, &nonce, &ciphertext)?;
                let content = message_content_view(&raw_body);
                let body = message_display_text(&content);
                Ok(ChatMessageView {
                    id,
                    contact_id,
                    direction: MessageDirection::from_str(&direction),
                    body,
                    content,
                    status: MessageStatus::from_str(&status),
                    sent_at_unix: sent_at as u64,
                    received_at_unix: received_at.map(|value| value as u64),
                })
            })
            .collect();
        rows
    }

    fn temporary_message_views(
        &self,
        storage_key: &Key32,
    ) -> Result<Vec<TemporaryMessageView>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT id, connection_id, direction, body_nonce, body_ciphertext, status, sent_at_unix, received_at_unix
             FROM (
                 SELECT id, connection_id, direction, body_nonce, body_ciphertext, status, sent_at_unix, received_at_unix,
                        ROW_NUMBER() OVER (PARTITION BY connection_id ORDER BY sent_at_unix DESC, id DESC) AS row_num
                 FROM temporary_messages
             )
             WHERE row_num <= ?1
             ORDER BY connection_id ASC, sent_at_unix ASC, id ASC",
        )?;
        let rows =
            statement
                .query_map(params![SNAPSHOT_MESSAGES_PER_THREAD], |row| {
                    let nonce: Vec<u8> = row.get(3)?;
                    let ciphertext: Vec<u8> = row.get(4)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        nonce,
                        ciphertext,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                    ))
                })?
                .map(|result| {
                    let (
                        id,
                        connection_id,
                        direction,
                        nonce,
                        ciphertext,
                        status,
                        sent_at,
                        received_at,
                    ) = result?;
                    let raw_body = decrypt_body(storage_key, &nonce, &ciphertext)?;
                    let content = message_content_view(&raw_body);
                    let body = message_display_text(&content);
                    Ok(TemporaryMessageView {
                        id,
                        connection_id,
                        direction: MessageDirection::from_str(&direction),
                        body,
                        content,
                        status: MessageStatus::from_str(&status),
                        sent_at_unix: sent_at as u64,
                        received_at_unix: received_at.map(|value| value as u64),
                    })
                })
                .collect();
        rows
    }

    fn last_message(
        &self,
        contact_id: &str,
        storage_key: &Key32,
    ) -> Result<Option<String>, DesktopError> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT body_nonce, body_ciphertext FROM messages WHERE contact_id = ?1 ORDER BY sent_at_unix DESC LIMIT 1",
                params![contact_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(nonce, ciphertext)| {
            decrypt_body(storage_key, &nonce, &ciphertext)
                .map(|body| message_display_text(&message_content_view(&body)))
        })
        .transpose()
    }

    fn last_group_message(
        &self,
        group_id: &str,
        storage_key: &Key32,
    ) -> Result<Option<String>, DesktopError> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT body_nonce, body_ciphertext FROM group_messages WHERE group_id = ?1 ORDER BY sent_at_unix DESC LIMIT 1",
                params![group_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(nonce, ciphertext)| {
            decrypt_body(storage_key, &nonce, &ciphertext)
                .map(|body| message_display_text(&message_content_view(&body)))
        })
        .transpose()
    }

    fn last_temporary_message(
        &self,
        connection_id: &str,
        storage_key: &Key32,
    ) -> Result<Option<String>, DesktopError> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT body_nonce, body_ciphertext FROM temporary_messages WHERE connection_id = ?1 ORDER BY sent_at_unix DESC LIMIT 1",
                params![connection_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(nonce, ciphertext)| {
            decrypt_body(storage_key, &nonce, &ciphertext)
                .map(|body| message_display_text(&message_content_view(&body)))
        })
        .transpose()
    }

    fn load_group_state(
        &self,
        group_id: &str,
        storage_key: &Key32,
    ) -> Result<Option<GroupState>, DesktopError> {
        let row: Option<GroupRecord> = self
            .conn
            .query_row(
                "SELECT id, display_name, epoch, secret_nonce, secret_ciphertext, updated_at_unix
                 FROM groups WHERE id = ?1",
                params![group_id],
                |row| {
                    Ok(GroupRecord {
                        id: row.get(0)?,
                        display_name: row.get(1)?,
                        epoch: row.get::<_, i64>(2)? as u64,
                        secret_nonce: row.get(3)?,
                        secret_ciphertext: row.get(4)?,
                        updated_at_unix: row.get::<_, i64>(5)? as u64,
                    })
                },
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let nonce: [u8; 12] = row
            .secret_nonce
            .as_slice()
            .try_into()
            .map_err(|_| secure_chat_core::CryptoError::InvalidInput)?;
        let secret = decrypt_secret(storage_key, &nonce, &row.secret_ciphertext)?;
        let secret: Key32 = secret
            .as_slice()
            .try_into()
            .map_err(|_| secure_chat_core::CryptoError::InvalidInput)?;
        let mut members = Vec::new();
        let mut statement = self.conn.prepare(
            "SELECT display_name, identity_json FROM group_members WHERE group_id = ?1 ORDER BY display_name ASC",
        )?;
        let rows = statement.query_map(params![group_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (display_name, identity_json) = row?;
            let identity_json = decrypt_text_if_needed(storage_key, &identity_json)?;
            let identity: PublicDeviceIdentity = serde_json::from_str(&identity_json)?;
            members.push(GroupMember {
                display_name,
                identity,
            });
        }
        Ok(Some(GroupState {
            group_id: Uuid::parse_str(&row.id)
                .map_err(|err| DesktopError::InvalidData(err.to_string()))?,
            display_name: row.display_name,
            epoch: row.epoch,
            secret,
            members,
        }))
    }

    fn save_group_state(
        &self,
        group: &GroupState,
        storage_key: &Key32,
    ) -> Result<(), DesktopError> {
        let (secret_nonce, secret_ciphertext) = encrypt_secret(storage_key, &group.secret)?;
        let now = now_unix();
        self.conn.execute(
            "INSERT INTO groups(id, display_name, epoch, secret_nonce, secret_ciphertext, created_at_unix, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
                display_name = excluded.display_name,
                epoch = excluded.epoch,
                secret_nonce = excluded.secret_nonce,
                secret_ciphertext = excluded.secret_ciphertext,
                updated_at_unix = excluded.updated_at_unix",
            params![
                group.group_id.to_string(),
                group.display_name,
                group.epoch as i64,
                secret_nonce.to_vec(),
                secret_ciphertext,
                now
            ],
        )?;
        self.conn.execute(
            "DELETE FROM group_members WHERE group_id = ?1",
            params![group.group_id.to_string()],
        )?;
        for member in &group.members {
            let identity_json =
                encrypt_text(storage_key, &serde_json::to_string(&member.identity)?)?;
            self.conn.execute(
                "INSERT INTO group_members(group_id, account_id, device_id, display_name, identity_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    group.group_id.to_string(),
                    member.identity.account_id.to_string(),
                    member.identity.device_id.to_string(),
                    member.display_name,
                    identity_json,
                ],
            )?;
        }
        Ok(())
    }

    fn handle_group_control(&self, body: &str, storage_key: &Key32) -> Result<bool, DesktopError> {
        match decode_group_control(body)? {
            Some(GroupControlMessage::Welcome(welcome)) => {
                self.save_group_state(&GroupState::from_welcome(welcome)?, storage_key)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn load_session(&self, contact_id: &str) -> Result<Option<RatchetSession>, DesktopError> {
        let storage_key = self.load_storage_key()?;
        let row: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT session_nonce, session_ciphertext FROM sessions WHERE contact_id = ?1",
                params![contact_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(nonce, ciphertext)| {
            let nonce: [u8; 12] = nonce
                .as_slice()
                .try_into()
                .map_err(|_| secure_chat_core::CryptoError::InvalidInput)?;
            let bytes = decrypt_secret(&storage_key, &nonce, &ciphertext)?;
            serde_json::from_slice(&bytes).map_err(DesktopError::from)
        })
        .transpose()
    }

    fn load_temporary_session(
        &self,
        connection: &TemporaryConnectionRecord,
    ) -> Result<Option<RatchetSession>, DesktopError> {
        let (Some(nonce), Some(ciphertext)) = (
            connection.session_nonce.as_ref(),
            connection.session_ciphertext.as_ref(),
        ) else {
            return Ok(None);
        };
        let storage_key = self.load_storage_key()?;
        let nonce: [u8; 12] = nonce
            .as_slice()
            .try_into()
            .map_err(|_| secure_chat_core::CryptoError::InvalidInput)?;
        let bytes = decrypt_secret(&storage_key, &nonce, ciphertext)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(DesktopError::from)
    }

    fn save_session(&self, contact_id: &str, session: &RatchetSession) -> Result<(), DesktopError> {
        let storage_key = self.load_storage_key()?;
        let (session_nonce, session_ciphertext) =
            encrypt_secret(&storage_key, &serde_json::to_vec(session)?)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions (contact_id, remote_device_id, session_nonce, session_ciphertext, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                contact_id,
                session.remote_identity.device_id.to_string(),
                session_nonce.to_vec(),
                session_ciphertext,
                now_unix()
            ],
        )?;
        self.conn.execute(
            "UPDATE contacts SET updated_at_unix = ?1 WHERE id = ?2",
            params![now_unix(), contact_id],
        )?;
        Ok(())
    }

    fn save_temporary_session(
        &self,
        connection_id: &str,
        session: &RatchetSession,
    ) -> Result<(), DesktopError> {
        let storage_key = self.load_storage_key()?;
        let (session_nonce, session_ciphertext) =
            encrypt_secret(&storage_key, &serde_json::to_vec(session)?)?;
        self.conn.execute(
            "UPDATE temporary_connections
             SET session_nonce = ?1, session_ciphertext = ?2, updated_at_unix = ?3
             WHERE id = ?4",
            params![
                session_nonce.to_vec(),
                session_ciphertext,
                now_unix(),
                connection_id
            ],
        )?;
        Ok(())
    }

    fn insert_message(
        &self,
        contact_id: &str,
        direction: MessageDirection,
        body: &str,
        status: MessageStatus,
        relay_ciphertext: Option<Vec<u8>>,
        remote_message_id: Option<String>,
        storage_key: &Key32,
    ) -> Result<(), DesktopError> {
        let (nonce, body_ciphertext) = encrypt_body(storage_key, body)?;
        let now = now_unix();
        self.conn.execute(
            "INSERT INTO messages
             (id, contact_id, direction, body_nonce, body_ciphertext, relay_ciphertext, remote_message_id, status, sent_at_unix, received_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                contact_id,
                direction.as_str(),
                nonce.to_vec(),
                body_ciphertext,
                relay_ciphertext,
                remote_message_id,
                status.as_str(),
                now,
                if direction == MessageDirection::Incoming {
                    Some(now as i64)
                } else {
                    None
                }
            ],
        )?;
        self.conn.execute(
            "UPDATE contacts SET updated_at_unix = ?1 WHERE id = ?2",
            params![now, contact_id],
        )?;
        Ok(())
    }

    fn insert_group_message(
        &self,
        group_id: &str,
        sender_device_id: Uuid,
        sender_display_name: &str,
        direction: MessageDirection,
        body: &str,
        status: MessageStatus,
        relay_ciphertext: Option<Vec<u8>>,
        remote_message_id: Option<String>,
        storage_key: &Key32,
    ) -> Result<(), DesktopError> {
        let (nonce, body_ciphertext) = encrypt_body(storage_key, body)?;
        let now = now_unix();
        self.conn.execute(
            "INSERT INTO group_messages
             (id, group_id, sender_device_id, sender_display_name, direction, body_nonce, body_ciphertext, relay_ciphertext, remote_message_id, status, sent_at_unix, received_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                Uuid::new_v4().to_string(),
                group_id,
                sender_device_id.to_string(),
                sender_display_name,
                direction.as_str(),
                nonce.to_vec(),
                body_ciphertext,
                relay_ciphertext,
                remote_message_id,
                status.as_str(),
                now,
                if direction == MessageDirection::Incoming {
                    Some(now as i64)
                } else {
                    None
                }
            ],
        )?;
        self.conn.execute(
            "UPDATE groups SET updated_at_unix = ?1 WHERE id = ?2",
            params![now, group_id],
        )?;
        Ok(())
    }

    fn insert_temporary_message(
        &self,
        connection_id: &str,
        direction: MessageDirection,
        body: &str,
        status: MessageStatus,
        relay_ciphertext: Option<Vec<u8>>,
        remote_message_id: Option<String>,
        storage_key: &Key32,
    ) -> Result<(), DesktopError> {
        let (nonce, body_ciphertext) = encrypt_body(storage_key, body)?;
        let now = now_unix();
        self.conn.execute(
            "INSERT INTO temporary_messages
             (id, connection_id, direction, body_nonce, body_ciphertext, relay_ciphertext, remote_message_id, status, sent_at_unix, received_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                connection_id,
                direction.as_str(),
                nonce.to_vec(),
                body_ciphertext,
                relay_ciphertext,
                remote_message_id,
                status.as_str(),
                now,
                if direction == MessageDirection::Incoming {
                    Some(now as i64)
                } else {
                    None
                }
            ],
        )?;
        self.conn.execute(
            "UPDATE temporary_connections SET updated_at_unix = ?1 WHERE id = ?2",
            params![now, connection_id],
        )?;
        self.prune_temporary_messages(connection_id)?;
        Ok(())
    }

    fn delete_temporary_connection(&self, connection_id: &str) -> Result<(), DesktopError> {
        self.delete_thread_attachments(ThreadKind::Temporary, connection_id)?;
        self.conn.execute(
            "DELETE FROM temporary_messages WHERE connection_id = ?1",
            params![connection_id],
        )?;
        self.conn.execute(
            "DELETE FROM temporary_connections WHERE id = ?1",
            params![connection_id],
        )?;
        Ok(())
    }

    fn delete_expired_temporary_connections(&self) -> Result<(), DesktopError> {
        let expired_ids = {
            let mut statement = self
                .conn
                .prepare("SELECT id FROM temporary_connections WHERE expires_unix <= ?1")?;
            let rows = statement
                .query_map(params![now_unix()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for id in expired_ids {
            self.delete_temporary_connection(&id)?;
        }
        Ok(())
    }

    fn prune_temporary_connections(&self) -> Result<(), DesktopError> {
        let stale_ids = {
            let mut statement = self.conn.prepare(
                "SELECT id FROM temporary_connections
                 ORDER BY updated_at_unix DESC, created_at_unix DESC
                 LIMIT -1 OFFSET ?1",
            )?;
            let rows = statement
                .query_map(params![MAX_TEMP_CONNECTIONS as i64], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for id in stale_ids {
            self.delete_temporary_connection(&id)?;
        }
        Ok(())
    }

    fn prune_temporary_messages(&self, connection_id: &str) -> Result<(), DesktopError> {
        self.conn.execute(
            "DELETE FROM temporary_messages
             WHERE connection_id = ?1
               AND id NOT IN (
                   SELECT id FROM temporary_messages
                   WHERE connection_id = ?1
                   ORDER BY sent_at_unix DESC
                   LIMIT ?2
               )",
            params![connection_id, MAX_TEMP_MESSAGES_PER_CONNECTION as i64],
        )?;
        Ok(())
    }

    fn handle_incoming_payload(
        &self,
        thread_kind: ThreadKind,
        thread_id: &str,
        body: &str,
        sender_device_id: Option<Uuid>,
        sender_display_name: Option<&str>,
        storage_key: &Key32,
    ) -> Result<bool, DesktopError> {
        let Some(content) = decode_wire_content(body)? else {
            return Ok(false);
        };
        if content.data_base64.is_some() && content.chunk_index.is_some() {
            self.store_attachment_chunk(thread_kind, thread_id, &content)?;
            if let Some(attachment) = self.try_complete_attachment(&content)? {
                let visible_body = encode_wire_content(&WireContent {
                    kind: attachment.kind.clone(),
                    text: None,
                    burn_id: None,
                    destroyed: None,
                    target_burn_id: None,
                    attachment: Some(attachment.clone()),
                    attachment_id: None,
                    file_name: None,
                    mime_type: None,
                    size_bytes: None,
                    sha256: None,
                    chunk_index: None,
                    total_chunks: None,
                    data_base64: None,
                })?;
                self.record_attachment(thread_kind, thread_id, &attachment, "complete")?;
                match thread_kind {
                    ThreadKind::Contact => self.insert_message(
                        thread_id,
                        MessageDirection::Incoming,
                        &visible_body,
                        MessageStatus::Received,
                        None,
                        None,
                        storage_key,
                    )?,
                    ThreadKind::Temporary => self.insert_temporary_message(
                        thread_id,
                        MessageDirection::Incoming,
                        &visible_body,
                        MessageStatus::Received,
                        None,
                        None,
                        storage_key,
                    )?,
                    ThreadKind::Group => self.insert_group_message(
                        thread_id,
                        sender_device_id.unwrap_or_else(Uuid::nil),
                        sender_display_name.unwrap_or("Group member"),
                        MessageDirection::Incoming,
                        &visible_body,
                        MessageStatus::Received,
                        None,
                        None,
                        storage_key,
                    )?,
                }
            }
            Ok(true)
        } else {
            match content.kind.as_str() {
                "destroy" => {
                    if let Some(burn_id) = content.target_burn_id.as_deref() {
                        self.destroy_burn_by_burn_id(thread_kind, burn_id)?;
                    }
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
    }

    fn store_attachment_chunk(
        &self,
        thread_kind: ThreadKind,
        thread_id: &str,
        content: &WireContent,
    ) -> Result<(), DesktopError> {
        let attachment_id = content
            .attachment_id
            .as_deref()
            .ok_or_else(|| DesktopError::InvalidData("missing attachment id".to_string()))?;
        let chunk_index = content
            .chunk_index
            .ok_or_else(|| DesktopError::InvalidData("missing chunk index".to_string()))?;
        let data = content
            .data_base64
            .as_deref()
            .ok_or_else(|| DesktopError::InvalidData("missing chunk data".to_string()))
            .and_then(|value| {
                STANDARD
                    .decode(value)
                    .map_err(|err| DesktopError::InvalidData(err.to_string()))
            })?;
        let dir = self.incoming_chunks_dir(attachment_id)?;
        let chunk_path = dir.join(format!("{chunk_index:08}.chunk"));
        fs::write(&chunk_path, data)?;
        let now = now_unix();
        self.conn.execute(
            "INSERT OR REPLACE INTO attachment_chunks
             (attachment_id, thread_kind, thread_id, kind, file_name, mime_type, size_bytes, sha256, chunk_index, total_chunks, chunk_path, created_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                attachment_id,
                thread_kind.as_str(),
                thread_id,
                normalize_attachment_kind(content.kind.as_str()),
                sanitize_file_name(content.file_name.as_deref().unwrap_or("attachment")),
                content.mime_type.as_deref().unwrap_or("application/octet-stream"),
                content.size_bytes.unwrap_or_default() as i64,
                content.sha256.as_deref().unwrap_or_default(),
                chunk_index as i64,
                content.total_chunks.unwrap_or(1) as i64,
                chunk_path.to_string_lossy().to_string(),
                now as i64,
            ],
        )?;
        Ok(())
    }

    fn try_complete_attachment(
        &self,
        content: &WireContent,
    ) -> Result<Option<AttachmentView>, DesktopError> {
        let attachment_id = content
            .attachment_id
            .as_deref()
            .ok_or_else(|| DesktopError::InvalidData("missing attachment id".to_string()))?;
        if self.conn.query_row(
            "SELECT COUNT(*) FROM attachments WHERE id = ?1",
            params![attachment_id],
            |row| row.get::<_, i64>(0),
        )? > 0
        {
            return Ok(None);
        }
        let total_chunks = content.total_chunks.unwrap_or(1);
        let received: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM attachment_chunks WHERE attachment_id = ?1",
            params![attachment_id],
            |row| row.get(0),
        )?;
        if received < total_chunks as i64 {
            return Ok(None);
        }
        let mut statement = self.conn.prepare(
            "SELECT chunk_path FROM attachment_chunks WHERE attachment_id = ?1 ORDER BY chunk_index ASC",
        )?;
        let chunk_paths = statement
            .query_map(params![attachment_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let file_name = sanitize_file_name(content.file_name.as_deref().unwrap_or("attachment"));
        let final_path = self.attachment_file_path(attachment_id, &file_name)?;
        let mut out = fs::File::create(&final_path)?;
        for chunk_path in &chunk_paths {
            let mut chunk = fs::File::open(chunk_path)?;
            std::io::copy(&mut chunk, &mut out)?;
        }
        out.flush()?;
        let actual_sha256 = sha256_file(&final_path)?;
        let expected_sha256 = content.sha256.clone().unwrap_or_default();
        if !expected_sha256.is_empty() && actual_sha256 != expected_sha256 {
            let _ = fs::remove_file(&final_path);
            return Err(DesktopError::InvalidData(
                "attachment checksum mismatch".to_string(),
            ));
        }
        self.conn.execute(
            "DELETE FROM attachment_chunks WHERE attachment_id = ?1",
            params![attachment_id],
        )?;
        let _ = fs::remove_dir_all(self.incoming_chunks_dir_path(attachment_id));
        Ok(Some(AttachmentView {
            id: attachment_id.to_string(),
            kind: normalize_attachment_kind(content.kind.as_str()).to_string(),
            file_name,
            mime_type: content
                .mime_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            size_bytes: content.size_bytes.unwrap_or_default(),
            sha256: actual_sha256,
            local_path: Some(final_path.to_string_lossy().to_string()),
            transfer_status: "complete".to_string(),
        }))
    }

    fn record_attachment(
        &self,
        thread_kind: ThreadKind,
        thread_id: &str,
        attachment: &AttachmentView,
        status: &str,
    ) -> Result<(), DesktopError> {
        let now = now_unix();
        self.conn.execute(
            "INSERT OR REPLACE INTO attachments
             (id, thread_kind, thread_id, kind, file_name, mime_type, size_bytes, sha256, local_path, status, created_at_unix, updated_at_unix)
             VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                COALESCE((SELECT created_at_unix FROM attachments WHERE id = ?1), ?11),
                ?11
             )",
            params![
                attachment.id.as_str(),
                thread_kind.as_str(),
                thread_id,
                attachment.kind.as_str(),
                attachment.file_name.as_str(),
                attachment.mime_type.as_str(),
                attachment.size_bytes as i64,
                attachment.sha256.as_str(),
                attachment.local_path.as_deref(),
                status,
                now as i64,
            ],
        )?;
        Ok(())
    }

    fn sticker_items(&self) -> Result<Vec<StickerItemView>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT id, display_name, file_name, mime_type, size_bytes, sha256, local_path, created_at_unix
             FROM stickers ORDER BY created_at_unix DESC, display_name ASC",
        )?;
        let stickers = statement
            .query_map([], |row| {
                Ok(StickerItemView {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    file_name: row.get(2)?,
                    mime_type: row.get(3)?,
                    size_bytes: row.get::<_, i64>(4)? as u64,
                    sha256: row.get(5)?,
                    local_path: row.get(6)?,
                    created_at_unix: row.get::<_, i64>(7)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DesktopError::from)?;
        Ok(stickers)
    }

    fn attachment_transfers(&self) -> Result<Vec<AttachmentTransferView>, DesktopError> {
        let mut statement = self.conn.prepare(
            "SELECT attachment_id, thread_kind, thread_id, kind, file_name, mime_type, size_bytes, sha256,
                    COUNT(*), MAX(total_chunks)
             FROM attachment_chunks
             GROUP BY attachment_id, thread_kind, thread_id, kind, file_name, mime_type, size_bytes, sha256
             ORDER BY MAX(created_at_unix) DESC",
        )?;
        let transfers = statement
            .query_map([], |row| {
                Ok(AttachmentTransferView {
                    id: row.get(0)?,
                    thread_kind: row.get(1)?,
                    thread_id: row.get(2)?,
                    kind: row.get(3)?,
                    file_name: row.get(4)?,
                    mime_type: row.get(5)?,
                    size_bytes: row.get::<_, i64>(6)? as u64,
                    sha256: row.get(7)?,
                    received_chunks: row.get::<_, i64>(8)? as u64,
                    total_chunks: row.get::<_, i64>(9)? as u64,
                    status: "receiving".to_string(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DesktopError::from)?;
        Ok(transfers)
    }

    fn import_sticker_inner(
        &self,
        file_path: &str,
        display_name: &str,
    ) -> Result<StickerItemView, DesktopError> {
        let source = Path::new(file_path);
        if !source.is_file() {
            return Err(DesktopError::FileNotFound);
        }
        let id = Uuid::new_v4().to_string();
        let file_name = source
            .file_name()
            .and_then(|value| value.to_str())
            .map(sanitize_file_name)
            .unwrap_or_else(|| format!("sticker-{id}"));
        let display_name = {
            let trimmed = display_name.trim();
            if trimmed.is_empty() {
                file_name.clone()
            } else {
                trimmed.to_string()
            }
        };
        let mime_type = guess_mime_type(&file_name);
        let size_bytes = fs::metadata(source)?.len();
        let sha256 = sha256_file(source)?;
        let target = self.sticker_file_path(&id, &file_name)?;
        fs::copy(source, &target)?;
        let now = now_unix();
        let sticker = StickerItemView {
            id,
            display_name,
            file_name,
            mime_type,
            size_bytes,
            sha256,
            local_path: target.to_string_lossy().to_string(),
            created_at_unix: now,
        };
        self.conn.execute(
            "INSERT INTO stickers(id, pack_id, display_name, file_name, mime_type, size_bytes, sha256, local_path, created_at_unix)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                sticker.id.as_str(),
                sticker.display_name.as_str(),
                sticker.file_name.as_str(),
                sticker.mime_type.as_str(),
                sticker.size_bytes as i64,
                sticker.sha256.as_str(),
                sticker.local_path.as_str(),
                now as i64,
            ],
        )?;
        Ok(sticker)
    }

    fn delete_sticker_inner(&self, sticker_id: &str) -> Result<(), DesktopError> {
        let path: Option<String> = self
            .conn
            .query_row(
                "SELECT local_path FROM stickers WHERE id = ?1",
                params![sticker_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(path) = path {
            let _ = fs::remove_file(path);
        }
        self.conn
            .execute("DELETE FROM stickers WHERE id = ?1", params![sticker_id])?;
        Ok(())
    }

    fn delete_contact_inner(&self, contact_id: &str) -> Result<(), DesktopError> {
        let exists: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM contacts WHERE id = ?1",
                params![contact_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(DesktopError::ContactNotFound);
        }
        self.delete_thread_attachments(ThreadKind::Contact, contact_id)?;
        self.conn.execute(
            "DELETE FROM messages WHERE contact_id = ?1",
            params![contact_id],
        )?;
        self.conn.execute(
            "DELETE FROM sessions WHERE contact_id = ?1",
            params![contact_id],
        )?;
        self.conn
            .execute("DELETE FROM contacts WHERE id = ?1", params![contact_id])?;
        Ok(())
    }

    fn delete_thread_attachments(
        &self,
        thread_kind: ThreadKind,
        thread_id: &str,
    ) -> Result<(), DesktopError> {
        let paths = {
            let mut statement = self.conn.prepare(
                "SELECT local_path FROM attachments WHERE thread_kind = ?1 AND thread_id = ?2 AND local_path IS NOT NULL",
            )?;
            let paths = statement
                .query_map(params![thread_kind.as_str(), thread_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            paths
        };
        for path in paths {
            let _ = fs::remove_file(path);
        }
        self.conn.execute(
            "DELETE FROM attachments WHERE thread_kind = ?1 AND thread_id = ?2",
            params![thread_kind.as_str(), thread_id],
        )?;
        self.conn.execute(
            "DELETE FROM attachment_chunks WHERE thread_kind = ?1 AND thread_id = ?2",
            params![thread_kind.as_str(), thread_id],
        )?;
        Ok(())
    }

    fn copy_attachment_file(
        &self,
        attachment_id: &str,
        source_path: &Path,
        file_name: &str,
    ) -> Result<PathBuf, DesktopError> {
        let target = self.attachment_file_path(attachment_id, file_name)?;
        fs::copy(source_path, &target)?;
        Ok(target)
    }

    fn attachment_file_path(
        &self,
        attachment_id: &str,
        file_name: &str,
    ) -> Result<PathBuf, DesktopError> {
        let dir = self.attachments_dir().join(attachment_id);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(sanitize_file_name(file_name)))
    }

    fn sticker_file_path(
        &self,
        sticker_id: &str,
        file_name: &str,
    ) -> Result<PathBuf, DesktopError> {
        let dir = self.data_dir.join("stickers");
        fs::create_dir_all(&dir)?;
        Ok(dir.join(format!("{sticker_id}-{}", sanitize_file_name(file_name))))
    }

    fn incoming_chunks_dir(&self, attachment_id: &str) -> Result<PathBuf, DesktopError> {
        let dir = self.incoming_chunks_dir_path(attachment_id);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn incoming_chunks_dir_path(&self, attachment_id: &str) -> PathBuf {
        self.data_dir.join("attachment-chunks").join(attachment_id)
    }

    fn attachments_dir(&self) -> PathBuf {
        self.data_dir.join("attachments")
    }

    fn destroy_local_burn_message(
        &self,
        thread_kind: ThreadKind,
        message_id: &str,
    ) -> Result<Option<String>, DesktopError> {
        let storage_key = self.load_storage_key()?;
        let table = match thread_kind {
            ThreadKind::Contact => "messages",
            ThreadKind::Group => "group_messages",
            ThreadKind::Temporary => "temporary_messages",
        };
        let row: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row(
                &format!("SELECT body_nonce, body_ciphertext FROM {table} WHERE id = ?1"),
                params![message_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((nonce, ciphertext)) = row else {
            return Ok(None);
        };
        let body = decrypt_body(&storage_key, &nonce, &ciphertext)?;
        let Some(mut content) = decode_wire_content(&body)? else {
            return Ok(None);
        };
        if content.kind != "burn" {
            return Ok(None);
        }
        let burn_id = content.burn_id.clone();
        content.text = None;
        content.destroyed = Some(true);
        let destroyed_body = encode_wire_content(&content)?;
        let (new_nonce, new_ciphertext) = encrypt_body(&storage_key, &destroyed_body)?;
        self.conn.execute(
            &format!(
                "UPDATE {table}
                 SET body_nonce = ?1, body_ciphertext = ?2, burn_destroyed = 1
                 WHERE id = ?3"
            ),
            params![new_nonce.to_vec(), new_ciphertext, message_id],
        )?;
        Ok(burn_id)
    }

    fn destroy_burn_by_burn_id(
        &self,
        thread_kind: ThreadKind,
        burn_id: &str,
    ) -> Result<(), DesktopError> {
        let storage_key = self.load_storage_key()?;
        let table = match thread_kind {
            ThreadKind::Contact => "messages",
            ThreadKind::Group => "group_messages",
            ThreadKind::Temporary => "temporary_messages",
        };
        let rows = {
            let mut statement = self.conn.prepare(&format!(
                "SELECT id, body_nonce, body_ciphertext FROM {table}"
            ))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for (message_id, nonce, ciphertext) in rows {
            let body = decrypt_body(&storage_key, &nonce, &ciphertext)?;
            let Some(mut content) = decode_wire_content(&body)? else {
                continue;
            };
            if content.kind == "burn" && content.burn_id.as_deref() == Some(burn_id) {
                content.text = None;
                content.destroyed = Some(true);
                let destroyed_body = encode_wire_content(&content)?;
                let (new_nonce, new_ciphertext) = encrypt_body(&storage_key, &destroyed_body)?;
                self.conn.execute(
                    &format!(
                        "UPDATE {table}
                         SET body_nonce = ?1, body_ciphertext = ?2, burn_destroyed = 1
                         WHERE id = ?3"
                    ),
                    params![new_nonce.to_vec(), new_ciphertext, message_id],
                )?;
            }
        }
        Ok(())
    }

    fn apply_receipts(
        &self,
        receipts: Vec<secure_chat_core::QueuedReceipt>,
    ) -> Result<(), DesktopError> {
        for receipt in receipts {
            let status = match receipt.kind {
                ReceiptKind::Delivered => MessageStatus::Delivered,
                ReceiptKind::Read => MessageStatus::Read,
            };
            self.conn.execute(
                "UPDATE messages
                 SET status = ?1, received_at_unix = COALESCE(received_at_unix, ?2)
                 WHERE remote_message_id = ?3
                   AND direction = 'outgoing'
                   AND status NOT IN ('read')",
                params![
                    status.as_str(),
                    receipt.at_unix as i64,
                    receipt.message_id.to_string()
                ],
            )?;
            self.conn.execute(
                "UPDATE temporary_messages
                 SET status = ?1, received_at_unix = COALESCE(received_at_unix, ?2)
                 WHERE remote_message_id = ?3
                   AND direction = 'outgoing'
                   AND status NOT IN ('read')",
                params![
                    status.as_str(),
                    receipt.at_unix as i64,
                    receipt.message_id.to_string()
                ],
            )?;
        }
        Ok(())
    }

    fn load_device_keys(&self) -> Result<DeviceKeyMaterial, DesktopError> {
        let json = self.load_secret("device_keys")?;
        let mut keys: DeviceKeyMaterial = serde_json::from_str(&json)?;
        if keys.ensure_current_signatures()? {
            self.save_device_keys(&keys)?;
        }
        Ok(keys)
    }

    fn save_device_keys(&self, keys: &DeviceKeyMaterial) -> Result<(), DesktopError> {
        self.save_secret("device_keys", &serde_json::to_string(keys)?)?;
        Ok(())
    }

    fn load_storage_key(&self) -> Result<Key32, DesktopError> {
        let text = self.load_secret("storage_key")?;
        let bytes = STANDARD
            .decode(text)
            .map_err(|err| DesktopError::InvalidData(err.to_string()))?;
        bytes
            .try_into()
            .map_err(|_| DesktopError::InvalidData("invalid storage key length".to_string()))
    }

    fn save_storage_key(&self, key: &Key32) -> Result<(), DesktopError> {
        self.save_secret("storage_key", &STANDARD.encode(key))?;
        Ok(())
    }

    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
    fn load_secret(&self, kind: &str) -> Result<String, DesktopError> {
        Ok(self.keychain_entry(kind)?.get_password()?)
    }

    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
    fn save_secret(&self, kind: &str, value: &str) -> Result<(), DesktopError> {
        self.keychain_entry(kind)?.set_password(value)?;
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn load_secret(&self, kind: &str) -> Result<String, DesktopError> {
        fs::read_to_string(self.secret_store_path(kind))
            .map(|value| value.trim_end_matches('\n').to_string())
            .map_err(|err| DesktopError::SecretStore(err.to_string()))
    }

    #[cfg(target_os = "android")]
    fn save_secret(&self, kind: &str, value: &str) -> Result<(), DesktopError> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;

        let path = self.secret_store_path(kind);
        let parent = path
            .parent()
            .ok_or_else(|| DesktopError::SecretStore("invalid secret path".to_string()))?;
        fs::create_dir_all(parent)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(value.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn secret_store_path(&self, kind: &str) -> PathBuf {
        self.data_dir
            .join("secrets")
            .join(self.keychain_scope())
            .join(format!("{kind}.secret"))
    }

    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
    fn keychain_entry(&self, kind: &str) -> Result<Entry, keyring::Error> {
        Entry::new(
            KEYCHAIN_SERVICE,
            &format!("secure-chat:{}:{kind}", self.keychain_scope()),
        )
    }

    fn keychain_scope(&self) -> String {
        to_hex(&sha256(&[self.data_dir.to_string_lossy().as_bytes()]))
    }
}

#[derive(Clone)]
struct ProfileRow {
    display_name: String,
    relay_url: String,
}

fn encode_wire_content(content: &WireContent) -> Result<String, DesktopError> {
    Ok(format!(
        "{CONTENT_PREFIX}{}",
        STANDARD.encode(serde_json::to_vec(content)?)
    ))
}

fn decode_wire_content(body: &str) -> Result<Option<WireContent>, DesktopError> {
    let Some(encoded) = body.strip_prefix(CONTENT_PREFIX) else {
        return Ok(None);
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|err| DesktopError::InvalidData(err.to_string()))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(DesktopError::from)
}

fn message_content_view(body: &str) -> MessageContentView {
    match decode_wire_content(body) {
        Ok(Some(content)) => match content.kind.as_str() {
            "burn" => MessageContentView {
                kind: "burn".to_string(),
                text: content.text,
                burn_id: content.burn_id,
                destroyed: content.destroyed.unwrap_or(false),
                attachment: content.attachment,
            },
            "image" | "file" | "sticker" => MessageContentView {
                kind: content.kind,
                text: content.text,
                burn_id: content.burn_id,
                destroyed: content.destroyed.unwrap_or(false),
                attachment: content.attachment,
            },
            "destroy" => MessageContentView {
                kind: "destroyed".to_string(),
                text: None,
                burn_id: content.target_burn_id,
                destroyed: true,
                attachment: None,
            },
            _ => MessageContentView {
                kind: "text".to_string(),
                text: content.text,
                burn_id: None,
                destroyed: false,
                attachment: None,
            },
        },
        _ => MessageContentView {
            kind: "text".to_string(),
            text: Some(body.to_string()),
            burn_id: None,
            destroyed: false,
            attachment: None,
        },
    }
}

fn message_display_text(content: &MessageContentView) -> String {
    match content.kind.as_str() {
        "burn" if content.destroyed => "Burned message".to_string(),
        "burn" => "Burn after reading".to_string(),
        "image" => content
            .attachment
            .as_ref()
            .map(|attachment| format!("Image: {}", attachment.file_name))
            .unwrap_or_else(|| "Image".to_string()),
        "sticker" => content
            .attachment
            .as_ref()
            .map(|attachment| format!("Sticker: {}", attachment.file_name))
            .unwrap_or_else(|| "Sticker".to_string()),
        "file" => content
            .attachment
            .as_ref()
            .map(|attachment| format!("File: {}", attachment.file_name))
            .unwrap_or_else(|| "File".to_string()),
        "destroyed" => "Burned message".to_string(),
        _ => content.text.clone().unwrap_or_default(),
    }
}

fn normalize_attachment_kind(kind: &str) -> &'static str {
    match kind {
        "image" => "image",
        "sticker" => "sticker",
        _ => "file",
    }
}

fn sanitize_file_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if sanitized.is_empty() {
        "attachment".to_string()
    } else {
        sanitized
    }
}

fn guess_mime_type(file_name: &str) -> String {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".heic") || lower.ends_with(".heif") {
        "image/heic"
    } else if lower.ends_with(".pdf") {
        "application/pdf"
    } else if lower.ends_with(".txt") {
        "text/plain"
    } else {
        "application/octet-stream"
    }
    .to_string()
}

fn sha256_file(path: &Path) -> Result<String, DesktopError> {
    let mut hasher = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(to_hex(digest.as_slice()))
}

fn encrypt_body(storage_key: &Key32, body: &str) -> Result<([u8; 12], Vec<u8>), DesktopError> {
    encrypt_secret(storage_key, body.as_bytes())
}

fn encrypt_secret(
    storage_key: &Key32,
    plaintext: &[u8],
) -> Result<([u8; 12], Vec<u8>), DesktopError> {
    let nonce = random_bytes::<12>();
    let ciphertext = encrypt_aead(
        CipherSuite::ChaCha20Poly1305,
        storage_key,
        &nonce,
        plaintext,
        b"secure-chat-v1/local-message",
    )?;
    Ok((nonce, ciphertext))
}

fn encrypt_text(storage_key: &Key32, plaintext: &str) -> Result<String, DesktopError> {
    let (nonce, ciphertext) = encrypt_secret(storage_key, plaintext.as_bytes())?;
    Ok(format!(
        "{ENCRYPTED_TEXT_PREFIX}{}:{}",
        STANDARD.encode(nonce),
        STANDARD.encode(ciphertext)
    ))
}

fn is_encrypted_text(value: &str) -> bool {
    value.starts_with(ENCRYPTED_TEXT_PREFIX)
}

fn decrypt_text_if_needed(storage_key: &Key32, value: &str) -> Result<String, DesktopError> {
    if !is_encrypted_text(value) {
        return Ok(value.to_string());
    }
    let encoded = value
        .strip_prefix(ENCRYPTED_TEXT_PREFIX)
        .ok_or_else(|| DesktopError::InvalidData("invalid encrypted text".to_string()))?;
    let (nonce, ciphertext) = encoded
        .split_once(':')
        .ok_or_else(|| DesktopError::InvalidData("invalid encrypted text".to_string()))?;
    let nonce = STANDARD
        .decode(nonce)
        .map_err(|err| DesktopError::InvalidData(err.to_string()))?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| DesktopError::InvalidData("invalid encrypted text nonce".to_string()))?;
    let ciphertext = STANDARD
        .decode(ciphertext)
        .map_err(|err| DesktopError::InvalidData(err.to_string()))?;
    let plaintext = decrypt_secret(storage_key, &nonce, &ciphertext)?;
    String::from_utf8(plaintext).map_err(|err| DesktopError::InvalidData(err.to_string()))
}

fn decrypt_body(
    storage_key: &Key32,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<String, DesktopError> {
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| secure_chat_core::CryptoError::InvalidInput)?;
    let plaintext = decrypt_secret(storage_key, &nonce, ciphertext)?;
    String::from_utf8(plaintext).map_err(|err| DesktopError::InvalidData(err.to_string()))
}

fn decrypt_secret(
    storage_key: &Key32,
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>, DesktopError> {
    Ok(decrypt_aead(
        CipherSuite::ChaCha20Poly1305,
        storage_key,
        nonce,
        ciphertext,
        b"secure-chat-v1/local-message",
    )?)
}

fn padding_profile(payload_len: usize) -> secure_chat_core::ObfuscationProfile {
    let mut profile = secure_chat_core::ObfuscationProfile::websocket_fallback();
    profile.fixed_frame_size = padded_bucket(payload_len);
    profile
}

fn padded_bucket(payload_len: usize) -> usize {
    const BUCKET: usize = 1024;
    let minimum = BUCKET;
    let needed = payload_len.saturating_add(16).max(minimum);
    needed.div_ceil(BUCKET) * BUCKET
}

fn decode_invite(input: &str) -> Result<(String, Invite), DesktopError> {
    let normalized = extract_invite_uri(input)?;
    let invite = Invite::from_uri(&normalized).map_err(|_| DesktopError::InvalidInvite)?;
    invite.verify().map_err(|_| DesktopError::InvalidInvite)?;
    Ok((normalized, invite))
}

fn extract_invite_uri(input: &str) -> Result<String, DesktopError> {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();
    let prefix = "schat://invite/";
    let start = lower.find(prefix).ok_or(DesktopError::InvalidInvite)?;
    let payload_start = start + prefix.len();
    let payload = trimmed[payload_start..]
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect::<String>();
    if payload.is_empty() {
        return Err(DesktopError::InvalidInvite);
    }
    Ok(format!("{prefix}{payload}"))
}

fn validate_invite_for_local_device(
    invite: &Invite,
    keys: &DeviceKeyMaterial,
) -> Result<(), DesktopError> {
    if let Some(expires_unix) = invite.expires_unix {
        if expires_unix <= now_unix() {
            return Err(DesktopError::ExpiredInvite);
        }
    }
    if invite.bundle.identity.device_id == keys.device_id {
        return Err(DesktopError::SelfInvite);
    }
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_data_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("secure-chat-{label}-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn regular_invite_reply_creates_normal_contact_for_invite_owner() {
        let (addr, handle) = secure_chat_relay::spawn_ephemeral().await.unwrap();
        let relay_url = format!("http://{addr}");
        let alice_dir = test_data_dir("alice-regular");
        let bob_dir = test_data_dir("bob-regular");

        DesktopRuntime::bootstrap(&alice_dir, "Alice", &relay_url)
            .await
            .unwrap();
        DesktopRuntime::bootstrap(&bob_dir, "Bob", &relay_url)
            .await
            .unwrap();

        let alice_invite = DesktopRuntime::invite(&alice_dir).unwrap().invite_uri;
        let bob_snapshot = DesktopRuntime::add_contact(&bob_dir, "Alice", &alice_invite).unwrap();
        let alice_contact_id = bob_snapshot.contacts[0].id.clone();
        DesktopRuntime::send_message(&bob_dir, &alice_contact_id, "hello from Bob")
            .await
            .unwrap();

        let alice_report = DesktopRuntime::receive(&alice_dir).await.unwrap();
        assert_eq!(alice_report.received_count, 1);
        assert_eq!(alice_report.snapshot.contacts.len(), 1);
        assert!(alice_report.snapshot.temporary_connections.is_empty());
        assert_eq!(alice_report.snapshot.messages.len(), 1);
        assert_eq!(alice_report.snapshot.messages[0].body, "hello from Bob");

        let bob_contact_id = alice_report.snapshot.contacts[0].id.clone();
        DesktopRuntime::send_message(&alice_dir, &bob_contact_id, "hi Bob")
            .await
            .unwrap();
        let bob_report = DesktopRuntime::receive(&bob_dir).await.unwrap();
        assert_eq!(bob_report.received_count, 1);
        assert!(bob_report
            .snapshot
            .messages
            .iter()
            .any(|message| message.body == "hi Bob"));

        let temporary_invite = DesktopRuntime::temporary_invite(&alice_dir)
            .unwrap()
            .invite_uri;
        let temporary_start =
            DesktopRuntime::start_temporary_connection(&bob_dir, &temporary_invite).unwrap();
        DesktopRuntime::send_temporary_message(
            &bob_dir,
            &temporary_start.connection_id,
            "temporary side channel",
        )
        .await
        .unwrap();
        let temporary_report = DesktopRuntime::receive(&alice_dir).await.unwrap();
        assert_eq!(temporary_report.received_count, 1);
        assert_eq!(temporary_report.snapshot.contacts.len(), 1);
        assert_eq!(temporary_report.snapshot.temporary_connections.len(), 1);
        assert!(temporary_report
            .snapshot
            .temporary_messages
            .iter()
            .any(|message| message.body == "temporary side channel"));

        handle.abort();
        let _ = fs::remove_dir_all(alice_dir);
        let _ = fs::remove_dir_all(bob_dir);
    }

    #[tokio::test]
    async fn temporary_invite_reply_stays_in_temporary_connections() {
        let (addr, handle) = secure_chat_relay::spawn_ephemeral().await.unwrap();
        let relay_url = format!("http://{addr}");
        let alice_dir = test_data_dir("alice-temporary");
        let bob_dir = test_data_dir("bob-temporary");

        DesktopRuntime::bootstrap(&alice_dir, "Alice", &relay_url)
            .await
            .unwrap();
        DesktopRuntime::bootstrap(&bob_dir, "Bob", &relay_url)
            .await
            .unwrap();

        let alice_invite = DesktopRuntime::temporary_invite(&alice_dir)
            .unwrap()
            .invite_uri;
        let start = DesktopRuntime::start_temporary_connection(&bob_dir, &alice_invite).unwrap();
        DesktopRuntime::send_temporary_message(&bob_dir, &start.connection_id, "temporary hello")
            .await
            .unwrap();

        let alice_report = DesktopRuntime::receive(&alice_dir).await.unwrap();
        assert_eq!(alice_report.received_count, 1);
        assert!(alice_report.snapshot.contacts.is_empty());
        assert_eq!(alice_report.snapshot.temporary_connections.len(), 1);
        assert_eq!(alice_report.snapshot.temporary_messages.len(), 1);
        assert_eq!(
            alice_report.snapshot.temporary_messages[0].body,
            "temporary hello"
        );

        handle.abort();
        let _ = fs::remove_dir_all(alice_dir);
        let _ = fs::remove_dir_all(bob_dir);
    }

    #[tokio::test]
    async fn rich_content_features_round_trip_and_contact_delete_cleans_chat() {
        let (addr, handle) = secure_chat_relay::spawn_ephemeral().await.unwrap();
        let relay_url = format!("http://{addr}");
        let alice_dir = test_data_dir("alice-rich-content");
        let bob_dir = test_data_dir("bob-rich-content");

        DesktopRuntime::bootstrap(&alice_dir, "Alice", &relay_url)
            .await
            .unwrap();
        DesktopRuntime::bootstrap(&bob_dir, "Bob", &relay_url)
            .await
            .unwrap();

        let alice_invite = DesktopRuntime::invite(&alice_dir).unwrap().invite_uri;
        let bob_snapshot = DesktopRuntime::add_contact(&bob_dir, "Alice", &alice_invite).unwrap();
        let alice_contact_id_for_bob = bob_snapshot.contacts[0].id.clone();
        let renamed = DesktopRuntime::update_contact_display_name(
            &bob_dir,
            &alice_contact_id_for_bob,
            "Alice 🔐",
        )
        .unwrap();
        assert_eq!(renamed.contacts[0].display_name, "Alice 🔐");

        let image_path = bob_dir.join("wave-image.png");
        fs::write(&image_path, b"not-a-real-png-but-still-private-bytes").unwrap();
        let sticker =
            DesktopRuntime::import_sticker(&bob_dir, image_path.to_str().unwrap(), "Wave 👋")
                .unwrap();
        assert_eq!(sticker.snapshot.stickers.len(), 1);

        DesktopRuntime::send_message(&bob_dir, &alice_contact_id_for_bob, "hello 😀")
            .await
            .unwrap();
        DesktopRuntime::send_attachment(
            &bob_dir,
            "contact",
            &alice_contact_id_for_bob,
            image_path.to_str().unwrap(),
            "image",
        )
        .await
        .unwrap();
        DesktopRuntime::send_attachment(
            &bob_dir,
            "contact",
            &alice_contact_id_for_bob,
            &sticker.sticker.local_path,
            "sticker",
        )
        .await
        .unwrap();
        DesktopRuntime::send_burn_message(
            &bob_dir,
            "contact",
            &alice_contact_id_for_bob,
            "secret once",
        )
        .await
        .unwrap();

        let alice_report = DesktopRuntime::receive(&alice_dir).await.unwrap();
        assert_eq!(alice_report.snapshot.contacts.len(), 1);
        assert!(alice_report
            .snapshot
            .messages
            .iter()
            .any(|message| message.body == "hello 😀"));

        let received_image = alice_report
            .snapshot
            .messages
            .iter()
            .find(|message| message.content.kind == "image")
            .and_then(|message| message.content.attachment.as_ref())
            .expect("image attachment should be visible after reassembly");
        let received_image_path = received_image.local_path.as_ref().unwrap();
        assert_eq!(
            fs::read(received_image_path).unwrap(),
            b"not-a-real-png-but-still-private-bytes"
        );
        assert!(alice_report
            .snapshot
            .messages
            .iter()
            .any(|message| message.content.kind == "sticker"));
        let burn_message = alice_report
            .snapshot
            .messages
            .iter()
            .find(|message| message.content.kind == "burn")
            .expect("burn message should be received");
        assert_eq!(burn_message.content.text.as_deref(), Some("secret once"));

        let alice_contact_id_for_alice = alice_report.snapshot.contacts[0].id.clone();
        let opened = DesktopRuntime::open_burn_message(
            &alice_dir,
            "contact",
            &alice_contact_id_for_alice,
            &burn_message.id,
        )
        .await
        .unwrap();
        assert!(opened
            .messages
            .iter()
            .any(|message| message.id == burn_message.id && message.content.destroyed));

        let bob_destroy_report = DesktopRuntime::receive(&bob_dir).await.unwrap();
        assert!(bob_destroy_report
            .snapshot
            .messages
            .iter()
            .any(|message| message.content.kind == "burn" && message.content.destroyed));

        let deleted = DesktopRuntime::delete_contact(&bob_dir, &alice_contact_id_for_bob).unwrap();
        assert!(deleted.contacts.is_empty());
        assert!(deleted.messages.is_empty());

        handle.abort();
        let _ = fs::remove_dir_all(alice_dir);
        let _ = fs::remove_dir_all(bob_dir);
    }

    #[tokio::test]
    async fn large_attachment_chunks_fit_relay_http_limits() {
        let (addr, handle) = secure_chat_relay::spawn_ephemeral().await.unwrap();
        let relay_url = format!("http://{addr}");
        let alice_dir = test_data_dir("alice-large-attachment");
        let bob_dir = test_data_dir("bob-large-attachment");

        DesktopRuntime::bootstrap(&alice_dir, "Alice", &relay_url)
            .await
            .unwrap();
        DesktopRuntime::bootstrap(&bob_dir, "Bob", &relay_url)
            .await
            .unwrap();

        let alice_invite = DesktopRuntime::invite(&alice_dir).unwrap().invite_uri;
        let bob_snapshot = DesktopRuntime::add_contact(&bob_dir, "Alice", &alice_invite).unwrap();
        let alice_contact_id_for_bob = bob_snapshot.contacts[0].id.clone();

        let file_path = bob_dir.join("large-photo.jpg");
        let bytes: Vec<u8> = (0..(600 * 1024 + 37))
            .map(|index| (index % 251) as u8)
            .collect();
        fs::write(&file_path, &bytes).unwrap();

        DesktopRuntime::send_attachment(
            &bob_dir,
            "contact",
            &alice_contact_id_for_bob,
            file_path.to_str().unwrap(),
            "image",
        )
        .await
        .unwrap();

        let alice_report = DesktopRuntime::receive(&alice_dir).await.unwrap();
        let attachment = alice_report
            .snapshot
            .messages
            .iter()
            .find(|message| message.content.kind == "image")
            .and_then(|message| message.content.attachment.as_ref())
            .expect("large image attachment should be reassembled");
        let received_path = attachment.local_path.as_ref().unwrap();
        assert_eq!(fs::read(received_path).unwrap(), bytes);

        handle.abort();
        let _ = fs::remove_dir_all(alice_dir);
        let _ = fs::remove_dir_all(bob_dir);
    }

    #[test]
    fn extract_invite_uri_accepts_surrounding_text() {
        let input = "Alice invite: schat://invite/abc_DEF-123. Please add me.";
        let normalized = extract_invite_uri(input).unwrap();
        assert_eq!(normalized, "schat://invite/abc_DEF-123");
    }

    #[test]
    fn extract_invite_uri_rejects_missing_payload() {
        assert!(matches!(
            extract_invite_uri("schat://invite/"),
            Err(DesktopError::InvalidInvite)
        ));
    }

    #[test]
    fn encrypted_text_round_trips_and_plaintext_stays_readable() {
        let key = random_bytes::<32>();
        let encrypted = encrypt_text(&key, "https://relay.example").unwrap();
        assert!(encrypted.starts_with(ENCRYPTED_TEXT_PREFIX));
        assert_eq!(
            decrypt_text_if_needed(&key, &encrypted).unwrap(),
            "https://relay.example"
        );
        assert_eq!(
            decrypt_text_if_needed(&key, "https://legacy.example").unwrap(),
            "https://legacy.example"
        );
    }
}
