use rustls_pki_types::{pem::PemObject, CertificateDer};
use secure_chat_core::{
    accept_session_as_responder_consuming_prekey, start_session_as_initiator, AccountId,
    ApnsPlatform, CipherSuite, ClaimMlsKeyPackageRequest, CryptoError, DeleteApnsTokenRequest,
    DeviceId, DeviceKeyMaterial, DevicePreKeyBundle, DrainReceiptsResponse, DrainRequest,
    DrainResponse, GroupPlainMessage, GroupState, GroupTransportEnvelope, InitialMessage, Invite,
    ListP2pCandidatesRequest, MlsKeyPackageResponse, ObfuscationProfile, P2pCandidate,
    P2pCandidateDraft, P2pCandidateKind, P2pCandidatesResponse, P2pDirectDatagram,
    P2pDirectReplayCache, P2pProbeRequest, P2pProbeResponse, PlainMessage,
    PublishMlsKeyPackageRequest, QueuedMessage, QueuedReceipt, RatchetSession, ReceiptRequest,
    RegisterApnsTokenRequest, RegisterApnsTokenResponse, RegisterRequest, RegisterResponse,
    RelayCommand, RelayCommandResponse, SendRequest, TransportFrame, TransportKind, WireMessage,
    GROUP_TRANSPORT_KIND, P2P_RENDEZVOUS_DEFAULT_PORT, RELAY_QUIC_ALPN,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};

const RELAY_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const RELAY_QUIC_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("protocol error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("missing session for device {0}")]
    MissingSession(DeviceId),
    #[error("transport error: {0}")]
    Transport(String),
}

#[derive(Clone)]
pub struct RelayClient {
    transport: RelayTransport,
}

#[derive(Clone)]
enum RelayTransport {
    Http(RelayHttpClient),
    Quic(QuicRelayClient),
}

impl RelayClient {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        if url.starts_with("quic://") {
            Self {
                transport: RelayTransport::Quic(QuicRelayClient::new(url)),
            }
        } else {
            Self {
                transport: RelayTransport::Http(RelayHttpClient::new(url)),
            }
        }
    }

    pub async fn health(&self) -> Result<serde_json::Value, ClientError> {
        match &self.transport {
            RelayTransport::Http(client) => client.health().await,
            RelayTransport::Quic(client) => match client.command(RelayCommand::Health).await? {
                RelayCommandResponse::Health(value) => Ok(value),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn register_device(
        &self,
        keys: &DeviceKeyMaterial,
    ) -> Result<RegisterResponse, ClientError> {
        let request = signed_register_request(keys)?;
        match &self.transport {
            RelayTransport::Http(client) => client.register_device(request).await,
            RelayTransport::Quic(client) => match client
                .command(RelayCommand::RegisterDevice(request))
                .await?
            {
                RelayCommandResponse::RegisterDevice(response) => Ok(response),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn list_devices(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<DevicePreKeyBundle>, ClientError> {
        match &self.transport {
            RelayTransport::Http(client) => client.list_devices(account_id).await,
            RelayTransport::Quic(client) => {
                match client
                    .command(RelayCommand::ListDevices { account_id })
                    .await?
                {
                    RelayCommandResponse::ListDevices(devices) => Ok(devices),
                    response => Err(unexpected_response(response)),
                }
            }
        }
    }

    pub async fn publish_p2p_candidates(
        &self,
        keys: &DeviceKeyMaterial,
        candidates: Vec<P2pCandidateDraft>,
    ) -> Result<Vec<P2pCandidate>, ClientError> {
        let request = signed_publish_p2p_candidates_request(keys, candidates)?;
        match &self.transport {
            RelayTransport::Http(client) => client.publish_p2p_candidates(request).await,
            RelayTransport::Quic(client) => match client
                .command(RelayCommand::PublishP2pCandidates(request))
                .await?
            {
                RelayCommandResponse::PublishP2pCandidates(response) => Ok(response.candidates),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn list_p2p_candidates(
        &self,
        keys: &DeviceKeyMaterial,
        target_account_id: AccountId,
        target_device_id: DeviceId,
    ) -> Result<Vec<P2pCandidate>, ClientError> {
        let request =
            signed_list_p2p_candidates_request(keys, target_account_id, target_device_id)?;
        match &self.transport {
            RelayTransport::Http(client) => client.list_p2p_candidates(request).await,
            RelayTransport::Quic(client) => match client
                .command(RelayCommand::ListP2pCandidates(request))
                .await?
            {
                RelayCommandResponse::ListP2pCandidates(response) => Ok(response.candidates),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn register_apns_token(
        &self,
        keys: &DeviceKeyMaterial,
        token: impl Into<String>,
        platform: ApnsPlatform,
    ) -> Result<RegisterApnsTokenResponse, ClientError> {
        let request = signed_register_apns_token_request(keys, token.into(), platform)?;
        match &self.transport {
            RelayTransport::Http(client) => client.register_apns_token(request).await,
            RelayTransport::Quic(client) => match client
                .command(RelayCommand::RegisterApnsToken(request))
                .await?
            {
                RelayCommandResponse::RegisterApnsToken(response) => Ok(response),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn delete_apns_token(
        &self,
        keys: &DeviceKeyMaterial,
        token: Option<String>,
    ) -> Result<RegisterApnsTokenResponse, ClientError> {
        let request = signed_delete_apns_token_request(keys, token)?;
        match &self.transport {
            RelayTransport::Http(client) => client.delete_apns_token(request).await,
            RelayTransport::Quic(client) => match client
                .command(RelayCommand::DeleteApnsToken(request))
                .await?
            {
                RelayCommandResponse::DeleteApnsToken(response) => Ok(response),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn publish_mls_key_package(
        &self,
        keys: &DeviceKeyMaterial,
        key_package: Vec<u8>,
    ) -> Result<MlsKeyPackageResponse, ClientError> {
        let request = signed_publish_mls_key_package_request(keys, key_package)?;
        match &self.transport {
            RelayTransport::Http(client) => client.publish_mls_key_package(request).await,
            RelayTransport::Quic(client) => match client
                .command(RelayCommand::PublishMlsKeyPackage(request))
                .await?
            {
                RelayCommandResponse::PublishMlsKeyPackage(response) => Ok(response),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn claim_mls_key_package(
        &self,
        keys: &DeviceKeyMaterial,
        target_account_id: AccountId,
        target_device_id: DeviceId,
    ) -> Result<MlsKeyPackageResponse, ClientError> {
        let request =
            signed_claim_mls_key_package_request(keys, target_account_id, target_device_id)?;
        match &self.transport {
            RelayTransport::Http(client) => client.claim_mls_key_package(request).await,
            RelayTransport::Quic(client) => match client
                .command(RelayCommand::ClaimMlsKeyPackage(request))
                .await?
            {
                RelayCommandResponse::ClaimMlsKeyPackage(response) => Ok(response),
                response => Err(unexpected_response(response)),
            },
        }
    }

    pub async fn send(
        &self,
        keys: &DeviceKeyMaterial,
        request: SendRequest,
    ) -> Result<QueuedMessage, ClientError> {
        let request = signed_send_request(keys, request)?;
        match &self.transport {
            RelayTransport::Http(client) => client.send(request).await,
            RelayTransport::Quic(client) => {
                match client.command(RelayCommand::SendMessage(request)).await? {
                    RelayCommandResponse::SendMessage(message) => Ok(message),
                    response => Err(unexpected_response(response)),
                }
            }
        }
    }

    pub async fn send_receipt(
        &self,
        keys: &DeviceKeyMaterial,
        request: ReceiptRequest,
    ) -> Result<QueuedReceipt, ClientError> {
        let request = signed_receipt_request(keys, request)?;
        match &self.transport {
            RelayTransport::Http(client) => client.send_receipt(request).await,
            RelayTransport::Quic(client) => {
                match client.command(RelayCommand::SendReceipt(request)).await? {
                    RelayCommandResponse::SendReceipt(receipt) => Ok(receipt),
                    response => Err(unexpected_response(response)),
                }
            }
        }
    }

    pub async fn drain(&self, keys: &DeviceKeyMaterial) -> Result<Vec<QueuedMessage>, ClientError> {
        let request = signed_drain_request(keys, "drain_messages")?;
        match &self.transport {
            RelayTransport::Http(client) => client.drain(request).await,
            RelayTransport::Quic(client) => {
                match client.command(RelayCommand::DrainMessages(request)).await? {
                    RelayCommandResponse::DrainMessages(response) => Ok(response.messages),
                    response => Err(unexpected_response(response)),
                }
            }
        }
    }

    pub async fn drain_receipts(
        &self,
        keys: &DeviceKeyMaterial,
    ) -> Result<Vec<QueuedReceipt>, ClientError> {
        let request = signed_drain_request(keys, "drain_receipts")?;
        match &self.transport {
            RelayTransport::Http(client) => client.drain_receipts(request).await,
            RelayTransport::Quic(client) => {
                match client.command(RelayCommand::DrainReceipts(request)).await? {
                    RelayCommandResponse::DrainReceipts(response) => Ok(response.receipts),
                    response => Err(unexpected_response(response)),
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct RelayHttpClient {
    base_url: String,
    http: reqwest::Client,
    insecure_error: Option<String>,
}

impl RelayHttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let insecure_error = insecure_http_error(&base_url);
        Self {
            base_url,
            http: reqwest::Client::builder()
                .connect_timeout(RELAY_HTTP_CONNECT_TIMEOUT)
                .timeout(RELAY_HTTP_REQUEST_TIMEOUT)
                .pool_idle_timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            insecure_error,
        }
    }

    pub async fn health(&self) -> Result<serde_json::Value, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .get(self.url("/health"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn register_device(
        &self,
        request: RegisterRequest,
    ) -> Result<RegisterResponse, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .post(self.url("/v1/accounts"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn list_devices(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<DevicePreKeyBundle>, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .get(self.url(&format!("/v1/accounts/{account_id}/devices")))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn send(&self, request: SendRequest) -> Result<QueuedMessage, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .post(self.url("/v1/messages"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn publish_p2p_candidates(
        &self,
        request: secure_chat_core::PublishP2pCandidatesRequest,
    ) -> Result<Vec<P2pCandidate>, ClientError> {
        self.ensure_allowed()?;
        let response: P2pCandidatesResponse = self
            .http
            .post(self.url("/v1/p2p/candidates"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.candidates)
    }

    pub async fn list_p2p_candidates(
        &self,
        request: ListP2pCandidatesRequest,
    ) -> Result<Vec<P2pCandidate>, ClientError> {
        self.ensure_allowed()?;
        let response: P2pCandidatesResponse = self
            .http
            .post(self.url("/v1/p2p/candidates/list"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.candidates)
    }

    pub async fn register_apns_token(
        &self,
        request: RegisterApnsTokenRequest,
    ) -> Result<RegisterApnsTokenResponse, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .post(self.url("/v1/push/apns/token"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn delete_apns_token(
        &self,
        request: DeleteApnsTokenRequest,
    ) -> Result<RegisterApnsTokenResponse, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .post(self.url("/v1/push/apns/token/delete"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn publish_mls_key_package(
        &self,
        request: PublishMlsKeyPackageRequest,
    ) -> Result<MlsKeyPackageResponse, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .post(self.url("/v1/mls/key-packages"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn claim_mls_key_package(
        &self,
        request: ClaimMlsKeyPackageRequest,
    ) -> Result<MlsKeyPackageResponse, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .post(self.url("/v1/mls/key-packages/claim"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn send_receipt(
        &self,
        request: ReceiptRequest,
    ) -> Result<QueuedReceipt, ClientError> {
        self.ensure_allowed()?;
        Ok(self
            .http
            .post(self.url("/v1/receipts"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn drain(&self, request: DrainRequest) -> Result<Vec<QueuedMessage>, ClientError> {
        self.ensure_allowed()?;
        let response: DrainResponse = self
            .http
            .post(self.url("/v1/messages/drain"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.messages)
    }

    pub async fn drain_receipts(
        &self,
        request: DrainRequest,
    ) -> Result<Vec<QueuedReceipt>, ClientError> {
        self.ensure_allowed()?;
        let response: DrainReceiptsResponse = self
            .http
            .post(self.url("/v1/receipts/drain"))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.receipts)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn ensure_allowed(&self) -> Result<(), ClientError> {
        if let Some(error) = &self.insecure_error {
            return Err(ClientError::Transport(error.clone()));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct QuicRelayClient {
    relay_url: String,
}

impl QuicRelayClient {
    pub fn new(relay_url: impl Into<String>) -> Self {
        Self {
            relay_url: relay_url.into(),
        }
    }

    pub async fn command(
        &self,
        command: RelayCommand,
    ) -> Result<RelayCommandResponse, ClientError> {
        let (server_name, addr) = quic_target(&self.relay_url)?;
        let local_addr = if addr.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let mut endpoint = quinn::Endpoint::client(local_addr.parse().map_err(|err| {
            ClientError::Transport(format!("invalid local QUIC endpoint: {err}"))
        })?)
        .map_err(|err| ClientError::Transport(err.to_string()))?;
        endpoint.set_default_client_config(quic_client_config()?);
        let connecting = endpoint
            .connect(addr, &server_name)
            .map_err(|err| ClientError::Transport(err.to_string()))?;
        let connection = timeout(RELAY_QUIC_COMMAND_TIMEOUT, connecting)
            .await
            .map_err(|_| ClientError::Transport("relay QUIC connect timed out".to_string()))?
            .map_err(|err| ClientError::Transport(err.to_string()))?;
        let (mut send, mut recv) = timeout(RELAY_QUIC_COMMAND_TIMEOUT, connection.open_bi())
            .await
            .map_err(|_| ClientError::Transport("relay QUIC stream timed out".to_string()))?
            .map_err(|err| ClientError::Transport(err.to_string()))?;
        let request = serde_json::to_vec(&command)?;
        timeout(RELAY_QUIC_COMMAND_TIMEOUT, send.write_all(&request))
            .await
            .map_err(|_| ClientError::Transport("relay QUIC write timed out".to_string()))?
            .map_err(|err| ClientError::Transport(err.to_string()))?;
        send.finish()
            .map_err(|err| ClientError::Transport(err.to_string()))?;
        let response = timeout(
            RELAY_QUIC_COMMAND_TIMEOUT,
            recv.read_to_end(16 * 1024 * 1024),
        )
        .await
        .map_err(|_| ClientError::Transport("relay QUIC read timed out".to_string()))?
        .map_err(|err| ClientError::Transport(err.to_string()))?;
        let response: RelayCommandResponse = serde_json::from_slice(&response)?;
        if let RelayCommandResponse::Error { status, message } = &response {
            return Err(ClientError::Transport(format!(
                "relay returned {status}: {message}"
            )));
        }
        Ok(response)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayEnvelope {
    #[serde(default)]
    pub temporary: bool,
    pub initial: Option<InitialMessage>,
    pub wire: WireMessage,
}

#[derive(Debug, Clone)]
pub struct DecryptedDelivery {
    pub message_id: uuid::Uuid,
    pub from_device_id: DeviceId,
    pub body: String,
    pub received_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaySmokePeer {
    pub account_id: AccountId,
    pub device_id: DeviceId,
    pub received: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaySmokeReport {
    pub ok: bool,
    pub relay: String,
    pub relay_health: serde_json::Value,
    pub alice: RelaySmokePeer,
    pub bob: RelaySmokePeer,
    pub bob_invite_uri_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSmokePeer {
    pub account_id: AccountId,
    pub device_id: DeviceId,
    pub received: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSmokeReport {
    pub ok: bool,
    pub relay: String,
    pub relay_health: serde_json::Value,
    pub group_id: uuid::Uuid,
    pub epoch: u64,
    pub apns_registered: bool,
    pub alice: GroupSmokePeer,
    pub bob: GroupSmokePeer,
    pub carol: GroupSmokePeer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pProbeReport {
    pub ok: bool,
    pub relay: String,
    pub rendezvous: String,
    pub local_addr: String,
    pub public_candidate: P2pCandidate,
    pub registered_candidates: Vec<P2pCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pSmokeReport {
    pub ok: bool,
    pub relay: String,
    pub rendezvous: String,
    pub alice_candidate: P2pCandidate,
    pub bob_candidate: P2pCandidate,
    pub alice_saw_bob: Vec<P2pCandidate>,
    pub bob_saw_alice: Vec<P2pCandidate>,
    pub direct_payload: String,
}

pub struct SecureChatDevice {
    keys: DeviceKeyMaterial,
    relay: RelayClient,
    sessions: HashMap<DeviceId, RatchetSession>,
}

impl SecureChatDevice {
    pub fn new(relay: RelayClient) -> Self {
        Self {
            keys: DeviceKeyMaterial::generate(16),
            relay,
            sessions: HashMap::new(),
        }
    }

    pub fn account_id(&self) -> AccountId {
        self.keys.account_id
    }

    pub fn device_id(&self) -> DeviceId {
        self.keys.device_id
    }

    pub fn invite(
        &self,
        relay_hint: Option<String>,
        expires_unix: Option<u64>,
    ) -> Result<Invite, ClientError> {
        Invite::new(&self.keys, relay_hint, expires_unix).map_err(ClientError::from)
    }

    pub async fn register(&self) -> Result<RegisterResponse, ClientError> {
        self.relay.register_device(&self.keys).await
    }

    pub async fn send_to_invite(
        &mut self,
        invite: &Invite,
        body: impl Into<String>,
    ) -> Result<QueuedMessage, ClientError> {
        invite.verify()?;
        let remote_device_id = invite.bundle.identity.device_id;
        let initial = if self.sessions.contains_key(&remote_device_id) {
            None
        } else {
            let (initial, session) =
                start_session_as_initiator(&self.keys, &invite.bundle, CipherSuite::default())?;
            self.sessions.insert(remote_device_id, session);
            Some(initial)
        };
        self.send_with_session(remote_device_id, initial, body.into())
            .await
    }

    pub async fn send_to_session(
        &mut self,
        remote_device_id: DeviceId,
        body: impl Into<String>,
    ) -> Result<QueuedMessage, ClientError> {
        self.send_with_session(remote_device_id, None, body.into())
            .await
    }

    pub async fn drain_plaintexts(&mut self) -> Result<Vec<DecryptedDelivery>, ClientError> {
        let queued = self.relay.drain(&self.keys).await?;
        let mut decrypted = Vec::new();
        for item in queued {
            let frame: TransportFrame = serde_json::from_slice(&item.ciphertext)?;
            let envelope: RelayEnvelope = serde_json::from_slice(&frame.expose()?)?;
            let remote_device_id = envelope.wire.sender_device_id;
            if !self.sessions.contains_key(&remote_device_id) {
                let initial = envelope
                    .initial
                    .as_ref()
                    .ok_or(ClientError::MissingSession(remote_device_id))?;
                let mut session =
                    accept_session_as_responder_consuming_prekey(&mut self.keys, initial)?;
                if session.remote_identity.device_id != remote_device_id {
                    return Err(CryptoError::InvalidInput.into());
                }
                let plain = session.decrypt(&envelope.wire)?;
                self.sessions.insert(remote_device_id, session);
                decrypted.push(DecryptedDelivery {
                    message_id: item.id,
                    from_device_id: remote_device_id,
                    body: plain.body,
                    received_unix: item.received_unix,
                });
                continue;
            }
            let session = self
                .sessions
                .get_mut(&remote_device_id)
                .ok_or(ClientError::MissingSession(remote_device_id))?;
            let plain = session.decrypt(&envelope.wire)?;
            decrypted.push(DecryptedDelivery {
                message_id: item.id,
                from_device_id: remote_device_id,
                body: plain.body,
                received_unix: item.received_unix,
            });
        }
        Ok(decrypted)
    }

    fn session(&mut self, remote_device_id: DeviceId) -> Result<&mut RatchetSession, ClientError> {
        self.sessions
            .get_mut(&remote_device_id)
            .ok_or(ClientError::MissingSession(remote_device_id))
    }

    async fn send_with_session(
        &mut self,
        remote_device_id: DeviceId,
        initial: Option<InitialMessage>,
        body: String,
    ) -> Result<QueuedMessage, ClientError> {
        let session = self.session(remote_device_id)?;
        let to_account_id = session.remote_identity.account_id;
        let to_device_id = session.remote_identity.device_id;
        let wire = session.encrypt(PlainMessage {
            sent_at_unix: now_unix(),
            body,
        })?;
        let envelope = RelayEnvelope {
            temporary: false,
            initial,
            wire,
        };
        let payload = serde_json::to_vec(&envelope)?;
        let frame = TransportFrame::protect(&payload, &padding_profile(payload.len()))?;
        self.relay
            .send(
                &self.keys,
                SendRequest {
                    sender_account_id: Some(self.keys.account_id),
                    sender_device_id: Some(self.keys.device_id),
                    to_account_id,
                    to_device_id,
                    transport_kind: TransportKind::WebSocketTls,
                    sealed_sender: None,
                    ciphertext: serde_json::to_vec(&frame)?,
                    expires_unix: Some(now_unix() + 7 * 24 * 60 * 60),
                    auth: None,
                },
            )
            .await
    }
}

pub struct P2pUdpSocket {
    socket: UdpSocket,
    rendezvous_addr: SocketAddr,
}

impl P2pUdpSocket {
    pub async fn bind(relay_url: &str) -> Result<Self, ClientError> {
        let rendezvous_addr = p2p_rendezvous_addr(relay_url)?;
        Self::bind_with_rendezvous(rendezvous_addr).await
    }

    pub async fn bind_with_rendezvous(rendezvous_addr: SocketAddr) -> Result<Self, ClientError> {
        let bind_addr = if rendezvous_addr.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        let socket = UdpSocket::bind(bind_addr)
            .await
            .map_err(|err| ClientError::Transport(format!("bind p2p UDP socket: {err}")))?;
        Ok(Self {
            socket,
            rendezvous_addr,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, ClientError> {
        self.socket
            .local_addr()
            .map_err(|err| ClientError::Transport(format!("read p2p local addr: {err}")))
    }

    pub fn rendezvous_addr(&self) -> SocketAddr {
        self.rendezvous_addr
    }

    pub async fn probe(&self, keys: &DeviceKeyMaterial) -> Result<P2pCandidate, ClientError> {
        let request = signed_p2p_probe_request(keys)?;
        let bytes = serde_json::to_vec(&request)?;
        self.socket
            .send_to(&bytes, self.rendezvous_addr)
            .await
            .map_err(|err| ClientError::Transport(format!("send p2p probe: {err}")))?;
        let mut buffer = vec![0u8; 64 * 1024];
        let (len, _) = timeout(Duration::from_secs(4), self.socket.recv_from(&mut buffer))
            .await
            .map_err(|_| ClientError::Transport("p2p probe timed out".to_string()))?
            .map_err(|err| ClientError::Transport(format!("receive p2p probe: {err}")))?;
        if let Ok(response) = serde_json::from_slice::<P2pProbeResponse>(&buffer[..len]) {
            return Ok(response.candidate);
        }
        if let Ok(RelayCommandResponse::Error { status, message }) =
            serde_json::from_slice::<RelayCommandResponse>(&buffer[..len])
        {
            return Err(ClientError::Transport(format!(
                "p2p probe failed with {status}: {message}"
            )));
        }
        Err(ClientError::Transport(
            "invalid p2p probe response".to_string(),
        ))
    }

    pub async fn send_direct_datagram(
        &self,
        datagram: &P2pDirectDatagram,
        addr: SocketAddr,
    ) -> Result<usize, ClientError> {
        let bytes = serde_json::to_vec(datagram)?;
        self.socket
            .send_to(&bytes, addr)
            .await
            .map_err(|err| ClientError::Transport(format!("send p2p datagram: {err}")))
    }

    pub async fn receive_direct_datagram(
        &self,
        max_wait: Duration,
    ) -> Result<(P2pDirectDatagram, SocketAddr), ClientError> {
        let mut buffer = vec![0u8; 64 * 1024];
        let (len, addr) = timeout(max_wait, self.socket.recv_from(&mut buffer))
            .await
            .map_err(|_| ClientError::Transport("p2p direct receive timed out".to_string()))?
            .map_err(|err| ClientError::Transport(format!("receive p2p datagram: {err}")))?;
        let datagram = serde_json::from_slice(&buffer[..len])?;
        Ok((datagram, addr))
    }

    pub async fn receive_verified_direct_datagram(
        &self,
        max_wait: Duration,
        sender: &secure_chat_core::PublicDeviceIdentity,
        receiver: &secure_chat_core::PublicDeviceIdentity,
        replay_cache: &mut P2pDirectReplayCache,
    ) -> Result<(P2pDirectDatagram, SocketAddr), ClientError> {
        let (datagram, addr) = self.receive_direct_datagram(max_wait).await?;
        datagram.verify_fresh(sender, receiver, now_unix(), replay_cache)?;
        Ok((datagram, addr))
    }
}

pub async fn run_p2p_probe_against(
    relay_url: &str,
    keys: &DeviceKeyMaterial,
) -> Result<P2pProbeReport, ClientError> {
    let relay = RelayClient::new(relay_url);
    relay.register_device(keys).await?;
    let socket = P2pUdpSocket::bind(relay_url).await?;
    let public_candidate = socket.probe(keys).await?;
    let registered_candidates = relay
        .list_p2p_candidates(keys, keys.account_id, keys.device_id)
        .await?;
    Ok(P2pProbeReport {
        ok: true,
        relay: relay_url.to_string(),
        rendezvous: socket.rendezvous_addr().to_string(),
        local_addr: socket.local_addr()?.to_string(),
        public_candidate,
        registered_candidates,
    })
}

pub async fn run_p2p_smoke() -> Result<P2pSmokeReport, ClientError> {
    let (http_addr, p2p_addr, http_handle, p2p_handle) =
        secure_chat_relay::spawn_ephemeral_with_p2p().await?;
    let relay_url = format!("http://{http_addr}");
    let report = run_p2p_smoke_against(&relay_url, p2p_addr).await?;
    http_handle.abort();
    p2p_handle.abort();
    Ok(report)
}

pub async fn run_p2p_smoke_against(
    relay_url: &str,
    rendezvous_addr: SocketAddr,
) -> Result<P2pSmokeReport, ClientError> {
    let relay = RelayClient::new(relay_url);
    let alice = DeviceKeyMaterial::generate(4);
    let mut bob = DeviceKeyMaterial::generate(4);
    relay.register_device(&alice).await?;
    relay.register_device(&bob).await?;

    let alice_socket = P2pUdpSocket::bind_with_rendezvous(rendezvous_addr).await?;
    let bob_socket = P2pUdpSocket::bind_with_rendezvous(rendezvous_addr).await?;
    let alice_candidate = alice_socket.probe(&alice).await?;
    let bob_candidate = bob_socket.probe(&bob).await?;
    let mut link_bundle = bob.pre_key_bundle();
    link_bundle.one_time_pre_key = None;
    let (link_initial, mut link_session) =
        start_session_as_initiator(&alice, &link_bundle, CipherSuite::default())?;
    let link_wire = link_session.encrypt(PlainMessage {
        sent_at_unix: now_unix(),
        body: "p2p authorization link".to_string(),
    })?;
    let link_envelope = RelayEnvelope {
        temporary: false,
        initial: Some(link_initial),
        wire: link_wire,
    };
    let link_payload = serde_json::to_vec(&link_envelope)?;
    let link_frame = TransportFrame::protect(&link_payload, &padding_profile(link_payload.len()))?;
    let link_request = SendRequest {
        sender_account_id: Some(alice.account_id),
        sender_device_id: Some(alice.device_id),
        to_account_id: bob.account_id,
        to_device_id: bob.device_id,
        transport_kind: TransportKind::QuicUdp,
        sealed_sender: None,
        ciphertext: serde_json::to_vec(&link_frame)?,
        expires_unix: Some(now_unix() + 60),
        auth: None,
    };
    relay.send(&alice, link_request).await?;
    let alice_saw_bob = relay
        .list_p2p_candidates(&alice, bob.account_id, bob.device_id)
        .await?;
    let bob_saw_alice = relay
        .list_p2p_candidates(&bob, alice.account_id, alice.device_id)
        .await?;

    let bob_addr = first_candidate_addr(&alice_saw_bob)?;
    let alice_addr = first_candidate_addr(&bob_saw_alice)?;
    let (direct_initial, mut alice_session) =
        start_session_as_initiator(&alice, &bob.pre_key_bundle(), CipherSuite::default())?;
    let mut bob_session = accept_session_as_responder_consuming_prekey(&mut bob, &direct_initial)?;
    let direct_wire = alice_session.encrypt(PlainMessage {
        sent_at_unix: now_unix(),
        body: "hello over direct p2p".to_string(),
    })?;
    let direct_envelope = RelayEnvelope {
        temporary: false,
        initial: Some(direct_initial),
        wire: direct_wire,
    };
    let direct_payload = serde_json::to_vec(&direct_envelope)?;
    let frame = TransportFrame::protect(&direct_payload, &p2p_probe_profile(direct_payload.len()))?;
    let datagram = P2pDirectDatagram::sign(
        &alice,
        &bob.public_identity(),
        now_unix(),
        serde_json::to_vec(&frame)?,
    );
    let reverse = P2pDirectDatagram::sign(
        &bob,
        &alice.public_identity(),
        now_unix(),
        serde_json::to_vec(&TransportFrame::protect(b"punch", &p2p_probe_profile(5))?)?,
    );
    let _ = bob_socket.send_direct_datagram(&reverse, alice_addr).await;
    for _ in 0..3 {
        alice_socket
            .send_direct_datagram(&datagram, bob_addr)
            .await?;
    }
    let mut replay_cache = P2pDirectReplayCache::new();
    let (received, _) = bob_socket
        .receive_verified_direct_datagram(
            Duration::from_secs(4),
            &alice.public_identity(),
            &bob.public_identity(),
            &mut replay_cache,
        )
        .await?;
    let direct_frame: TransportFrame = serde_json::from_slice(&received.frame)?;
    let direct_envelope: RelayEnvelope = serde_json::from_slice(&direct_frame.expose()?)?;
    let direct_plaintext = bob_session.decrypt(&direct_envelope.wire)?;

    Ok(P2pSmokeReport {
        ok: true,
        relay: relay_url.to_string(),
        rendezvous: rendezvous_addr.to_string(),
        alice_candidate,
        bob_candidate,
        alice_saw_bob,
        bob_saw_alice,
        direct_payload: direct_plaintext.body,
    })
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn padding_profile(payload_len: usize) -> ObfuscationProfile {
    let mut profile = ObfuscationProfile::websocket_fallback();
    profile.fixed_frame_size = padded_bucket(payload_len);
    profile
}

fn p2p_probe_profile(payload_len: usize) -> ObfuscationProfile {
    ObfuscationProfile {
        kind: TransportKind::QuicUdp,
        fixed_frame_size: payload_len.saturating_add(16).max(64),
        timing_jitter_ms: 0,
        cover_traffic: false,
        http3_like_cover: false,
    }
}

fn signed_register_request(keys: &DeviceKeyMaterial) -> Result<RegisterRequest, ClientError> {
    let mut request = RegisterRequest {
        bundle: keys.pre_key_bundle(),
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "register_device",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_publish_p2p_candidates_request(
    keys: &DeviceKeyMaterial,
    candidates: Vec<P2pCandidateDraft>,
) -> Result<secure_chat_core::PublishP2pCandidatesRequest, ClientError> {
    let mut request = secure_chat_core::PublishP2pCandidatesRequest {
        account_id: keys.account_id,
        device_id: keys.device_id,
        candidates,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "publish_p2p_candidates",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_list_p2p_candidates_request(
    keys: &DeviceKeyMaterial,
    target_account_id: AccountId,
    target_device_id: DeviceId,
) -> Result<ListP2pCandidatesRequest, ClientError> {
    let mut request = ListP2pCandidatesRequest {
        requester_account_id: keys.account_id,
        requester_device_id: keys.device_id,
        target_account_id,
        target_device_id,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "list_p2p_candidates",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_p2p_probe_request(keys: &DeviceKeyMaterial) -> Result<P2pProbeRequest, ClientError> {
    let mut request = P2pProbeRequest {
        account_id: keys.account_id,
        device_id: keys.device_id,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "p2p_probe",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_register_apns_token_request(
    keys: &DeviceKeyMaterial,
    token: String,
    platform: ApnsPlatform,
) -> Result<RegisterApnsTokenRequest, ClientError> {
    let mut request = RegisterApnsTokenRequest {
        account_id: keys.account_id,
        device_id: keys.device_id,
        token,
        platform,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "register_apns_token",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_delete_apns_token_request(
    keys: &DeviceKeyMaterial,
    token: Option<String>,
) -> Result<DeleteApnsTokenRequest, ClientError> {
    let mut request = DeleteApnsTokenRequest {
        account_id: keys.account_id,
        device_id: keys.device_id,
        token,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "delete_apns_token",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_publish_mls_key_package_request(
    keys: &DeviceKeyMaterial,
    key_package: Vec<u8>,
) -> Result<PublishMlsKeyPackageRequest, ClientError> {
    let mut request = PublishMlsKeyPackageRequest {
        account_id: keys.account_id,
        device_id: keys.device_id,
        key_package,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "publish_mls_key_package",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_claim_mls_key_package_request(
    keys: &DeviceKeyMaterial,
    target_account_id: AccountId,
    target_device_id: DeviceId,
) -> Result<ClaimMlsKeyPackageRequest, ClientError> {
    let mut request = ClaimMlsKeyPackageRequest {
        requester_account_id: keys.account_id,
        requester_device_id: keys.device_id,
        target_account_id,
        target_device_id,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "claim_mls_key_package",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_send_request(
    keys: &DeviceKeyMaterial,
    mut request: SendRequest,
) -> Result<SendRequest, ClientError> {
    request.auth = None;
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "send_message",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_receipt_request(
    keys: &DeviceKeyMaterial,
    mut request: ReceiptRequest,
) -> Result<ReceiptRequest, ClientError> {
    request.auth = None;
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        "send_receipt",
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn signed_drain_request(
    keys: &DeviceKeyMaterial,
    action: &str,
) -> Result<DrainRequest, ClientError> {
    let mut request = DrainRequest {
        account_id: keys.account_id,
        device_id: keys.device_id,
        auth: None,
    };
    request.auth = Some(secure_chat_core::sign_relay_auth_for_request(
        keys,
        action,
        &request,
        now_unix(),
    )?);
    Ok(request)
}

fn p2p_rendezvous_addr(relay_url: &str) -> Result<SocketAddr, ClientError> {
    if let Ok(addr) = std::env::var("SECURE_CHAT_P2P_RENDEZVOUS_ADDR") {
        return addr
            .parse()
            .map_err(|err| ClientError::Transport(format!("invalid p2p rendezvous addr: {err}")));
    }
    let host = relay_host(relay_url)?;
    (host.as_str(), P2P_RENDEZVOUS_DEFAULT_PORT)
        .to_socket_addrs()
        .map_err(|err| ClientError::Transport(format!("resolve p2p rendezvous: {err}")))?
        .next()
        .ok_or_else(|| ClientError::Transport("could not resolve p2p rendezvous".to_string()))
}

fn insecure_http_error(relay_url: &str) -> Option<String> {
    insecure_http_error_with_override(relay_url, allow_insecure_http())
}

fn insecure_http_error_with_override(relay_url: &str, allow_insecure: bool) -> Option<String> {
    if !relay_url.to_ascii_lowercase().starts_with("http://") {
        return None;
    }
    if allow_insecure {
        return None;
    }
    let host = relay_host(relay_url).ok()?;
    if host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
    {
        return None;
    }
    Some(format!(
        "refusing insecure HTTP relay URL for non-loopback host {host}; use https:// or quic://, or set SECURE_CHAT_ALLOW_INSECURE_HTTP=1 for a local test"
    ))
}

fn allow_insecure_http() -> bool {
    std::env::var("SECURE_CHAT_ALLOW_INSECURE_HTTP")
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn relay_host(relay_url: &str) -> Result<String, ClientError> {
    let without_scheme = relay_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(relay_url);
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, _) = rest
            .split_once(']')
            .ok_or_else(|| ClientError::Transport("invalid IPv6 relay URL".to_string()))?;
        return Ok(host.to_string());
    }
    let host = authority
        .rsplit_once(':')
        .map(|(host, port)| {
            if port.chars().all(|ch| ch.is_ascii_digit()) {
                host
            } else {
                authority
            }
        })
        .unwrap_or(authority);
    if host.is_empty() {
        return Err(ClientError::Transport("missing relay host".to_string()));
    }
    Ok(host.to_string())
}

fn first_candidate_addr(candidates: &[P2pCandidate]) -> Result<SocketAddr, ClientError> {
    candidates
        .iter()
        .find(|candidate| candidate.kind == P2pCandidateKind::ServerReflexive)
        .or_else(|| candidates.first())
        .ok_or_else(|| ClientError::Transport("no p2p candidates available".to_string()))?
        .addr
        .parse()
        .map_err(|err| ClientError::Transport(format!("invalid p2p candidate addr: {err}")))
}

fn padded_bucket(payload_len: usize) -> usize {
    const BUCKET: usize = 1024;
    let minimum = BUCKET;
    let needed = payload_len.saturating_add(16).max(minimum);
    needed.div_ceil(BUCKET) * BUCKET
}

pub async fn run_relay_smoke() -> Result<RelaySmokeReport, ClientError> {
    let (addr, handle) = secure_chat_relay::spawn_ephemeral().await?;
    let relay_url = format!("http://{addr}");
    let report = run_relay_smoke_against(&relay_url).await?;
    handle.abort();
    Ok(report)
}

pub async fn run_relay_smoke_against(relay_url: &str) -> Result<RelaySmokeReport, ClientError> {
    let relay = RelayClient::new(relay_url);
    let relay_health = relay.health().await?;

    let mut alice = SecureChatDevice::new(relay.clone());
    let mut bob = SecureChatDevice::new(relay);
    alice.register().await?;
    bob.register().await?;

    let bob_invite = bob.invite(Some(relay_url.to_string()), None)?;
    let bob_invite_uri = bob_invite.to_uri()?;
    alice
        .send_to_invite(&bob_invite, "hello from Alice via relay")
        .await?;
    let bob_messages = bob.drain_plaintexts().await?;

    bob.send_to_session(alice.device_id(), "hi Alice, decrypted and replied")
        .await?;
    let alice_messages = alice.drain_plaintexts().await?;

    Ok(RelaySmokeReport {
        ok: true,
        relay: relay_url.to_string(),
        relay_health,
        alice: RelaySmokePeer {
            account_id: alice.account_id(),
            device_id: alice.device_id(),
            received: alice_messages.into_iter().map(|msg| msg.body).collect(),
        },
        bob: RelaySmokePeer {
            account_id: bob.account_id(),
            device_id: bob.device_id(),
            received: bob_messages.into_iter().map(|msg| msg.body).collect(),
        },
        bob_invite_uri_prefix: bob_invite_uri.chars().take(32).collect(),
    })
}

pub async fn run_group_smoke() -> Result<GroupSmokeReport, ClientError> {
    let (addr, handle) = secure_chat_relay::spawn_ephemeral().await?;
    let relay_url = format!("http://{addr}");
    let report = run_group_smoke_against(&relay_url).await?;
    handle.abort();
    Ok(report)
}

pub async fn run_group_smoke_against(relay_url: &str) -> Result<GroupSmokeReport, ClientError> {
    let relay = RelayClient::new(relay_url);
    let relay_health = relay.health().await?;
    let alice = DeviceKeyMaterial::generate(16);
    let bob = DeviceKeyMaterial::generate(16);
    let carol = DeviceKeyMaterial::generate(16);
    relay.register_device(&alice).await?;
    relay.register_device(&bob).await?;
    relay.register_device(&carol).await?;
    let apns_registered = relay
        .register_apns_token(&bob, "00deadbeef", ApnsPlatform::Ios)
        .await?
        .registered;
    relay
        .send(
            &alice,
            SendRequest {
                sender_account_id: Some(alice.account_id),
                sender_device_id: Some(alice.device_id),
                to_account_id: bob.account_id,
                to_device_id: bob.device_id,
                transport_kind: TransportKind::RelayHttps,
                sealed_sender: None,
                ciphertext: b"mls-key-package-claim-authorization".to_vec(),
                expires_unix: Some(now_unix() + 60),
                auth: None,
            },
        )
        .await?;
    let _ = relay.drain(&bob).await?;
    relay
        .publish_mls_key_package(&bob, b"mock-openmls-key-package".to_vec())
        .await?;
    let claimed_bob_key_package = relay
        .claim_mls_key_package(&alice, bob.account_id, bob.device_id)
        .await?
        .key_package;

    let mut group = GroupState::create("SecureChat Smoke", "Alice", alice.public_identity())?;
    group.add_member("Bob", bob.public_identity())?;
    group.add_member("Carol", carol.public_identity())?;
    let bob_group = GroupState::from_welcome(group.welcome())?;
    let carol_group = GroupState::from_welcome(group.welcome())?;
    let wire = group.encrypt_message(
        &alice.public_identity(),
        GroupPlainMessage {
            sent_at_unix: now_unix(),
            body: "hello MLS group".to_string(),
        },
    )?;
    let payload = serde_json::to_vec(&group.transport_envelope(wire))?;
    let frame = TransportFrame::protect(&payload, &ObfuscationProfile::websocket_fallback())?;
    let ciphertext = serde_json::to_vec(&frame)?;
    for member in [&bob.public_identity(), &carol.public_identity()] {
        relay
            .send(
                &alice,
                SendRequest {
                    sender_account_id: Some(alice.account_id),
                    sender_device_id: Some(alice.device_id),
                    to_account_id: member.account_id,
                    to_device_id: member.device_id,
                    transport_kind: TransportKind::WebSocketTls,
                    sealed_sender: None,
                    ciphertext: ciphertext.clone(),
                    expires_unix: Some(now_unix() + 60),
                    auth: None,
                },
            )
            .await?;
    }

    let bob_received = drain_group_plaintexts(&relay, &bob, &bob_group).await?;
    let carol_received = drain_group_plaintexts(&relay, &carol, &carol_group).await?;
    Ok(GroupSmokeReport {
        ok: claimed_bob_key_package == Some(b"mock-openmls-key-package".to_vec())
            && bob_received == vec!["hello MLS group".to_string()]
            && carol_received == vec!["hello MLS group".to_string()],
        relay: relay_url.to_string(),
        relay_health,
        group_id: group.group_id,
        epoch: group.epoch,
        apns_registered,
        alice: GroupSmokePeer {
            account_id: alice.account_id,
            device_id: alice.device_id,
            received: Vec::new(),
        },
        bob: GroupSmokePeer {
            account_id: bob.account_id,
            device_id: bob.device_id,
            received: bob_received,
        },
        carol: GroupSmokePeer {
            account_id: carol.account_id,
            device_id: carol.device_id,
            received: carol_received,
        },
    })
}

async fn drain_group_plaintexts(
    relay: &RelayClient,
    keys: &DeviceKeyMaterial,
    group: &GroupState,
) -> Result<Vec<String>, ClientError> {
    let mut bodies = Vec::new();
    for queued in relay.drain(keys).await? {
        let frame: TransportFrame = serde_json::from_slice(&queued.ciphertext)?;
        let envelope: GroupTransportEnvelope = serde_json::from_slice(&frame.expose()?)?;
        if envelope.kind != GROUP_TRANSPORT_KIND {
            continue;
        }
        bodies.push(group.decrypt_message(&envelope.wire)?.body);
    }
    Ok(bodies)
}

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        ClientError::Serialization(serde_json::Error::io(error))
    }
}

fn unexpected_response(response: RelayCommandResponse) -> ClientError {
    ClientError::Transport(format!("unexpected relay response: {response:?}"))
}

fn quic_target(url: &str) -> Result<(String, SocketAddr), ClientError> {
    let target = url
        .strip_prefix("quic://")
        .ok_or_else(|| ClientError::Transport("QUIC relay URL must start with quic://".into()))?;
    let (host, port) = match target.rsplit_once(':') {
        Some((host, port)) => (
            host.to_string(),
            port.parse::<u16>()
                .map_err(|err| ClientError::Transport(format!("invalid QUIC port: {err}")))?,
        ),
        None => (target.to_string(), 8788),
    };
    let addr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|err| ClientError::Transport(err.to_string()))?
        .next()
        .ok_or_else(|| ClientError::Transport("could not resolve QUIC relay".into()))?;
    Ok((host, addr))
}

fn quic_client_config() -> Result<quinn::ClientConfig, ClientError> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let roots = quic_root_cert_store()?;
    let mut crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![RELAY_QUIC_ALPN.to_vec()];
    Ok(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|err| ClientError::Transport(err.to_string()))?,
    )))
}

fn quic_root_cert_store() -> Result<rustls::RootCertStore, ClientError> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let native_certs = rustls_native_certs::load_native_certs();
    if native_cert_env_is_explicit()
        && !native_certs.errors.is_empty()
        && native_certs.certs.is_empty()
    {
        return Err(ClientError::Transport(format!(
            "could not load native certificates: {:?}",
            native_certs.errors
        )));
    }
    roots.add_parsable_certificates(native_certs.certs);

    if let Ok(ca_path) = std::env::var("SECURE_CHAT_QUIC_CA_CERT") {
        let mut reader = BufReader::new(
            File::open(&ca_path)
                .map_err(|err| ClientError::Transport(format!("open QUIC CA cert: {err}")))?,
        );
        for cert in CertificateDer::pem_reader_iter(&mut reader) {
            roots
                .add(cert.map_err(|err| ClientError::Transport(err.to_string()))?)
                .map_err(|err| ClientError::Transport(err.to_string()))?;
        }
    }
    Ok(roots)
}

fn native_cert_env_is_explicit() -> bool {
    std::env::var_os("SSL_CERT_FILE").is_some() || std::env::var_os("SSL_CERT_DIR").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn relay_backed_e2ee_delivery_round_trip() {
        let (addr, handle) = secure_chat_relay::spawn_ephemeral().await.unwrap();
        let relay = RelayClient::new(format!("http://{addr}"));
        relay.health().await.unwrap();

        let mut alice = SecureChatDevice::new(relay.clone());
        let mut bob = SecureChatDevice::new(relay);
        alice.register().await.unwrap();
        bob.register().await.unwrap();

        let bob_invite = bob.invite(Some(format!("http://{addr}")), None).unwrap();
        alice
            .send_to_invite(&bob_invite, "hello through relay")
            .await
            .unwrap();
        let bob_messages = bob.drain_plaintexts().await.unwrap();
        assert_eq!(bob_messages.len(), 1);
        assert_eq!(bob_messages[0].body, "hello through relay");

        bob.send_to_session(alice.device_id(), "reply over the same ratchet")
            .await
            .unwrap();
        let alice_messages = alice.drain_plaintexts().await.unwrap();
        assert_eq!(alice_messages.len(), 1);
        assert_eq!(alice_messages[0].body, "reply over the same ratchet");

        handle.abort();
    }

    #[tokio::test]
    async fn relay_smoke_report_contains_two_way_messages() {
        let report = run_relay_smoke().await.unwrap();
        assert!(report.ok);
        assert_eq!(report.bob.received, vec!["hello from Alice via relay"]);
        assert_eq!(
            report.alice.received,
            vec!["hi Alice, decrypted and replied"]
        );
    }

    #[test]
    fn quic_root_store_has_webpki_fallback() {
        let roots = quic_root_cert_store().unwrap();
        assert!(roots.roots.len() >= webpki_roots::TLS_SERVER_ROOTS.len());
    }

    #[tokio::test]
    async fn p2p_rendezvous_smoke_exchanges_signed_direct_datagram() {
        let report = run_p2p_smoke().await.unwrap();
        assert!(report.ok);
        assert_eq!(report.direct_payload, "hello over direct p2p");
        assert!(!report.alice_saw_bob.is_empty());
        assert!(!report.bob_saw_alice.is_empty());
    }

    #[test]
    fn insecure_http_is_limited_to_loopback_by_default() {
        assert!(insecure_http_error_with_override("http://127.0.0.1:8787", false).is_none());
        assert!(insecure_http_error_with_override("http://[::1]:8787", false).is_none());
        assert!(insecure_http_error_with_override("https://203.0.113.10", false).is_none());
        assert!(insecure_http_error_with_override("http://203.0.113.10:8787", false).is_some());
        assert!(insecure_http_error_with_override("http://203.0.113.10:8787", true).is_none());
    }
}
