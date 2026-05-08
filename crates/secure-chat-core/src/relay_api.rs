use crate::identity::{AccountId, DeviceId, DevicePreKeyBundle};
use crate::transport::TransportKind;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RELAY_QUIC_ALPN: &[u8] = b"secure-chat-relay/1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub bundle: DevicePreKeyBundle,
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
    ListDevices {
        account_id: AccountId,
    },
    SendMessage(SendRequest),
    DrainMessages {
        account_id: AccountId,
        device_id: DeviceId,
    },
    SendReceipt(ReceiptRequest),
    DrainReceipts {
        account_id: AccountId,
        device_id: DeviceId,
    },
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
