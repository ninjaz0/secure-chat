use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection};
use rustls_pki_types::pem::PemObject;
use secure_chat_core::{
    verify_relay_auth_for_request, AccountId, ApnsPlatform, ClaimMlsKeyPackageRequest,
    DeleteApnsTokenRequest, DeviceId, DevicePreKeyBundle, DrainReceiptsResponse, DrainRequest,
    DrainResponse, ListP2pCandidatesRequest, MlsKeyPackageResponse, P2pCandidate,
    P2pCandidateDraft, P2pCandidateKind, P2pCandidatesResponse, P2pProbeRequest, P2pProbeResponse,
    PublishMlsKeyPackageRequest, QueuedMessage, QueuedReceipt, ReceiptKind, ReceiptRequest,
    RegisterApnsTokenRequest, RegisterApnsTokenResponse, RegisterRequest, RegisterResponse,
    RelayAuth, RelayCommand, RelayCommandResponse, SendRequest, P2P_CANDIDATE_TTL_SECS,
    RELAY_QUIC_ALPN,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
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

const MAX_TOTAL_DEVICES: usize = 100_000;
const MAX_DEVICES_PER_ACCOUNT: usize = 16;
const MAX_QUEUE_MESSAGES_PER_DEVICE: usize = 1_024;
const MAX_RECEIPTS_PER_DEVICE: usize = 2_048;
const MAX_AUTH_NONCES_PER_DEVICE: usize = 2_048;
const MAX_MESSAGE_CIPHERTEXT_BYTES: usize = 1024 * 1024;
const MAX_SEALED_SENDER_BYTES: usize = 16 * 1024;
const MAX_APNS_TOKEN_BYTES: usize = 256;
const MAX_MLS_KEY_PACKAGE_BYTES: usize = 64 * 1024;
const MAX_MESSAGE_TTL_SECS: u64 = 30 * 24 * 60 * 60;
const MAX_P2P_PROBE_BYTES: usize = 4096;

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
    p2p_candidates: HashMap<DeviceId, Vec<P2pCandidate>>,
    mls_key_packages: HashMap<DeviceId, Vec<u8>>,
    apns_tokens: HashMap<DeviceId, Vec<ApnsTokenRecord>>,
    peer_links: HashSet<DevicePair>,
    receipt_grants: HashMap<Uuid, ReceiptGrant>,
}

#[derive(Clone)]
struct AuthNonceRecord {
    issued_unix: u64,
    nonce: [u8; 16],
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ApnsTokenRecord {
    token: String,
    platform: ApnsPlatform,
    updated_unix: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DevicePair {
    left: DeviceId,
    right: DeviceId,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ReceiptGrant {
    message_id: Uuid,
    sender_account_id: AccountId,
    sender_device_id: DeviceId,
    recipient_account_id: AccountId,
    recipient_device_id: DeviceId,
    expires_unix: u64,
}

impl DevicePair {
    fn new(a: DeviceId, b: DeviceId) -> Self {
        if a <= b {
            Self { left: a, right: b }
        } else {
            Self { left: b, right: a }
        }
    }
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
        .route("/v1/p2p/candidates", post(publish_p2p_candidates))
        .route("/v1/p2p/candidates/list", post(list_p2p_candidates))
        .route("/v1/mls/key-packages", post(publish_mls_key_package))
        .route("/v1/mls/key-packages/claim", post(claim_mls_key_package))
        .route("/v1/push/apns/token", post(register_apns_token))
        .route("/v1/push/apns/token/delete", post(delete_apns_token))
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

pub async fn run_p2p_rendezvous(
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_p2p_rendezvous_with_state(addr, AppState::default()).await
}

pub async fn run_p2p_rendezvous_with_state(
    addr: SocketAddr,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let socket = tokio::net::UdpSocket::bind(addr).await?;
    tracing::info!(%addr, "secure-chat P2P rendezvous listening");
    let mut buffer = vec![0u8; MAX_P2P_PROBE_BYTES];
    loop {
        let (len, peer_addr) = socket.recv_from(&mut buffer).await?;
        let response = match serde_json::from_slice::<P2pProbeRequest>(&buffer[..len]) {
            Ok(request) => match handle_p2p_probe(&state, request, peer_addr).await {
                Ok(response) => serde_json::to_vec(&response)?,
                Err(status) => serde_json::to_vec(&RelayCommandResponse::Error {
                    status: status.as_u16(),
                    message: "p2p probe failed".to_string(),
                })?,
            },
            Err(err) => serde_json::to_vec(&RelayCommandResponse::Error {
                status: 400,
                message: err.to_string(),
            })?,
        };
        let _ = socket.send_to(&response, peer_addr).await;
    }
}

pub async fn spawn_ephemeral() -> std::io::Result<(SocketAddr, JoinHandle<std::io::Result<()>>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let handle =
        tokio::spawn(async move { axum::serve(listener, router(AppState::default())).await });
    Ok((addr, handle))
}

pub async fn spawn_ephemeral_with_p2p() -> std::io::Result<(
    SocketAddr,
    SocketAddr,
    JoinHandle<std::io::Result<()>>,
    JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
)> {
    let state = AppState::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let http_addr = listener.local_addr()?;
    let udp_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    let p2p_addr = udp_socket.local_addr()?;
    drop(udp_socket);
    let http_state = state.clone();
    let http_handle = tokio::spawn(async move { axum::serve(listener, router(http_state)).await });
    let p2p_handle = tokio::spawn(run_p2p_rendezvous_with_state(p2p_addr, state));
    Ok((http_addr, p2p_addr, http_handle, p2p_handle))
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
        RelayCommand::PublishP2pCandidates(request) => {
            match publish_p2p_candidates_inner(&state, request).await {
                Ok(response) => RelayCommandResponse::PublishP2pCandidates(response),
                Err(status) => error_response(status, "publish p2p candidates failed"),
            }
        }
        RelayCommand::ListP2pCandidates(request) => {
            match list_p2p_candidates_inner(&state, request).await {
                Ok(response) => RelayCommandResponse::ListP2pCandidates(response),
                Err(status) => error_response(status, "list p2p candidates failed"),
            }
        }
        RelayCommand::PublishMlsKeyPackage(request) => {
            match publish_mls_key_package_inner(&state, request).await {
                Ok(response) => RelayCommandResponse::PublishMlsKeyPackage(response),
                Err(status) => error_response(status, "publish mls key package failed"),
            }
        }
        RelayCommand::ClaimMlsKeyPackage(request) => {
            match claim_mls_key_package_inner(&state, request).await {
                Ok(response) => RelayCommandResponse::ClaimMlsKeyPackage(response),
                Err(status) => error_response(status, "claim mls key package failed"),
            }
        }
        RelayCommand::RegisterApnsToken(request) => {
            match register_apns_token_inner(&state, request).await {
                Ok(response) => RelayCommandResponse::RegisterApnsToken(response),
                Err(status) => error_response(status, "register apns token failed"),
            }
        }
        RelayCommand::DeleteApnsToken(request) => {
            match delete_apns_token_inner(&state, request).await {
                Ok(response) => RelayCommandResponse::DeleteApnsToken(response),
                Err(status) => error_response(status, "delete apns token failed"),
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
        "p2p_rendezvous": "signed_udp_observed_address",
        "receipts": ["delivered", "read"],
        "push": {
            "apns": apns_config().is_some(),
            "payload": "generic_no_plaintext"
        },
        "device_auth": "ed25519_request_signatures",
    })
}

#[derive(Clone)]
struct ApnsConfig {
    team_id: String,
    key_id: String,
    private_key_pem: Vec<u8>,
    topic_ios: Option<String>,
    topic_macos: Option<String>,
    environment: ApnsEnvironment,
}

#[derive(Clone, Copy)]
enum ApnsEnvironment {
    Sandbox,
    Production,
}

#[derive(serde::Serialize)]
struct ApnsJwtClaims {
    iss: String,
    iat: u64,
}

fn apns_config() -> Option<ApnsConfig> {
    let team_id = env::var("SECURE_CHAT_APNS_TEAM_ID").ok()?;
    let key_id = env::var("SECURE_CHAT_APNS_KEY_ID").ok()?;
    let private_key_path = env::var("SECURE_CHAT_APNS_PRIVATE_KEY_PATH").ok()?;
    let private_key_pem = match std::fs::read(private_key_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!(%err, "APNs private key is not readable; push disabled");
            return None;
        }
    };
    let environment = match env::var("SECURE_CHAT_APNS_ENV")
        .unwrap_or_else(|_| "sandbox".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "production" | "prod" => ApnsEnvironment::Production,
        _ => ApnsEnvironment::Sandbox,
    };
    Some(ApnsConfig {
        team_id,
        key_id,
        private_key_pem,
        topic_ios: env::var("SECURE_CHAT_APNS_TOPIC_IOS").ok(),
        topic_macos: env::var("SECURE_CHAT_APNS_TOPIC_MACOS").ok(),
        environment,
    })
}

async fn send_apns_notifications(targets: Vec<ApnsTokenRecord>) {
    if targets.is_empty() {
        return;
    }
    let Some(config) = apns_config() else {
        return;
    };
    let jwt = match apns_jwt(&config) {
        Ok(jwt) => jwt,
        Err(err) => {
            tracing::warn!(%err, "APNs JWT creation failed; push skipped");
            return;
        }
    };
    let client = reqwest::Client::new();
    for target in targets {
        let Some(topic) = apns_topic(&config, target.platform) else {
            tracing::warn!(
                platform = target.platform.as_str(),
                "APNs topic is not configured; push skipped"
            );
            continue;
        };
        let url = format!(
            "{}/3/device/{}",
            apns_base_url(config.environment),
            target.token
        );
        let payload = serde_json::json!({
            "aps": {
                "alert": "New encrypted message",
                "sound": "default",
                "content-available": 1
            },
            "secureChatRefresh": true
        });
        let result = client
            .post(url)
            .bearer_auth(&jwt)
            .header("apns-topic", topic)
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .json(&payload)
            .send()
            .await;
        match result {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                tracing::warn!(status = %response.status(), "APNs send failed");
            }
            Err(err) => {
                tracing::warn!(%err, "APNs send failed");
            }
        }
    }
}

fn apns_jwt(config: &ApnsConfig) -> Result<String, jsonwebtoken::errors::Error> {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
    header.kid = Some(config.key_id.clone());
    jsonwebtoken::encode(
        &header,
        &ApnsJwtClaims {
            iss: config.team_id.clone(),
            iat: now_unix(),
        },
        &jsonwebtoken::EncodingKey::from_ec_pem(&config.private_key_pem)?,
    )
}

fn apns_topic(config: &ApnsConfig, platform: ApnsPlatform) -> Option<&str> {
    match platform {
        ApnsPlatform::Ios => config.topic_ios.as_deref(),
        ApnsPlatform::Macos => config.topic_macos.as_deref(),
    }
}

fn apns_base_url(environment: ApnsEnvironment) -> &'static str {
    match environment {
        ApnsEnvironment::Sandbox => "https://api.sandbox.push.apple.com",
        ApnsEnvironment::Production => "https://api.push.apple.com",
    }
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
        enforce_device_limits(&inner, account_id, device_id)?;
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

async fn register_apns_token(
    State(state): State<AppState>,
    Json(request): Json<RegisterApnsTokenRequest>,
) -> Result<Json<RegisterApnsTokenResponse>, StatusCode> {
    register_apns_token_inner(&state, request).await.map(Json)
}

async fn register_apns_token_inner(
    state: &AppState,
    request: RegisterApnsTokenRequest,
) -> Result<RegisterApnsTokenResponse, StatusCode> {
    let token = request.token.trim();
    if token.is_empty() || token.len() > MAX_APNS_TOKEN_BYTES {
        return Err(StatusCode::BAD_REQUEST);
    }
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
        "register_apns_token",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let record = ApnsTokenRecord {
        token: token.to_string(),
        platform: request.platform,
        updated_unix: now_unix(),
    };
    let records = inner.apns_tokens.entry(request.device_id).or_default();
    records.retain(|existing| existing.token != record.token);
    records.push(record.clone());
    persist_apns_token(state.db_path(), request.device_id, &record).map_err(|err| {
        tracing::error!(%err, "persist APNs token failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(RegisterApnsTokenResponse { registered: true })
}

async fn delete_apns_token(
    State(state): State<AppState>,
    Json(request): Json<DeleteApnsTokenRequest>,
) -> Result<Json<RegisterApnsTokenResponse>, StatusCode> {
    delete_apns_token_inner(&state, request).await.map(Json)
}

async fn delete_apns_token_inner(
    state: &AppState,
    request: DeleteApnsTokenRequest,
) -> Result<RegisterApnsTokenResponse, StatusCode> {
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
        "delete_apns_token",
        &signed_request,
        request.auth.as_ref(),
    )?;
    if let Some(token) = request.token.as_deref() {
        if let Some(records) = inner.apns_tokens.get_mut(&request.device_id) {
            records.retain(|record| record.token != token);
        }
        delete_apns_token_from_db(state.db_path(), request.device_id, Some(token)).map_err(
            |err| {
                tracing::error!(%err, "delete APNs token failed");
                StatusCode::INTERNAL_SERVER_ERROR
            },
        )?;
    } else {
        inner.apns_tokens.remove(&request.device_id);
        delete_apns_token_from_db(state.db_path(), request.device_id, None).map_err(|err| {
            tracing::error!(%err, "delete APNs tokens failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    Ok(RegisterApnsTokenResponse { registered: false })
}

async fn publish_mls_key_package(
    State(state): State<AppState>,
    Json(request): Json<PublishMlsKeyPackageRequest>,
) -> Result<Json<MlsKeyPackageResponse>, StatusCode> {
    publish_mls_key_package_inner(&state, request)
        .await
        .map(Json)
}

async fn publish_mls_key_package_inner(
    state: &AppState,
    request: PublishMlsKeyPackageRequest,
) -> Result<MlsKeyPackageResponse, StatusCode> {
    if request.key_package.is_empty() || request.key_package.len() > MAX_MLS_KEY_PACKAGE_BYTES {
        return Err(StatusCode::BAD_REQUEST);
    }
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
        "publish_mls_key_package",
        &signed_request,
        request.auth.as_ref(),
    )?;
    persist_mls_key_package(state.db_path(), request.device_id, &request.key_package).map_err(
        |err| {
            tracing::error!(%err, "persist MLS KeyPackage failed");
            StatusCode::INTERNAL_SERVER_ERROR
        },
    )?;
    inner
        .mls_key_packages
        .insert(request.device_id, request.key_package.clone());
    Ok(MlsKeyPackageResponse {
        account_id: request.account_id,
        device_id: request.device_id,
        key_package: Some(request.key_package),
    })
}

async fn claim_mls_key_package(
    State(state): State<AppState>,
    Json(request): Json<ClaimMlsKeyPackageRequest>,
) -> Result<Json<MlsKeyPackageResponse>, StatusCode> {
    claim_mls_key_package_inner(&state, request).await.map(Json)
}

async fn claim_mls_key_package_inner(
    state: &AppState,
    request: ClaimMlsKeyPackageRequest,
) -> Result<MlsKeyPackageResponse, StatusCode> {
    let mut inner = state.inner.write().await;
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let device_public = device_signing_public(
        &inner,
        request.requester_account_id,
        request.requester_device_id,
    )
    .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        request.requester_account_id,
        request.requester_device_id,
        &device_public,
        "claim_mls_key_package",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let exists = inner
        .devices
        .get(&request.target_account_id)
        .and_then(|devices| devices.get(&request.target_device_id))
        .is_some();
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    let key_package = inner.mls_key_packages.remove(&request.target_device_id);
    if key_package.is_some() {
        delete_mls_key_package(state.db_path(), request.target_device_id).map_err(|err| {
            tracing::error!(%err, "delete claimed MLS KeyPackage failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    Ok(MlsKeyPackageResponse {
        account_id: request.target_account_id,
        device_id: request.target_device_id,
        key_package,
    })
}

async fn publish_p2p_candidates(
    State(state): State<AppState>,
    Json(request): Json<secure_chat_core::PublishP2pCandidatesRequest>,
) -> Result<Json<P2pCandidatesResponse>, StatusCode> {
    publish_p2p_candidates_inner(&state, request)
        .await
        .map(Json)
}

async fn publish_p2p_candidates_inner(
    state: &AppState,
    request: secure_chat_core::PublishP2pCandidatesRequest,
) -> Result<P2pCandidatesResponse, StatusCode> {
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let mut inner = state.inner.write().await;
    let device_public = device_signing_public(&inner, request.account_id, request.device_id)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        request.account_id,
        request.device_id,
        &device_public,
        "publish_p2p_candidates",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let mut candidates = normalize_p2p_candidates(request.candidates)?;
    retain_live_observed_candidates(&inner, request.device_id, &mut candidates);
    set_p2p_candidates(state.db_path(), &mut inner, request.device_id, candidates)
}

async fn list_p2p_candidates(
    State(state): State<AppState>,
    Json(request): Json<ListP2pCandidatesRequest>,
) -> Result<Json<P2pCandidatesResponse>, StatusCode> {
    list_p2p_candidates_inner(&state, request).await.map(Json)
}

async fn list_p2p_candidates_inner(
    state: &AppState,
    request: ListP2pCandidatesRequest,
) -> Result<P2pCandidatesResponse, StatusCode> {
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let mut inner = state.inner.write().await;
    let requester_public = device_signing_public(
        &inner,
        request.requester_account_id,
        request.requester_device_id,
    )
    .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        request.requester_account_id,
        request.requester_device_id,
        &requester_public,
        "list_p2p_candidates",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let target_exists = inner
        .devices
        .get(&request.target_account_id)
        .and_then(|devices| devices.get(&request.target_device_id))
        .is_some();
    if !target_exists {
        return Err(StatusCode::NOT_FOUND);
    }
    if !can_access_p2p_candidates(
        &inner,
        request.requester_device_id,
        request.target_device_id,
    ) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(P2pCandidatesResponse {
        candidates: live_p2p_candidates(&mut inner, request.target_device_id),
    })
}

async fn handle_p2p_probe(
    state: &AppState,
    request: P2pProbeRequest,
    peer_addr: SocketAddr,
) -> Result<P2pProbeResponse, StatusCode> {
    let mut signed_request = request.clone();
    signed_request.auth = None;
    let mut inner = state.inner.write().await;
    let device_public = device_signing_public(&inner, request.account_id, request.device_id)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    verify_relay_auth(
        &mut inner,
        request.account_id,
        request.device_id,
        &device_public,
        "p2p_probe",
        &signed_request,
        request.auth.as_ref(),
    )?;
    let now = now_unix();
    let candidate = P2pCandidate {
        kind: P2pCandidateKind::ServerReflexive,
        addr: peer_addr.to_string(),
        updated_unix: now,
        expires_unix: now + P2P_CANDIDATE_TTL_SECS,
    };
    upsert_p2p_candidate(
        state.db_path(),
        &mut inner,
        request.device_id,
        candidate.clone(),
    )?;
    Ok(P2pProbeResponse { candidate })
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
    if request.ciphertext.len() > MAX_MESSAGE_CIPHERTEXT_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    if request
        .sealed_sender
        .as_ref()
        .is_some_and(|sealed| sealed.len() > MAX_SEALED_SENDER_BYTES)
    {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let exists = inner
        .devices
        .get(&request.to_account_id)
        .and_then(|devices| devices.get(&request.to_device_id))
        .is_some();
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    let queue = inner.queues.entry(request.to_device_id).or_default();
    if queue.len() >= MAX_QUEUE_MESSAGES_PER_DEVICE {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let now = now_unix();
    let max_expires_unix = now + MAX_MESSAGE_TTL_SECS;
    let expires_unix = request
        .expires_unix
        .map(|expires| expires.min(max_expires_unix))
        .or(Some(max_expires_unix));
    let message = QueuedMessage {
        id: Uuid::new_v4(),
        sender_account_id: Some(sender_account_id),
        sender_device_id: Some(sender_device_id),
        transport_kind: request.transport_kind,
        sealed_sender: request.sealed_sender,
        ciphertext: request.ciphertext,
        received_unix: now,
        expires_unix,
    };
    let grant = ReceiptGrant {
        message_id: message.id,
        sender_account_id,
        sender_device_id,
        recipient_account_id: request.to_account_id,
        recipient_device_id: request.to_device_id,
        expires_unix: expires_unix.unwrap_or(max_expires_unix),
    };
    persist_message(state.db_path(), request.to_device_id, &message).map_err(|err| {
        tracing::error!(%err, "persist message failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    persist_receipt_grant(state.db_path(), &grant).map_err(|err| {
        tracing::error!(%err, "persist receipt grant failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    persist_peer_link(state.db_path(), sender_device_id, request.to_device_id).map_err(|err| {
        tracing::error!(%err, "persist peer link failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    queue.push_back(message.clone());
    inner.receipt_grants.insert(message.id, grant);
    inner
        .peer_links
        .insert(DevicePair::new(sender_device_id, request.to_device_id));
    let push_targets = inner
        .apns_tokens
        .get(&request.to_device_id)
        .cloned()
        .unwrap_or_default();
    drop(inner);
    send_apns_notifications(push_targets).await;
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
    let mut messages = Vec::new();
    let mut delivery_receipts = Vec::new();
    let mut drained_message_ids = Vec::new();
    let mut expired_message_ids = Vec::new();
    {
        let queue = inner.queues.entry(request.device_id).or_default();
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
    }
    for (sender_device_id, receipt) in delivery_receipts {
        if receipt_queue_is_full(&inner, sender_device_id) {
            tracing::warn!(%sender_device_id, "dropping delivery receipt because receipt queue is full");
            continue;
        }
        if let Err(err) = persist_receipt(state.db_path(), sender_device_id, &receipt) {
            tracing::error!(%err, "persist delivery receipt failed");
            continue;
        }
        inner
            .receipts
            .entry(sender_device_id)
            .or_default()
            .push_back(receipt);
    }
    for message_id in drained_message_ids {
        if let Err(err) = delete_message(state.db_path(), message_id) {
            tracing::error!(%err, "delete drained message failed");
        }
    }
    for message_id in expired_message_ids {
        if let Err(err) = delete_message(state.db_path(), message_id) {
            tracing::error!(%err, "delete expired message failed");
        }
        inner.receipt_grants.remove(&message_id);
        if let Err(err) = delete_receipt_grant(state.db_path(), message_id) {
            tracing::error!(%err, "delete expired receipt grant failed");
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
    validate_receipt_grant(&mut inner, &request)?;
    if receipt_queue_is_full(&inner, request.to_device_id) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
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
    while nonces.len() > MAX_AUTH_NONCES_PER_DEVICE {
        nonces.pop_front();
    }
    Ok(())
}

fn enforce_device_limits(
    inner: &RelayState,
    account_id: AccountId,
    device_id: DeviceId,
) -> Result<(), StatusCode> {
    if inner
        .devices
        .get(&account_id)
        .and_then(|devices| devices.get(&device_id))
        .is_some()
    {
        return Ok(());
    }
    if inner.devices.iter().any(|(existing_account_id, devices)| {
        *existing_account_id != account_id && devices.contains_key(&device_id)
    }) {
        return Err(StatusCode::CONFLICT);
    }
    let total_devices: usize = inner.devices.values().map(HashMap::len).sum();
    if total_devices >= MAX_TOTAL_DEVICES {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let account_devices = inner.devices.get(&account_id).map_or(0, HashMap::len);
    if account_devices >= MAX_DEVICES_PER_ACCOUNT {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(())
}

fn receipt_queue_is_full(inner: &RelayState, device_id: DeviceId) -> bool {
    inner
        .receipts
        .get(&device_id)
        .is_some_and(|queue| queue.len() >= MAX_RECEIPTS_PER_DEVICE)
}

fn can_access_p2p_candidates(
    inner: &RelayState,
    requester_device_id: DeviceId,
    target_device_id: DeviceId,
) -> bool {
    requester_device_id == target_device_id
        || inner
            .peer_links
            .contains(&DevicePair::new(requester_device_id, target_device_id))
}

fn validate_receipt_grant(
    inner: &mut RelayState,
    request: &ReceiptRequest,
) -> Result<(), StatusCode> {
    let now = now_unix();
    inner
        .receipt_grants
        .retain(|_, grant| grant.expires_unix >= now);
    let grant = inner
        .receipt_grants
        .get(&request.message_id)
        .ok_or(StatusCode::FORBIDDEN)?;
    if grant.sender_account_id != request.to_account_id
        || grant.sender_device_id != request.to_device_id
        || grant.recipient_account_id != request.from_account_id
        || grant.recipient_device_id != request.from_device_id
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn normalize_p2p_candidates(
    drafts: Vec<P2pCandidateDraft>,
) -> Result<Vec<P2pCandidate>, StatusCode> {
    if drafts.len() > 8 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let now = now_unix();
    let mut candidates = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let addr: SocketAddr = draft.addr.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
        if addr.ip().is_unspecified() || addr.port() == 0 {
            return Err(StatusCode::BAD_REQUEST);
        }
        candidates.push(P2pCandidate {
            kind: draft.kind,
            addr: addr.to_string(),
            updated_unix: now,
            expires_unix: now + P2P_CANDIDATE_TTL_SECS,
        });
    }
    Ok(candidates)
}

fn set_p2p_candidates(
    db_path: Option<&Path>,
    inner: &mut RelayState,
    device_id: DeviceId,
    candidates: Vec<P2pCandidate>,
) -> Result<P2pCandidatesResponse, StatusCode> {
    persist_p2p_candidates(db_path, device_id, &candidates).map_err(|err| {
        tracing::error!(%err, "persist p2p candidates failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    inner.p2p_candidates.insert(device_id, candidates.clone());
    Ok(P2pCandidatesResponse { candidates })
}

fn upsert_p2p_candidate(
    db_path: Option<&Path>,
    inner: &mut RelayState,
    device_id: DeviceId,
    candidate: P2pCandidate,
) -> Result<(), StatusCode> {
    let candidates = inner.p2p_candidates.entry(device_id).or_default();
    candidates.retain(|existing| {
        existing.expires_unix >= candidate.updated_unix
            && !(existing.kind == candidate.kind && existing.addr == candidate.addr)
    });
    candidates.push(candidate);
    prune_p2p_candidate_list(candidates);
    persist_p2p_candidates(db_path, device_id, candidates).map_err(|err| {
        tracing::error!(%err, "persist p2p candidate failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

fn retain_live_observed_candidates(
    inner: &RelayState,
    device_id: DeviceId,
    candidates: &mut Vec<P2pCandidate>,
) {
    let now = now_unix();
    if let Some(existing) = inner.p2p_candidates.get(&device_id) {
        for candidate in existing {
            if candidate.kind != P2pCandidateKind::ServerReflexive || candidate.expires_unix < now {
                continue;
            }
            if candidates
                .iter()
                .any(|item| item.kind == candidate.kind && item.addr == candidate.addr)
            {
                continue;
            }
            candidates.push(candidate.clone());
        }
    }
    prune_p2p_candidate_list(candidates);
}

fn prune_p2p_candidate_list(candidates: &mut Vec<P2pCandidate>) {
    let now = now_unix();
    candidates.retain(|candidate| candidate.expires_unix >= now);
    candidates.sort_by(|left, right| right.updated_unix.cmp(&left.updated_unix));
    candidates.truncate(8);
}

fn live_p2p_candidates(inner: &mut RelayState, device_id: DeviceId) -> Vec<P2pCandidate> {
    let now = now_unix();
    let candidates = inner.p2p_candidates.entry(device_id).or_default();
    candidates.retain(|candidate| candidate.expires_unix >= now);
    candidates.clone()
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
        CREATE TABLE IF NOT EXISTS p2p_candidates (
            device_id TEXT NOT NULL,
            addr TEXT NOT NULL,
            candidate_json TEXT NOT NULL,
            expires_unix INTEGER NOT NULL,
            PRIMARY KEY(device_id, addr)
        );
        CREATE INDEX IF NOT EXISTS idx_p2p_candidates_device ON p2p_candidates(device_id, expires_unix);
        CREATE TABLE IF NOT EXISTS mls_key_packages (
            device_id TEXT NOT NULL PRIMARY KEY,
            key_package BLOB NOT NULL,
            updated_unix INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS apns_tokens (
            device_id TEXT NOT NULL,
            token TEXT NOT NULL,
            token_json TEXT NOT NULL,
            updated_unix INTEGER NOT NULL,
            PRIMARY KEY(device_id, token)
        );
        CREATE INDEX IF NOT EXISTS idx_apns_tokens_device ON apns_tokens(device_id, updated_unix);
        CREATE TABLE IF NOT EXISTS peer_links (
            left_device_id TEXT NOT NULL,
            right_device_id TEXT NOT NULL,
            created_unix INTEGER NOT NULL,
            PRIMARY KEY(left_device_id, right_device_id)
        );
        CREATE TABLE IF NOT EXISTS receipt_grants (
            message_id TEXT NOT NULL PRIMARY KEY,
            grant_json TEXT NOT NULL,
            expires_unix INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_receipt_grants_expires ON receipt_grants(expires_unix);
        "#,
    )?;
    conn.execute(
        "DELETE FROM messages WHERE expires_unix IS NOT NULL AND expires_unix < ?1",
        params![now_unix() as i64],
    )?;
    conn.execute(
        "DELETE FROM p2p_candidates WHERE expires_unix < ?1",
        params![now_unix() as i64],
    )?;
    conn.execute(
        "DELETE FROM receipt_grants WHERE expires_unix < ?1",
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
        if let Err(err) = bundle.verify() {
            tracing::warn!(%err, "skipping invalid persisted device bundle");
            continue;
        }
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

    let now = now_unix() as i64;
    let mut p2p_candidates = conn
        .prepare("SELECT device_id, candidate_json FROM p2p_candidates WHERE expires_unix >= ?1")?;
    let rows = p2p_candidates.query_map(params![now], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (device_id, json) = row?;
        let device_id = parse_uuid(&device_id)?;
        let candidate: P2pCandidate = parse_json(&json)?;
        state
            .p2p_candidates
            .entry(device_id)
            .or_default()
            .push(candidate);
    }

    let mut mls_key_packages =
        conn.prepare("SELECT device_id, key_package FROM mls_key_packages")?;
    let rows = mls_key_packages.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    for row in rows {
        let (device_id, key_package) = row?;
        state
            .mls_key_packages
            .insert(parse_uuid(&device_id)?, key_package);
    }

    let mut apns_tokens =
        conn.prepare("SELECT device_id, token_json FROM apns_tokens ORDER BY updated_unix ASC")?;
    let rows = apns_tokens.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (device_id, json) = row?;
        let device_id = parse_uuid(&device_id)?;
        let record: ApnsTokenRecord = parse_json(&json)?;
        state.apns_tokens.entry(device_id).or_default().push(record);
    }

    let mut peer_links = conn.prepare("SELECT left_device_id, right_device_id FROM peer_links")?;
    let rows = peer_links.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (left, right) = row?;
        state
            .peer_links
            .insert(DevicePair::new(parse_uuid(&left)?, parse_uuid(&right)?));
    }

    let mut receipt_grants =
        conn.prepare("SELECT message_id, grant_json FROM receipt_grants WHERE expires_unix >= ?1")?;
    let rows = receipt_grants.query_map(params![now], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (message_id, json) = row?;
        state
            .receipt_grants
            .insert(parse_uuid(&message_id)?, parse_json(&json)?);
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

fn persist_p2p_candidates(
    db_path: Option<&Path>,
    device_id: DeviceId,
    candidates: &[P2pCandidate],
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let mut conn = Connection::open(path)?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM p2p_candidates WHERE device_id = ?1",
        params![device_id.to_string()],
    )?;
    for candidate in candidates {
        tx.execute(
            "INSERT INTO p2p_candidates(device_id, addr, candidate_json, expires_unix)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                device_id.to_string(),
                &candidate.addr,
                to_json(candidate)?,
                candidate.expires_unix as i64
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn persist_apns_token(
    db_path: Option<&Path>,
    device_id: DeviceId,
    record: &ApnsTokenRecord,
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO apns_tokens(device_id, token, token_json, updated_unix)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(device_id, token) DO UPDATE SET
            token_json = excluded.token_json,
            updated_unix = excluded.updated_unix",
        params![
            device_id.to_string(),
            record.token,
            to_json(record)?,
            record.updated_unix as i64
        ],
    )?;
    Ok(())
}

fn persist_mls_key_package(
    db_path: Option<&Path>,
    device_id: DeviceId,
    key_package: &[u8],
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO mls_key_packages(device_id, key_package, updated_unix)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(device_id) DO UPDATE SET
            key_package = excluded.key_package,
            updated_unix = excluded.updated_unix",
        params![device_id.to_string(), key_package, now_unix() as i64],
    )?;
    Ok(())
}

fn delete_mls_key_package(db_path: Option<&Path>, device_id: DeviceId) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "DELETE FROM mls_key_packages WHERE device_id = ?1",
        params![device_id.to_string()],
    )?;
    Ok(())
}

fn delete_apns_token_from_db(
    db_path: Option<&Path>,
    device_id: DeviceId,
    token: Option<&str>,
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    if let Some(token) = token {
        conn.execute(
            "DELETE FROM apns_tokens WHERE device_id = ?1 AND token = ?2",
            params![device_id.to_string(), token],
        )?;
    } else {
        conn.execute(
            "DELETE FROM apns_tokens WHERE device_id = ?1",
            params![device_id.to_string()],
        )?;
    }
    Ok(())
}

fn persist_peer_link(
    db_path: Option<&Path>,
    left_device_id: DeviceId,
    right_device_id: DeviceId,
) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let pair = DevicePair::new(left_device_id, right_device_id);
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT OR IGNORE INTO peer_links(left_device_id, right_device_id, created_unix)
         VALUES (?1, ?2, ?3)",
        params![
            pair.left.to_string(),
            pair.right.to_string(),
            now_unix() as i64
        ],
    )?;
    Ok(())
}

fn persist_receipt_grant(db_path: Option<&Path>, grant: &ReceiptGrant) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT OR REPLACE INTO receipt_grants(message_id, grant_json, expires_unix)
         VALUES (?1, ?2, ?3)",
        params![
            grant.message_id.to_string(),
            to_json(grant)?,
            grant.expires_unix as i64
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

fn delete_receipt_grant(db_path: Option<&Path>, message_id: Uuid) -> rusqlite::Result<()> {
    let Some(path) = db_path else {
        return Ok(());
    };
    let conn = Connection::open(path)?;
    conn.execute(
        "DELETE FROM receipt_grants WHERE message_id = ?1",
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
    let reader = BufReader::new(File::open(path)?);
    rustls::pki_types::CertificateDer::pem_reader_iter(reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
}

fn load_private_key(path: &Path) -> io::Result<rustls::pki_types::PrivateKeyDer<'static>> {
    rustls::pki_types::PrivateKeyDer::from_pem_file(path)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
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

    #[tokio::test]
    async fn p2p_candidates_require_signed_registered_device() {
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

        let unsigned = secure_chat_core::PublishP2pCandidatesRequest {
            account_id: keys.account_id,
            device_id: keys.device_id,
            candidates: vec![secure_chat_core::P2pCandidateDraft {
                kind: secure_chat_core::P2pCandidateKind::Host,
                addr: "127.0.0.1:40000".to_string(),
            }],
            auth: None,
        };
        assert_eq!(
            publish_p2p_candidates_inner(&state, unsigned)
                .await
                .unwrap_err(),
            StatusCode::UNAUTHORIZED
        );

        let mut signed = secure_chat_core::PublishP2pCandidatesRequest {
            account_id: keys.account_id,
            device_id: keys.device_id,
            candidates: vec![secure_chat_core::P2pCandidateDraft {
                kind: secure_chat_core::P2pCandidateKind::Host,
                addr: "127.0.0.1:40000".to_string(),
            }],
            auth: None,
        };
        signed.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                &keys,
                "publish_p2p_candidates",
                &signed,
                now_unix(),
            )
            .unwrap(),
        );
        publish_p2p_candidates_inner(&state, signed).await.unwrap();

        let mut list = secure_chat_core::ListP2pCandidatesRequest {
            requester_account_id: keys.account_id,
            requester_device_id: keys.device_id,
            target_account_id: keys.account_id,
            target_device_id: keys.device_id,
            auth: None,
        };
        list.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                &keys,
                "list_p2p_candidates",
                &list,
                now_unix(),
            )
            .unwrap(),
        );
        let response = list_p2p_candidates_inner(&state, list).await.unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(response.candidates[0].addr, "127.0.0.1:40000");
    }

    #[tokio::test]
    async fn p2p_candidates_require_existing_peer_link() {
        let alice = DeviceKeyMaterial::generate(1);
        let bob = DeviceKeyMaterial::generate(1);
        let state = AppState::memory();
        register_device_inner(&state, signed_register(&alice))
            .await
            .unwrap();
        register_device_inner(&state, signed_register(&bob))
            .await
            .unwrap();

        publish_p2p_candidates_inner(&state, signed_publish_candidates(&bob, "127.0.0.1:41000"))
            .await
            .unwrap();

        assert_eq!(
            list_p2p_candidates_inner(&state, signed_list_candidates(&alice, &bob))
                .await
                .unwrap_err(),
            StatusCode::FORBIDDEN
        );

        send_message_inner(&state, signed_send(&alice, &bob, b"hello".to_vec()))
            .await
            .unwrap();

        let response = list_p2p_candidates_inner(&state, signed_list_candidates(&alice, &bob))
            .await
            .unwrap();
        assert_eq!(response.candidates.len(), 1);
    }

    #[tokio::test]
    async fn receipts_require_matching_message_grant() {
        let alice = DeviceKeyMaterial::generate(1);
        let bob = DeviceKeyMaterial::generate(1);
        let state = AppState::memory();
        register_device_inner(&state, signed_register(&alice))
            .await
            .unwrap();
        register_device_inner(&state, signed_register(&bob))
            .await
            .unwrap();

        let message = send_message_inner(&state, signed_send(&alice, &bob, b"ciphertext".to_vec()))
            .await
            .unwrap();

        let spoofed = signed_receipt(&alice, message.id, &bob);
        assert_eq!(
            send_receipt_inner(&state, spoofed).await.unwrap_err(),
            StatusCode::FORBIDDEN
        );

        let valid = signed_receipt(&bob, message.id, &alice);
        send_receipt_inner(&state, valid).await.unwrap();
    }

    #[tokio::test]
    async fn relay_rejects_oversized_ciphertext() {
        let keys = DeviceKeyMaterial::generate(1);
        let state = AppState::memory();
        register_device_inner(&state, signed_register(&keys))
            .await
            .unwrap();

        let oversized = vec![0u8; MAX_MESSAGE_CIPHERTEXT_BYTES + 1];
        assert_eq!(
            send_message_inner(&state, signed_send(&keys, &keys, oversized))
                .await
                .unwrap_err(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[tokio::test]
    async fn apns_tokens_require_signed_registered_device_and_can_be_deleted() {
        let keys = DeviceKeyMaterial::generate(1);
        let state = AppState::memory();
        register_device_inner(&state, signed_register(&keys))
            .await
            .unwrap();

        let unsigned = RegisterApnsTokenRequest {
            account_id: keys.account_id,
            device_id: keys.device_id,
            token: "abcd".to_string(),
            platform: ApnsPlatform::Ios,
            auth: None,
        };
        assert_eq!(
            register_apns_token_inner(&state, unsigned)
                .await
                .unwrap_err(),
            StatusCode::UNAUTHORIZED
        );

        register_apns_token_inner(&state, signed_register_apns(&keys, "abcd"))
            .await
            .unwrap();
        {
            let inner = state.inner.read().await;
            assert_eq!(inner.apns_tokens[&keys.device_id].len(), 1);
        }
        delete_apns_token_inner(&state, signed_delete_apns(&keys, None))
            .await
            .unwrap();
        let inner = state.inner.read().await;
        assert!(!inner.apns_tokens.contains_key(&keys.device_id));
    }

    #[tokio::test]
    async fn relay_rejects_device_id_collision_across_accounts() {
        let alice = DeviceKeyMaterial::generate(1);
        let mut attacker = DeviceKeyMaterial::generate(1);
        attacker.device_id = alice.device_id;
        attacker.refresh_signatures();
        let state = AppState::memory();
        register_device_inner(&state, signed_register(&alice))
            .await
            .unwrap();

        assert_eq!(
            register_device_inner(&state, signed_register(&attacker))
                .await
                .unwrap_err(),
            StatusCode::CONFLICT
        );
    }

    fn signed_register(keys: &DeviceKeyMaterial) -> RegisterRequest {
        let mut request = RegisterRequest {
            bundle: keys.pre_key_bundle(),
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                keys,
                "register_device",
                &request,
                now_unix(),
            )
            .unwrap(),
        );
        request
    }

    fn signed_send(
        sender: &DeviceKeyMaterial,
        recipient: &DeviceKeyMaterial,
        ciphertext: Vec<u8>,
    ) -> SendRequest {
        let mut request = SendRequest {
            sender_account_id: Some(sender.account_id),
            sender_device_id: Some(sender.device_id),
            to_account_id: recipient.account_id,
            to_device_id: recipient.device_id,
            transport_kind: TransportKind::RelayHttps,
            sealed_sender: None,
            ciphertext,
            expires_unix: None,
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                sender,
                "send_message",
                &request,
                now_unix(),
            )
            .unwrap(),
        );
        request
    }

    fn signed_receipt(
        sender: &DeviceKeyMaterial,
        message_id: Uuid,
        recipient: &DeviceKeyMaterial,
    ) -> ReceiptRequest {
        let mut request = ReceiptRequest {
            message_id,
            from_account_id: sender.account_id,
            from_device_id: sender.device_id,
            to_account_id: recipient.account_id,
            to_device_id: recipient.device_id,
            kind: ReceiptKind::Read,
            at_unix: now_unix(),
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                sender,
                "send_receipt",
                &request,
                now_unix(),
            )
            .unwrap(),
        );
        request
    }

    fn signed_publish_candidates(
        keys: &DeviceKeyMaterial,
        addr: &str,
    ) -> secure_chat_core::PublishP2pCandidatesRequest {
        let mut request = secure_chat_core::PublishP2pCandidatesRequest {
            account_id: keys.account_id,
            device_id: keys.device_id,
            candidates: vec![secure_chat_core::P2pCandidateDraft {
                kind: secure_chat_core::P2pCandidateKind::Host,
                addr: addr.to_string(),
            }],
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                keys,
                "publish_p2p_candidates",
                &request,
                now_unix(),
            )
            .unwrap(),
        );
        request
    }

    fn signed_list_candidates(
        requester: &DeviceKeyMaterial,
        target: &DeviceKeyMaterial,
    ) -> secure_chat_core::ListP2pCandidatesRequest {
        let mut request = secure_chat_core::ListP2pCandidatesRequest {
            requester_account_id: requester.account_id,
            requester_device_id: requester.device_id,
            target_account_id: target.account_id,
            target_device_id: target.device_id,
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                requester,
                "list_p2p_candidates",
                &request,
                now_unix(),
            )
            .unwrap(),
        );
        request
    }

    fn signed_register_apns(keys: &DeviceKeyMaterial, token: &str) -> RegisterApnsTokenRequest {
        let mut request = RegisterApnsTokenRequest {
            account_id: keys.account_id,
            device_id: keys.device_id,
            token: token.to_string(),
            platform: ApnsPlatform::Ios,
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                keys,
                "register_apns_token",
                &request,
                now_unix(),
            )
            .unwrap(),
        );
        request
    }

    fn signed_delete_apns(
        keys: &DeviceKeyMaterial,
        token: Option<String>,
    ) -> DeleteApnsTokenRequest {
        let mut request = DeleteApnsTokenRequest {
            account_id: keys.account_id,
            device_id: keys.device_id,
            token,
            auth: None,
        };
        request.auth = Some(
            secure_chat_core::sign_relay_auth_for_request(
                keys,
                "delete_apns_token",
                &request,
                now_unix(),
            )
            .unwrap(),
        );
        request
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
