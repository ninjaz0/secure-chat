use crate::crypto::{random_bytes, serde_bytes, sha256, CryptoError, Key32};
use crate::identity::{
    sign_bytes, verify_signature, AccountId, DeviceId, DeviceKeyMaterial, DevicePreKeyBundle,
};
use crate::transport::TransportKind;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use uuid::Uuid;

pub const RELAY_QUIC_ALPN: &[u8] = b"secure-chat-relay/1";
pub const RELAY_AUTH_MAX_SKEW_SECS: u64 = 5 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayAuth {
    pub account_id: AccountId,
    pub device_id: DeviceId,
    pub issued_unix: u64,
    pub nonce: [u8; 16],
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrainRequest {
    pub account_id: AccountId,
    pub device_id: DeviceId,
    pub auth: Option<RelayAuth>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub bundle: DevicePreKeyBundle,
    pub auth: Option<RelayAuth>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub account_id: AccountId,
    pub device_id: DeviceId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendRequest {
    pub sender_account_id: Option<AccountId>,
    pub sender_device_id: Option<DeviceId>,
    pub to_account_id: AccountId,
    pub to_device_id: DeviceId,
    pub transport_kind: TransportKind,
    pub sealed_sender: Option<Vec<u8>>,
    pub ciphertext: Vec<u8>,
    pub expires_unix: Option<u64>,
    pub auth: Option<RelayAuth>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub id: Uuid,
    pub sender_account_id: Option<AccountId>,
    pub sender_device_id: Option<DeviceId>,
    pub transport_kind: TransportKind,
    pub sealed_sender: Option<Vec<u8>>,
    pub ciphertext: Vec<u8>,
    pub received_unix: u64,
    pub expires_unix: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrainResponse {
    pub messages: Vec<QueuedMessage>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    Delivered,
    Read,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiptRequest {
    pub message_id: Uuid,
    pub from_account_id: AccountId,
    pub from_device_id: DeviceId,
    pub to_account_id: AccountId,
    pub to_device_id: DeviceId,
    pub kind: ReceiptKind,
    pub at_unix: u64,
    pub auth: Option<RelayAuth>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedReceipt {
    pub id: Uuid,
    pub message_id: Uuid,
    pub from_account_id: AccountId,
    pub from_device_id: DeviceId,
    pub kind: ReceiptKind,
    pub at_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrainReceiptsResponse {
    pub receipts: Vec<QueuedReceipt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RelayCommand {
    Health,
    RegisterDevice(RegisterRequest),
    ListDevices { account_id: AccountId },
    SendMessage(SendRequest),
    DrainMessages(DrainRequest),
    SendReceipt(ReceiptRequest),
    DrainReceipts(DrainRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RelayCommandResponse {
    Health(serde_json::Value),
    RegisterDevice(RegisterResponse),
    ListDevices(Vec<DevicePreKeyBundle>),
    SendMessage(QueuedMessage),
    DrainMessages(DrainResponse),
    SendReceipt(QueuedReceipt),
    DrainReceipts(DrainReceiptsResponse),
    Error { status: u16, message: String },
}

pub fn sign_relay_auth_for_request<T: Serialize>(
    keys: &DeviceKeyMaterial,
    action: &str,
    request: &T,
    issued_unix: u64,
) -> Result<RelayAuth, CryptoError> {
    let request_digest = relay_request_digest(request)?;
    let mut auth = RelayAuth {
        account_id: keys.account_id,
        device_id: keys.device_id,
        issued_unix,
        nonce: random_bytes::<16>(),
        signature: [0u8; 64],
    };
    auth.signature = sign_bytes(
        &keys.device_signing_key(),
        &relay_auth_payload(action, &request_digest, &auth),
    );
    Ok(auth)
}

pub fn verify_relay_auth_for_request<T: Serialize>(
    device_signing_public: &Key32,
    action: &str,
    request: &T,
    auth: &RelayAuth,
    now_unix: u64,
) -> Result<(), CryptoError> {
    if auth.issued_unix.abs_diff(now_unix) > RELAY_AUTH_MAX_SKEW_SECS {
        return Err(CryptoError::InvalidInput);
    }
    let request_digest = relay_request_digest(request)?;
    verify_signature(
        device_signing_public,
        &relay_auth_payload(action, &request_digest, auth),
        &auth.signature,
    )
}

fn relay_request_digest<T: Serialize>(request: &T) -> Result<Key32, CryptoError> {
    Ok(sha256(&[&serde_bytes(request)?]))
}

fn relay_auth_payload(action: &str, request_digest: &Key32, auth: &RelayAuth) -> Vec<u8> {
    [
        b"secure-chat-v1/relay-auth".as_slice(),
        action.as_bytes(),
        auth.account_id.as_bytes(),
        auth.device_id.as_bytes(),
        &auth.issued_unix.to_be_bytes(),
        auth.nonce.as_slice(),
        request_digest.as_slice(),
    ]
    .concat()
}
