use crate::crypto::{sha256, Key32};
use crate::identity::PublicDeviceIdentity;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyFingerprint {
    pub protocol: &'static str,
    pub fingerprint_hex: String,
    pub number: String,
    pub qr_payload: String,
}

pub fn safety_number(
    local_devices: &[PublicDeviceIdentity],
    remote_devices: &[PublicDeviceIdentity],
) -> SafetyFingerprint {
    let mut groups = [
        device_group_digest(local_devices),
        device_group_digest(remote_devices),
    ];
    groups.sort();
    let raw = sha256(&[
        b"secure-chat-v1/safety-number",
        groups[0].as_slice(),
        groups[1].as_slice(),
    ]);
    let number = decimal_groups(&raw);
    let fingerprint_hex = to_hex(&raw);
    let qr_payload = serde_json::json!({
        "type": "secure-chat/safety-v1",
        "protocol": "secure-chat-v1",
        "fingerprint": fingerprint_hex,
        "number": number,
    })
    .to_string();
    SafetyFingerprint {
        protocol: "secure-chat-v1",
        fingerprint_hex,
        number,
        qr_payload,
    }
}

pub fn device_group_digest(devices: &[PublicDeviceIdentity]) -> Key32 {
    let mut devices = devices.to_vec();
    devices.sort_by_key(|device| (device.account_id, device.device_id));
    let mut parts: Vec<Vec<u8>> = Vec::new();
    for device in devices {
        parts.push(device.account_id.as_bytes().to_vec());
        parts.push(device.device_id.as_bytes().to_vec());
        parts.push(device.account_signing_public.to_vec());
        parts.push(device.device_signing_public.to_vec());
        parts.push(device.identity_x25519_public.to_vec());
    }
    let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    sha256(&refs)
}

fn decimal_groups(raw: &Key32) -> String {
    let mut digits = String::with_capacity(60 + 11);
    for idx in 0..60 {
        if idx > 0 && idx % 5 == 0 {
            digits.push(' ');
        }
        let byte = raw[idx % raw.len()];
        let digit = ((byte as usize + idx * 17 + (raw[(idx * 7) % raw.len()] as usize)) % 10) as u8;
        digits.push((b'0' + digit) as char);
    }
    digits
}

pub fn to_hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}
