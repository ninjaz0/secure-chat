use crate::crypto::{base64_bytes, random_bytes, sha256, CryptoError};
use crate::identity::{
    sign_bytes, verify_signature, AccountId, DeviceId, DeviceKeyMaterial, PublicDeviceIdentity,
};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::collections::{HashMap, VecDeque};

pub const P2P_DIRECT_MAX_SKEW_SECS: u64 = 5 * 60;
const P2P_DIRECT_REPLAY_CACHE_PER_DEVICE: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    QuicUdp,
    WebSocketTls,
    RelayHttps,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObfuscationProfile {
    pub kind: TransportKind,
    pub fixed_frame_size: usize,
    pub timing_jitter_ms: u16,
    pub cover_traffic: bool,
    pub http3_like_cover: bool,
}

impl ObfuscationProfile {
    pub fn stealth_quic() -> Self {
        Self {
            kind: TransportKind::QuicUdp,
            fixed_frame_size: 1200,
            timing_jitter_ms: 300,
            cover_traffic: true,
            http3_like_cover: true,
        }
    }

    pub fn websocket_fallback() -> Self {
        Self {
            kind: TransportKind::WebSocketTls,
            fixed_frame_size: 1024,
            timing_jitter_ms: 600,
            cover_traffic: false,
            http3_like_cover: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportFrame {
    pub version: u8,
    pub kind: TransportKind,
    pub original_len: u32,
    #[serde(with = "base64_bytes")]
    pub padded_body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pDirectDatagram {
    pub version: u8,
    pub sender_account_id: AccountId,
    pub sender_device_id: DeviceId,
    pub receiver_account_id: AccountId,
    pub receiver_device_id: DeviceId,
    pub sent_unix: u64,
    pub nonce: [u8; 16],
    #[serde(with = "base64_bytes")]
    pub frame: Vec<u8>,
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}

#[derive(Default)]
pub struct P2pDirectReplayCache {
    seen: HashMap<DeviceId, VecDeque<[u8; 16]>>,
}

impl P2pDirectReplayCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn remember(&mut self, sender_device_id: DeviceId, nonce: [u8; 16]) -> Result<(), CryptoError> {
        let nonces = self.seen.entry(sender_device_id).or_default();
        if nonces.iter().any(|seen| *seen == nonce) {
            return Err(CryptoError::ReplayOrDuplicate);
        }
        nonces.push_back(nonce);
        while nonces.len() > P2P_DIRECT_REPLAY_CACHE_PER_DEVICE {
            nonces.pop_front();
        }
        Ok(())
    }
}

impl TransportFrame {
    pub fn protect(payload: &[u8], profile: &ObfuscationProfile) -> Result<Self, CryptoError> {
        if payload.len() > u32::MAX as usize || payload.len() + 4 > profile.fixed_frame_size {
            return Err(CryptoError::InvalidInput);
        }
        let padding_len = profile.fixed_frame_size - payload.len();
        let mut padded_body = Vec::with_capacity(profile.fixed_frame_size);
        padded_body.extend_from_slice(payload);
        if padding_len > 0 {
            padded_body.extend_from_slice(&random_bytes_vec(padding_len));
        }
        Ok(Self {
            version: 1,
            kind: profile.kind,
            original_len: payload.len() as u32,
            padded_body,
        })
    }

    pub fn expose(&self) -> Result<Vec<u8>, CryptoError> {
        if self.version != 1 {
            return Err(CryptoError::InvalidInput);
        }
        let len = self.original_len as usize;
        if len > self.padded_body.len() {
            return Err(CryptoError::InvalidInput);
        }
        Ok(self.padded_body[..len].to_vec())
    }
}

impl P2pDirectDatagram {
    pub fn sign(
        keys: &DeviceKeyMaterial,
        receiver: &PublicDeviceIdentity,
        sent_unix: u64,
        frame: Vec<u8>,
    ) -> Self {
        let mut datagram = Self {
            version: 1,
            sender_account_id: keys.account_id,
            sender_device_id: keys.device_id,
            receiver_account_id: receiver.account_id,
            receiver_device_id: receiver.device_id,
            sent_unix,
            nonce: random_bytes::<16>(),
            frame,
            signature: [0u8; 64],
        };
        datagram.signature = sign_bytes(&keys.device_signing_key(), &datagram.signature_payload());
        datagram
    }

    pub fn verify(
        &self,
        sender: &PublicDeviceIdentity,
        receiver: &PublicDeviceIdentity,
    ) -> Result<(), CryptoError> {
        if self.version != 1
            || self.sender_account_id != sender.account_id
            || self.sender_device_id != sender.device_id
            || self.receiver_account_id != receiver.account_id
            || self.receiver_device_id != receiver.device_id
        {
            return Err(CryptoError::InvalidInput);
        }
        verify_signature(
            &sender.device_signing_public,
            &self.signature_payload(),
            &self.signature,
        )
    }

    pub fn verify_fresh(
        &self,
        sender: &PublicDeviceIdentity,
        receiver: &PublicDeviceIdentity,
        now_unix: u64,
        replay_cache: &mut P2pDirectReplayCache,
    ) -> Result<(), CryptoError> {
        self.verify(sender, receiver)?;
        if self.sent_unix.abs_diff(now_unix) > P2P_DIRECT_MAX_SKEW_SECS {
            return Err(CryptoError::InvalidInput);
        }
        replay_cache.remember(self.sender_device_id, self.nonce)
    }

    fn signature_payload(&self) -> Vec<u8> {
        [
            b"secure-chat-v1/p2p-direct-datagram".as_slice(),
            &[self.version],
            self.sender_account_id.as_bytes(),
            self.sender_device_id.as_bytes(),
            self.receiver_account_id.as_bytes(),
            self.receiver_device_id.as_bytes(),
            &self.sent_unix.to_be_bytes(),
            self.nonce.as_slice(),
            sha256(&[self.frame.as_slice()]).as_slice(),
        ]
        .concat()
    }
}

fn random_bytes_vec(len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    for chunk in out.chunks_mut(32) {
        let random = random_bytes::<32>();
        chunk.copy_from_slice(&random[..chunk.len()]);
    }
    out
}
