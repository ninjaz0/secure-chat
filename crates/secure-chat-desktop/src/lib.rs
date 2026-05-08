use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use keyring::Entry;
use rusqlite::{params, Connection, OptionalExtension};
use secure_chat_client::{RelayClient, RelayEnvelope};
use secure_chat_core::crypto::{
    decrypt_aead, encrypt_aead, random_bytes, sha256, CipherSuite, Key32,
};
use secure_chat_core::safety::to_hex;
use secure_chat_core::{
    accept_session_as_responder, safety_number, start_session_as_initiator, DeviceKeyMaterial,
    Invite, PlainMessage, RatchetSession, ReceiptKind, ReceiptRequest, SendRequest, TransportFrame,
    TransportKind,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

const KEYCHAIN_SERVICE: &str = "dev.local.securechat";
const PROFILE_ID: &str = "default";

#[derive(Debug, Error)]
pub enum DesktopError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("keychain error: {0}")]
    Keychain(#[from] keyring::Error),
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
    #[error("message body cannot be empty")]
    EmptyMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub ready: bool,
    pub profile: Option<AppProfile>,
    pub contacts: Vec<ContactSummary>,
    pub messages: Vec<ChatMessageView>,
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
pub struct ReceiveReport {
    pub received_count: usize,
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
            });
        };
        let keys = self.load_device_keys()?;
        let invite_uri =
            Invite::new(keys.pre_key_bundle(), Some(profile.relay_url.clone()), None).to_uri()?;
        let storage_key = self.load_storage_key()?;
        let contacts = self.contact_summaries(&storage_key)?;
        let messages = self.message_views(&storage_key)?;
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
        })
    }

    pub async fn update_relay(
        data_dir: impl AsRef<Path>,
        relay_url: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
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

    pub fn add_contact(
        data_dir: impl AsRef<Path>,
        display_name: &str,
        invite_uri: &str,
    ) -> Result<AppSnapshot, DesktopError> {
        let runtime = Self::open(data_dir)?;
        runtime.ensure_profile()?;
        runtime.add_contact_inner(display_name, invite_uri)?;
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
        relay.register_device(keys.pre_key_bundle()).await?;

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
            .send(SendRequest {
                sender_account_id: Some(keys.account_id),
                sender_device_id: Some(keys.device_id),
                to_account_id: session.remote_identity.account_id,
                to_device_id: session.remote_identity.device_id,
                transport_kind: TransportKind::WebSocketTls,
                sealed_sender: None,
                ciphertext: relay_ciphertext.clone(),
                expires_unix: Some(now_unix() + 7 * 24 * 60 * 60),
            })
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
        let keys = runtime.load_device_keys()?;
        let storage_key = runtime.load_storage_key()?;
        let relay = RelayClient::new(&profile.relay_url);
        relay.register_device(keys.pre_key_bundle()).await?;
        runtime.apply_receipts(
            relay
                .drain_receipts(keys.account_id, keys.device_id)
                .await?,
        )?;
        let queued = relay.drain(keys.account_id, keys.device_id).await?;
        let mut received_count = 0usize;

        for item in queued {
            let frame: TransportFrame = serde_json::from_slice(&item.ciphertext)?;
            let envelope: RelayEnvelope = serde_json::from_slice(&frame.expose()?)?;
            let remote_device_id = envelope.wire.sender_device_id;
            let mut contact = runtime.contact_by_device(&remote_device_id.to_string())?;
            if contact.is_none() {
                contact = Some(runtime.create_incoming_contact(&keys, &envelope)?);
            }
            let contact = contact.ok_or(DesktopError::ContactNotFound)?;
            let mut session = runtime.load_session(&contact.id)?;
            if session.is_none() {
                let initial = envelope
                    .initial
                    .as_ref()
                    .ok_or(DesktopError::ContactNotFound)?;
                session = Some(accept_session_as_responder(&keys, initial)?);
            }
            let mut session = session.ok_or(DesktopError::ContactNotFound)?;
            let plain = session.decrypt(&envelope.wire)?;
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
            if let (Some(sender_account_id), Some(sender_device_id)) =
                (item.sender_account_id, item.sender_device_id)
            {
                let _ = relay
                    .send_receipt(ReceiptRequest {
                        message_id: item.id,
                        from_account_id: keys.account_id,
                        from_device_id: keys.device_id,
                        to_account_id: sender_account_id,
                        to_device_id: sender_device_id,
                        kind: ReceiptKind::Read,
                        at_unix: now_unix(),
                    })
                    .await;
            }
            received_count += 1;
        }

        Ok(ReceiveReport {
            received_count,
            snapshot: runtime.snapshot()?,
        })
    }

    async fn bootstrap_profile(
        &self,
        display_name: &str,
        relay_url: &str,
    ) -> Result<(), DesktopError> {
        if self.profile_row()?.is_none() {
            let keys = DeviceKeyMaterial::generate(16);
            self.save_device_keys(&keys)?;
            self.save_storage_key(&random_bytes::<32>())?;
            self.conn.execute(
                "INSERT INTO profile (id, display_name, relay_url, created_at_unix, updated_at_unix)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![PROFILE_ID, display_name, relay_url, now_unix()],
            )?;
        } else {
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
            .register_device(keys.pre_key_bundle())
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
            "#,
        )?;
        self.ensure_sessions_schema()?;
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

    fn profile_row(&self) -> Result<Option<ProfileRow>, DesktopError> {
        self.conn
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
            .optional()
            .map_err(Into::into)
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
        let invite = Invite::from_uri(invite_uri)?;
        invite.bundle.verify()?;
        let remote = invite.bundle.identity.clone();
        let safety = safety_number(&[keys.public_identity()], std::slice::from_ref(&remote));
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let remote_identity_json = serde_json::to_string(&remote)?;
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
        envelope: &RelayEnvelope,
    ) -> Result<ContactRecord, DesktopError> {
        let remote = envelope
            .initial
            .as_ref()
            .ok_or(DesktopError::ContactNotFound)?
            .initiator_identity
            .clone();
        let suffix = remote
            .device_id
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        let display_name = format!("Incoming {suffix}");
        let safety = safety_number(&[keys.public_identity()], std::slice::from_ref(&remote));
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let remote_identity_json = serde_json::to_string(&remote)?;
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
        self.conn
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
            .optional()
            .map_err(Into::into)
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
        }
        Ok(())
    }

    fn load_device_keys(&self) -> Result<DeviceKeyMaterial, DesktopError> {
        let json = self.keychain_entry("device_keys")?.get_password()?;
        serde_json::from_str(&json).map_err(Into::into)
    }

    fn save_device_keys(&self, keys: &DeviceKeyMaterial) -> Result<(), DesktopError> {
        self.keychain_entry("device_keys")?
            .set_password(&serde_json::to_string(keys)?)?;
        Ok(())
    }

    fn load_storage_key(&self) -> Result<Key32, DesktopError> {
        let text = self.keychain_entry("storage_key")?.get_password()?;
        let bytes = STANDARD
            .decode(text)
            .map_err(|err| DesktopError::InvalidData(err.to_string()))?;
        bytes
            .try_into()
            .map_err(|_| DesktopError::InvalidData("invalid storage key length".to_string()))
    }

    fn save_storage_key(&self, key: &Key32) -> Result<(), DesktopError> {
        self.keychain_entry("storage_key")?
            .set_password(&STANDARD.encode(key))?;
        Ok(())
    }

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

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
