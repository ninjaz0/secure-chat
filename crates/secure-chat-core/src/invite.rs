use crate::crypto::CryptoError;
use crate::identity::{AccountId, DevicePreKeyBundle};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Invite {
    pub version: u8,
    pub account_id: AccountId,
    pub relay_hint: Option<String>,
    pub expires_unix: Option<u64>,
    pub bundle: DevicePreKeyBundle,
}

impl Invite {
    pub fn new(
        bundle: DevicePreKeyBundle,
        relay_hint: Option<String>,
        expires_unix: Option<u64>,
    ) -> Self {
        Self {
            version: 1,
            account_id: bundle.identity.account_id,
            relay_hint,
            expires_unix,
            bundle,
        }
    }

    pub fn to_uri(&self) -> Result<String, CryptoError> {
        let bytes =
            serde_json::to_vec(self).map_err(|err| CryptoError::Serialization(err.to_string()))?;
        Ok(format!("schat://invite/{}", URL_SAFE_NO_PAD.encode(bytes)))
    }

    pub fn from_uri(uri: &str) -> Result<Self, CryptoError> {
        let payload = uri
            .strip_prefix("schat://invite/")
            .ok_or(CryptoError::InvalidInput)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| CryptoError::InvalidInput)?;
        serde_json::from_slice(&bytes).map_err(|err| CryptoError::Serialization(err.to_string()))
    }
}
