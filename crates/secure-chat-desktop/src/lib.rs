use base64::engine::general_purpose::STANDARD;
use base64::Engine;
#[cfg(not(target_os = "android"))]
use keyring::Entry;
use rusqlite::{params, Connection, OptionalExtension};
use secure_chat_client::{RelayClient, RelayEnvelope};
use secure_chat_core::crypto::{
    decrypt_aead, encrypt_aead, random_bytes, sha256, CipherSuite, Key32,
};
use secure_chat_core::safety::to_hex;
use secure_chat_core::{
    accept_session_as_responder_consuming_prekey, safety_number, start_session_as_initiator,
    DeviceKeyMaterial, Invite, InviteMode, PlainMessage, PublicDeviceIdentity, RatchetSession,
    ReceiptKind, ReceiptRequest, SendRequest, TransportFrame, TransportKind,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

#[cfg(not(target_os = "android"))]
const KEYCHAIN_SERVICE: &str = "dev.local.securechat";
const PROFILE_ID: &str = "default";
const ENCRYPTED_TEXT_PREFIX: &str = "enc:v1:";
const TEMP_INVITE_TTL_SECS: u64 = 15 * 60;
const TEMP_CONNECTION_TTL_SECS: u64 = 24 * 60 * 60;
const TEMP_MESSAGE_TTL_SECS: u64 = 10 * 60;
const MAX_TEMP_CONNECTIONS: usize = 32;
const MAX_TEMP_MESSAGES_PER_CONNECTION: usize = 200;

#[derive(Debug, Error)]
pub enum DesktopError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(not(target_os = "android"))]
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub ready: bool,
    pub profile: Option<AppProfile>,
    pub contacts: Vec<ContactSummary>,
    pub messages: Vec<ChatMessageView>,
    pub temporary_connections: Vec<TemporaryConnectionSummary>,
    pub temporary_messages: Vec<TemporaryMessageView>,
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
    pub status: MessageStatus,
    pub sent_at_unix: u64,
    pub received_at_unix: Option<u64>,
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
                temporary_connections: Vec::new(),
                temporary_messages: Vec::new(),
            });
        };
        let keys = self.load_device_keys()?;
        let invite_uri =
            Invite::new(keys.pre_key_bundle(), Some(profile.relay_url.clone()), None).to_uri()?;
        let storage_key = self.load_storage_key()?;
        let contacts = self.contact_summaries(&storage_key)?;
        let messages = self.message_views(&storage_key)?;
        let temporary_connections = self.temporary_connection_summaries(&storage_key)?;
        let temporary_messages = self.temporary_message_views(&storage_key)?;
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
            temporary_connections,
            temporary_messages,
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
            invite_uri: Invite::new(keys.pre_key_bundle(), Some(profile.relay_url), None)
                .to_uri()?,
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
            invite_uri: Invite::temporary(
                keys.pre_key_bundle(),
                Some(profile.relay_url),
                Some(expires_unix),
            )
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
        let envelope = RelayEnvelope { initial, wire };
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
        let relay = RelayClient::new(profile.relay_url);
        relay.register_device(&keys).await?;

        let mut session = runtime.load_session(contact_id)?;
        let initial = if session.is_some() {
            None
        } else if let Some(invite_uri) = &contact.invite_uri {
            let invite = Invite::from_uri(invite_uri)?;
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
        let envelope = RelayEnvelope { initial, wire };
        let payload = serde_json::to_vec(&envelope)?;
        let frame = TransportFrame::protect(&payload, &padding_profile(payload.len()))?;
        let relay_ciphertext = serde_json::to_vec(&frame)?;
        relay
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
                    expires_unix: Some(now_unix() + 7 * 24 * 60 * 60),
                    auth: None,
                },
            )
            .await
            .map(|sent| {
                runtime
                    .insert_message(
                        contact_id,
                        MessageDirection::Outgoing,
                        body,
                        MessageStatus::Sent,
                        Some(relay_ciphertext),
                        Some(sent.id.to_string()),
                        &storage_key,
                    )
                    .map(|_| sent)
            })??;
        runtime.save_session(contact_id, &session)?;
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
            let envelope: RelayEnvelope = serde_json::from_slice(&frame.expose()?)?;
            let remote_device_id = envelope.wire.sender_device_id;
            let contact = runtime.contact_by_device(&remote_device_id.to_string())?;
            let mut temporary_connection = if contact.is_none() {
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
            if let Some(contact) = contact {
                runtime.save_session(&contact.id, &session)?;
                runtime.insert_message(
                    &contact.id,
                    MessageDirection::Incoming,
                    &plain.body,
                    MessageStatus::Received,
                    Some(item.ciphertext),
                    Some(item.id.to_string()),
                    &storage_key,
                )?;
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
            "#,
        )?;
        self.ensure_sessions_schema()?;
        self.encrypt_existing_metadata_if_possible()?;
        self.delete_expired_temporary_connections()?;
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
             FROM messages ORDER BY sent_at_unix ASC",
        )?;
        let rows = statement
            .query_map([], |row| {
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
                let body = decrypt_body(storage_key, &nonce, &ciphertext)?;
                Ok(ChatMessageView {
                    id,
                    contact_id,
                    direction: MessageDirection::from_str(&direction),
                    body,
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
             FROM temporary_messages ORDER BY sent_at_unix ASC",
        )?;
        let rows =
            statement
                .query_map([], |row| {
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
                    let body = decrypt_body(storage_key, &nonce, &ciphertext)?;
                    Ok(TemporaryMessageView {
                        id,
                        connection_id,
                        direction: MessageDirection::from_str(&direction),
                        body,
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
        row.map(|(nonce, ciphertext)| decrypt_body(storage_key, &nonce, &ciphertext))
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
        row.map(|(nonce, ciphertext)| decrypt_body(storage_key, &nonce, &ciphertext))
            .transpose()
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

    #[cfg(not(target_os = "android"))]
    fn load_secret(&self, kind: &str) -> Result<String, DesktopError> {
        Ok(self.keychain_entry(kind)?.get_password()?)
    }

    #[cfg(not(target_os = "android"))]
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

    #[cfg(not(target_os = "android"))]
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
