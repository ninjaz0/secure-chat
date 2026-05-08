use crate::crypto::{random_bytes, CryptoError};
use serde::{Deserialize, Serialize};

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
    pub padded_body: Vec<u8>,
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
        let len = self.original_len as usize;
        if len > self.padded_body.len() {
            return Err(CryptoError::InvalidInput);
        }
        Ok(self.padded_body[..len].to_vec())
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
