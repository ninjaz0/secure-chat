use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection};
use secure_chat_core::{
    verify_relay_auth_for_request, AccountId, DeviceId, DevicePreKeyBundle, DrainReceiptsResponse,
    DrainRequest, DrainResponse, QueuedMessage, QueuedReceipt, ReceiptKind, ReceiptRequest,
    RegisterRequest, RegisterResponse, RelayAuth, RelayCommand, RelayCommandResponse, SendRequest,
    RELAY_QUIC_ALPN,
};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{self, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct AppState {
    inner: Arc<RwLock<RelayState>>,
    db_path: Option<Arc<PathBuf>>,
}

#[derive(Default)]
struct RelayState {
    devices: HashMap<AccountId, HashMap<DeviceId, DevicePreKeyBundle>>,
    queues: HashMap<DeviceId, VecDeque<QueuedMessage>>,
    receipts: HashMap<DeviceId, VecDeque<QueuedReceipt>>,
    auth_nonces: HashMap<DeviceId, VecDeque<AuthNonceRecord>>,
}

#[derive(Clone)]
struct AuthNonceRecord {
    issued_unix: u64,
    nonce: [u8; 16],
}

impl AppState {
    pub fn memory() -> Self {
        Self::default()
    }

    pub fn persistent(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        init_relay_db(&path).map_err(sqlite_to_io)?;
        let inner = load_relay_state(&path).map_err(sqlite_to_io)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
            db_path: Some(Arc::new(path)),
        })
    }

    fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref().map(PathBuf::as_path)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/accounts", post(register_device))
        .route("/v1/accounts/:account_id/devices", get(list_devices))
        .route("/v1/messages", post(send_message))
        .route("/v1/messages/drain", post(drain_messages))
        .route("/v1/receipts", post(send_receipt))
        .route("/v1/receipts/drain", post(drain_receipts))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn run(addr: SocketAddr) -> std::io::Result<()> {
    run_http_with_state(addr, AppState::default()).await
}

pub async fn run_http_with_state(addr: SocketAddr, state: AppState) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "secure-chat relay listening");
    axum::serve(listener, router(state)).await
}

pub async fn run_https(
    addr: SocketAddr,
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
) -> std::io::Result<()> {
    run_https_with_state(addr, cert_path, key_path, AppState::default()).await
}

pub async fn run_https_with_state(
    addr: SocketAddr,
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
    state: AppState,
) -> std::io::Result<()> {
    install_crypto_provider();
    let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert_path, key_path).await?;
    tracing::info!(%addr, "secure-chat HTTPS relay listening");
    axum_server::bind_rustls(addr, config)
        .serve(router(state).into_make_service())
        .await
}

pub async fn run_quic(
    addr: SocketAddr,
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_quic_with_state(addr, cert_path, key_path, AppState::default()).await
}

pub async fn run_quic_with_state(
    addr: SocketAddr,
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    install_crypto_provider();
    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            load_certs(cert_path.as_ref())?,
            load_private_key(key_path.as_ref())?,
        )?;
    crypto.alpn_protocols = vec![RELAY_QUIC_ALPN.to_vec()];
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(crypto)?,
    ));
    let endpoint = quinn::Endpoint::server(server_config, addr)?;
    tracing::info!(%addr, "secure-chat QUIC relay listening");
    while let Some(connecting) = endpoint.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_quic_connection(state, connecting).await {
                tracing::warn!(%err, "QUIC connection failed");
            }
        });
    }
    Ok(())
}

pub async fn spawn_ephemeral() -> std::io::Result<(SocketAddr, JoinHandle<std::io::Result<()>>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let handle =
        tokio::spawn(async move { axum::serve(listener, router(AppState::default())).await });
    Ok((addr, handle))
}

pub async fn handle_command(state: AppState, command: RelayCommand) -> RelayCommandResponse {
    match command {
        RelayCommand::Health => RelayCommandResponse::Health(health_value()),
        RelayCommand::RegisterDevice(request) => match register_device_inner(&state, request).await
        {
            Ok(response) => RelayCommandResponse::RegisterDevice(response),
            Err(status) => error_response(status, "register device failed"),
        },
        RelayCommand::ListDevices { account_id } => {
            match list_devices_inner(&state, account_id).await {
                Ok(devices) => RelayCommandResponse::ListDevices(devices),
                Err(status) => error_response(status, "account not found"),
            }
        }
        RelayCommand::SendMessage(request) => match send_message_inner(&state, request).await {
            Ok(message) => RelayCommandResponse::SendMessage(message),
            Err(status) => error_response(status, "send message failed"),
        },
        RelayCommand::DrainMessages(request) => match drain_messages_inner(&state, request).await {
            Ok(response) => RelayCommandResponse::DrainMessages(response),
            Err(status) => error_response(status, "drain messages failed"),
        },
        RelayCommand::SendReceipt(request) => match send_receipt_inner(&state, request).await {
            Ok(receipt) => RelayCommandResponse::SendReceipt(receipt),
            Err(status) => error_response(status, "send receipt failed"),
        },
        RelayCommand::DrainReceipts(request) => match drain_receipts_inner(&state, request).await {
            Ok(response) => RelayCommandResponse::DrainReceipts(response),
            Err(status) => error_response(status, "drain receipts failed"),
        },
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(health_value())
}

fn health_value() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "service": "secure-chat-relay",
        "stores_plaintext": false,
        "transports": ["http", "https", "quic"],
        "receipts": ["delivered", "read"],
        "device_auth": "ed25519_request_signatures",
    })
}

async fn register_device(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, StatusCode> {
    register_device_inner(&state, request).await.map(Json)
}

async fn register_device_inner(
    state: &AppState,
    request: RegisterRequest,
) -> Result<RegisterResponse, StatusCode> {
    request
        .bundle
        .verify()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let account_id = request.bundle.identity.account_id;
    let device_id = request.bundle.identity.device_id;
    {
        let mut inner = state.inner.write().await;
        let mut signed_request = request.clone();
        signed_request.auth = None;
        verify_relay_auth(
            &mut inner,
            account_id,
            device_id,
            &request.bundle.identity.device_signing_public,
            "register_device",
            &signed_request,
            request.auth.as_ref(),
        )?;
    }
    persist_device(state.db_path(), account_id, device_id, &request.bundle).map_err(|err| {
        tracing::error!(%err, "persist device failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut inner = state.inner.write().await;
    inner
        .devices
        .entry(account_id)
        .or_default()
        .insert(device_id, request.bundle);
    inner.queues.entry(device_id).or_default();
    inner.receipts.entry(device_id).or_default();
    Ok(RegisterResponse {
        account_id,
        device_id,
    })
}

async fn list_devices(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<AccountId>,
) -> Result<Json<Vec<DevicePreKeyBundle>>, StatusCode> {
    list_devices_inner(&state, account_id).await.map(Json)
}

async fn list_devices_inner(
    state: &AppState,
    account_id: AccountId,
) -> Result<Vec<DevicePreKeyBundle>, StatusCode> {
    let inner = state.inner.read().await;
    let devices = inner
        .devices
        .get(&account_id)
        .ok_or(StatusCode::NOT_FOUND)?
        .values()
        .cloned()
        .collect();
    Ok(devices)
}

async fn send_message(
    State(state): State<AppState>,
    Json(request): Json<SendRequest>,
) -> Result<Json<QueuedMessage>, StatusCode> {
    send_message_inner(&state, request).await.map(Json)
}

async fn send_message_inner(
    state: &AppState,
    request: SendRequest,
) -> Result<QueuedMessage, StatusCode> {
    let mut inner = state.inner.write().await;
    let sender_account_id = request.sender_account_id.ok_or(StatusCode::UNAUTHORIZED)?;
    let sender_device_id = request.sender_device_id.ok_or(StatusCode::UNAUTHORIZED)?;
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let sender_public = device_signing_public(&inner, sender_account_id, sender_device_id)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        sender_account_id,
        sender_device_id,
        &sender_public,
        "send_message",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let exists = inner
        .devices
        .get(&request.to_account_id)
        .and_then(|devices| devices.get(&request.to_device_id))
        .is_some();
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    let message = QueuedMessage {
        id: Uuid::new_v4(),
        sender_account_id: Some(sender_account_id),
        sender_device_id: Some(sender_device_id),
        transport_kind: request.transport_kind,
        sealed_sender: request.sealed_sender,
        ciphertext: request.ciphertext,
        received_unix: now_unix(),
        expires_unix: request.expires_unix,
    };
    persist_message(state.db_path(), request.to_device_id, &message).map_err(|err| {
        tracing::error!(%err, "persist message failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    inner
        .queues
        .entry(request.to_device_id)
        .or_default()
        .push_back(message.clone());
    Ok(message)
}

async fn drain_messages(
    State(state): State<AppState>,
    Json(request): Json<DrainRequest>,
) -> Result<Json<DrainResponse>, StatusCode> {
    drain_messages_inner(&state, request).await.map(Json)
}

async fn drain_messages_inner(
    state: &AppState,
    request: DrainRequest,
) -> Result<DrainResponse, StatusCode> {
    let mut inner = state.inner.write().await;
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let device_public = device_signing_public(&inner, request.account_id, request.device_id)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        request.account_id,
        request.device_id,
        &device_public,
        "drain_messages",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let now = now_unix();
    let queue = inner.queues.entry(request.device_id).or_default();
    let mut messages = Vec::new();
    let mut delivery_receipts = Vec::new();
    let mut drained_message_ids = Vec::new();
    let mut expired_message_ids = Vec::new();
    while let Some(message) = queue.pop_front() {
        if message.expires_unix.is_none_or(|expires| expires >= now) {
            drained_message_ids.push(message.id);
            if let (Some(sender_account_id), Some(sender_device_id)) =
                (message.sender_account_id, message.sender_device_id)
            {
                delivery_receipts.push((
                    sender_device_id,
                    QueuedReceipt {
                        id: Uuid::new_v4(),
                        message_id: message.id,
                        from_account_id: request.account_id,
                        from_device_id: request.device_id,
                        kind: ReceiptKind::Delivered,
                        at_unix: now,
                    },
                ));
                let _ = sender_account_id;
            }
            messages.push(message);
        } else {
            expired_message_ids.push(message.id);
        }
    }
    for (sender_device_id, receipt) in delivery_receipts {
        if let Err(err) = persist_receipt(state.db_path(), sender_device_id, &receipt) {
            tracing::error!(%err, "persist delivery receipt failed");
        }
        inner
            .receipts
            .entry(sender_device_id)
            .or_default()
            .push_back(receipt);
    }
    for message_id in drained_message_ids.into_iter().chain(expired_message_ids) {
        if let Err(err) = delete_message(state.db_path(), message_id) {
            tracing::error!(%err, "delete drained message failed");
        }
    }
    Ok(DrainResponse { messages })
}

async fn send_receipt(
    State(state): State<AppState>,
    Json(request): Json<ReceiptRequest>,
) -> Result<Json<QueuedReceipt>, StatusCode> {
    send_receipt_inner(&state, request).await.map(Json)
}

async fn send_receipt_inner(
    state: &AppState,
    request: ReceiptRequest,
) -> Result<QueuedReceipt, StatusCode> {
    let mut inner = state.inner.write().await;
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let sender_public =
        device_signing_public(&inner, request.from_account_id, request.from_device_id)
            .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        request.from_account_id,
        request.from_device_id,
        &sender_public,
        "send_receipt",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let exists = inner
        .devices
        .get(&request.to_account_id)
        .and_then(|devices| devices.get(&request.to_device_id))
        .is_some();
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    let receipt = QueuedReceipt {
        id: Uuid::new_v4(),
        message_id: request.message_id,
        from_account_id: request.from_account_id,
        from_device_id: request.from_device_id,
        kind: request.kind,
        at_unix: request.at_unix,
    };
    persist_receipt(state.db_path(), request.to_device_id, &receipt).map_err(|err| {
        tracing::error!(%err, "persist receipt failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    inner
        .receipts
        .entry(request.to_device_id)
        .or_default()
        .push_back(receipt.clone());
    Ok(receipt)
}

async fn drain_receipts(
    State(state): State<AppState>,
    Json(request): Json<DrainRequest>,
) -> Result<Json<DrainReceiptsResponse>, StatusCode> {
    drain_receipts_inner(&state, request).await.map(Json)
}

async fn drain_receipts_inner(
    state: &AppState,
    request: DrainRequest,
) -> Result<DrainReceiptsResponse, StatusCode> {
    let mut inner = state.inner.write().await;
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let device_public = device_signing_public(&inner, request.account_id, request.device_id)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        request.account_id,
        request.device_id,
        &device_public,
        "drain_receipts",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let queue = inner.receipts.entry(request.device_id).or_default();
    let mut receipts = Vec::new();
    let mut receipt_ids = Vec::new();
    while let Some(receipt) = queue.pop_front() {
        receipt_ids.push(receipt.id);
        receipts.push(receipt);
    }
    for receipt_id in receipt_ids {
        if let Err(err) = delete_receipt(state.db_path(), receipt_id) {
            tracing::error!(%err, "delete drained receipt failed");
        }
    }
    Ok(DrainReceiptsResponse { receipts })
}

fn device_signing_public(
    inner: &RelayState,
    account_id: AccountId,
    device_id: DeviceId,
) -> Option<[u8; 32]> {
    inner
        .devices
        .get(&account_id)
        .and_then(|devices| devices.get(&device_id))
        .map(|bundle| bundle.identity.device_signing_public)
}

fn verify_relay_auth<T: serde::Serialize>(
    inner: &mut RelayState,
    account_id: AccountId,
    device_id: DeviceId,
    device_signing_public: &[u8; 32],
    action: &str,
    unsigned_request: &T,
    auth: Option<&RelayAuth>,
) -> Result<(), StatusCode> {
    let auth = auth.ok_or(StatusCode::UNAUTHORIZED)?;
    if auth.account_id != account_id || auth.device_id != device_id {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let now = now_unix();
    verify_relay_auth_for_request(device_signing_public, action, unsigned_request, auth, now)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let nonces = inner.auth_nonces.entry(device_id).or_default();
    nonces.retain(|record| record.issued_unix + secure_chat_core::RELAY_AUTH_MAX_SKEW_SECS >= now);
    if nonces.iter().any(|record| record.nonce == auth.nonce) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    nonces.push_back(AuthNonceRecord {
        issued_unix: auth.issued_unix,
        nonce: auth.nonce,
    });
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn init_relay_db(path: &Path) -> rusqlite::Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        CREATE TABLE IF NOT EXISTS devices (
            account_id TEXT NOT NULL,
            device_id TEXT NOT NULL PRIMARY KEY,
            bundle_json TEXT NOT NULL,
            updated_unix INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_devices_account ON devices(account_id);
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT NOT NULL PRIMARY KEY,
            to_device_id TEXT NOT NULL,
            message_json TEXT NOT NULL,
            received_unix INTEGER NOT NULL,
            expires_unix INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_messages_device ON messages(to_device_id, received_unix);
        CREATE TABLE IF NOT EXISTS receipts (
            id TEXT NOT NULL PRIMARY KEY,
            to_device_id TEXT NOT NULL,
            receipt_json TEXT NOT NULL,
            at_unix INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_receipts_device ON receipts(to_device_id, at_unix);
        "#,
    )?;
    conn.execute(
        "DELETE FROM messages WHERE expires_unix IS NOT NULL AND expires_unix < ?1",
        params![now_unix() as i64],
    )?;
    Ok(())
}

fn load_relay_state(path: &Path) -> rusqlite::Result<RelayState> {
    let conn = Connection::open(path)?;
    let mut state = RelayState::default();

    let mut devices = conn.prepare("SELECT bundle_json FROM devices")?;
    let rows = devices.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let bundle: DevicePreKeyBundle = parse_json(&row?)?;
        state
            .devices
            .entry(bundle.identity.account_id)
            .or_default()
            .insert(bundle.identity.device_id, bundle);
    }

    let mut messages =
        conn.prepare("SELECT to_device_id, message_json FROM messages ORDER BY received_unix ASC")?;
    let rows = messages.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (device_id, json) = row?;
        let device_id = parse_uuid(&device_id)?;
        let message: QueuedMessage = parse_json(&json)?;
        state
            .queues
            .entry(device_id)
            .or_default()
            .push_back(message);
    }

    let mut receipts =
        conn.prepare("SELECT to_device_id, receipt_json FROM receipts ORDER BY at_unix ASC")?;
    let rows = receipts.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (device_id, json) = row?;
        let device_id = parse_uuid(&device_id)?;
        let receipt: QueuedReceipt = parse_json(&json)?;
        state
            .receipts
            .entry(device_id)
            .or_default()
            .push_back(receipt);
    }

    Ok(state)
}

fn persist_device(
    db_path: Option<&Path>,
    account_id: AccountId,
    device_id: DeviceId,
    bundle: &DevicePreKeyBundle,
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO devices(account_id, device_id, bundle_json, updated_unix)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(device_id) DO UPDATE SET
            account_id = excluded.account_id,
            bundle_json = excluded.bundle_json,
            updated_unix = excluded.updated_unix",
        params![
            account_id.to_string(),
            device_id.to_string(),
            to_json(bundle)?,
            now_unix() as i64
        ],
    )?;
    Ok(())
}

fn persist_message(
    db_path: Option<&Path>,
    to_device_id: DeviceId,
    message: &QueuedMessage,
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO messages(id, to_device_id, message_json, received_unix, expires_unix)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            message.id.to_string(),
            to_device_id.to_string(),
            to_json(message)?,
            message.received_unix as i64,
            message.expires_unix.map(|expires| expires as i64)
        ],
    )?;
    Ok(())
}

fn persist_receipt(
    db_path: Option<&Path>,
    to_device_id: DeviceId,
    receipt: &QueuedReceipt,
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO receipts(id, to_device_id, receipt_json, at_unix)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            receipt.id.to_string(),
            to_device_id.to_string(),
            to_json(receipt)?,
            receipt.at_unix as i64
        ],
    )?;
    Ok(())
}

fn delete_message(db_path: Option<&Path>, message_id: Uuid) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "DELETE FROM messages WHERE id = ?1",
        params![message_id.to_string()],
    )?;
    Ok(())
}

fn delete_receipt(db_path: Option<&Path>, receipt_id: Uuid) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "DELETE FROM receipts WHERE id = ?1",
        params![receipt_id.to_string()],
    )?;
    Ok(())
}

fn to_json<T: serde::Serialize>(value: &T) -> rusqlite::Result<String> {
    serde_json::to_string(value).map_err(|err| rusqlite::Error::ToSqlConversionFailure(err.into()))
}

fn parse_json<T: serde::de::DeserializeOwned>(json: &str) -> rusqlite::Result<T> {
    serde_json::from_str(json).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, err.into())
    })
}

fn parse_uuid(value: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, err.into())
    })
}

fn sqlite_to_io(error: rusqlite::Error) -> io::Error {
    io::Error::other(error)
}

fn error_response(status: StatusCode, message: &str) -> RelayCommandResponse {
    RelayCommandResponse::Error {
        status: status.as_u16(),
        message: message.to_string(),
    }
}

async fn handle_quic_connection(
    state: AppState,
    connecting: quinn::Incoming,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = connecting.await?;
    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        let state = state.clone();
        tokio::spawn(async move {
            let response = match recv.read_to_end(16 * 1024 * 1024).await {
                Ok(bytes) => match serde_json::from_slice::<RelayCommand>(&bytes) {
                    Ok(command) => handle_command(state, command).await,
                    Err(err) => RelayCommandResponse::Error {
                        status: 400,
                        message: err.to_string(),
                    },
                },
                Err(err) => RelayCommandResponse::Error {
                    status: 400,
                    message: err.to_string(),
                },
            };
            if let Ok(bytes) = serde_json::to_vec(&response) {
                let _ = send.write_all(&bytes).await;
                let _ = send.finish();
            }
        });
    }
    Ok(())
}

fn load_certs(path: &Path) -> io::Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::certs(&mut reader).collect()
}

fn load_private_key(path: &Path) -> io::Result<rustls::pki_types::PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing private key"))
}

fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use secure_chat_core::{DeviceKeyMaterial, TransportKind};

    #[tokio::test]
    async fn persistent_relay_recovers_messages_and_receipts_after_restart() {
        let db_path =
            std::env::temp_dir().join(format!("secure-chat-relay-{}.sqlite3", Uuid::new_v4()));
        let keys = DeviceKeyMaterial::generate(1);
        let bundle = keys.pre_key_bundle();

        let state = AppState::persistent(&db_path).unwrap();
        let mut register_request = RegisterRequest {
            bundle: bundle.clone(),
            auth: None,
        };
        register_request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                &keys,
                "register_device",
                &register_request,
                now_unix(),
            )
            .unwrap(),
        );
        register_device_inner(&state, register_request)
            .await
            .unwrap();

        let mut send_request = SendRequest {
            sender_account_id: Some(keys.account_id),
            sender_device_id: Some(keys.device_id),
            to_account_id: keys.account_id,
            to_device_id: keys.device_id,
            transport_kind: TransportKind::RelayHttps,
            sealed_sender: None,
            ciphertext: b"ciphertext".to_vec(),
            expires_unix: None,
            auth: None,
        };
        send_request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                &keys,
                "send_message",
                &send_request,
                now_unix(),
            )
            .unwrap(),
        );
        let message = send_message_inner(&state, send_request).await.unwrap();

        let restarted = AppState::persistent(&db_path).unwrap();
        let devices = list_devices_inner(&restarted, keys.account_id)
            .await
            .unwrap();
        assert_eq!(devices, vec![bundle]);

        let drained = drain_messages_inner(&restarted, signed_drain(&keys, "drain_messages"))
            .await
            .unwrap();
        assert_eq!(drained.messages.len(), 1);
        assert_eq!(drained.messages[0].id, message.id);

        let restarted = AppState::persistent(&db_path).unwrap();
        assert!(
            drain_messages_inner(&restarted, signed_drain(&keys, "drain_messages"))
                .await
                .unwrap()
                .messages
                .is_empty()
        );

        let receipts = drain_receipts_inner(&restarted, signed_drain(&keys, "drain_receipts"))
            .await
            .unwrap();
        assert_eq!(receipts.receipts.len(), 1);
        assert_eq!(receipts.receipts[0].message_id, message.id);
        assert_eq!(receipts.receipts[0].kind, ReceiptKind::Delivered);

        let restarted = AppState::persistent(&db_path).unwrap();
        assert!(
            drain_receipts_inner(&restarted, signed_drain(&keys, "drain_receipts"))
                .await
                .unwrap()
                .receipts
                .is_empty()
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn relay_rejects_unsigned_and_replayed_device_requests() {
        let keys = DeviceKeyMaterial::generate(1);
        let state = AppState::memory();
        let mut register_request = RegisterRequest {
            bundle: keys.pre_key_bundle(),
            auth: None,
        };
        register_request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                &keys,
                "register_device",
                &register_request,
                now_unix(),
            )
            .unwrap(),
        );
        register_device_inner(&state, register_request)
            .await
            .unwrap();

        let unsigned = SendRequest {
            sender_account_id: Some(keys.account_id),
            sender_device_id: Some(keys.device_id),
            to_account_id: keys.account_id,
            to_device_id: keys.device_id,
            transport_kind: TransportKind::RelayHttps,
            sealed_sender: None,
            ciphertext: b"ciphertext".to_vec(),
            expires_unix: None,
            auth: None,
        };
        assert_eq!(
            send_message_inner(&state, unsigned).await.unwrap_err(),
            StatusCode::UNAUTHORIZED
        );

        let drain = signed_drain(&keys, "drain_messages");
        drain_messages_inner(&state, drain.clone()).await.unwrap();
        assert_eq!(
            drain_messages_inner(&state, drain).await.unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }

    fn signed_drain(keys: &DeviceKeyMaterial, action: &str) -> DrainRequest {
        let mut request = DrainRequest {
            account_id: keys.account_id,
            device_id: keys.device_id,
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(keys, action, &request, now_unix())
                .unwrap(),
        );
        request
    }
}
