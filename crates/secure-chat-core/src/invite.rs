use crate::crypto::{serde_bytes, CryptoError, Key32};
use crate::identity::{
    sign_bytes, verify_signature, AccountId, DeviceId, DeviceKeyMaterial, DevicePreKeyBundle,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Invite {
    pub version: u8,
    pub account_id: AccountId,
    #[serde(default)]
    pub mode: InviteMode,
    pub relay_hint: Option<String>,
    pub expires_unix: Option<u64>,
    pub bundle: DevicePreKeyBundle,
    #[serde(default)]
    pub signature: Option<InviteSignature>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteSignature {
    pub signer_device_id: DeviceId,
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InviteMode {
    #[default]
    Permanent,
    Temporary,
}

impl Invite {
    pub fn new(
        keys: &DeviceKeyMaterial,
        relay_hint: Option<String>,
        expires_unix: Option<u64>,
    ) -> Result<Self, CryptoError> {
        Self::signed(keys, InviteMode::Permanent, relay_hint, expires_unix)
    }

    pub fn temporary(
        keys: &DeviceKeyMaterial,
        relay_hint: Option<String>,
        expires_unix: Option<u64>,
    ) -> Result<Self, CryptoError> {
        Self::signed(keys, InviteMode::Temporary, relay_hint, expires_unix)
    }

    fn signed(
        keys: &DeviceKeyMaterial,
        mode: InviteMode,
        relay_hint: Option<String>,
        expires_unix: Option<u64>,
    ) -> Result<Self, CryptoError> {
        let mut invite = Self {
            version: 1,
            account_id: keys.account_id,
            mode,
            relay_hint,
            expires_unix,
            bundle: keys.pre_key_bundle(),
            signature: None,
        };
        let signature = sign_bytes(&keys.device_signing_key(), &invite.signed_payload()?);
        invite.signature = Some(InviteSignature {
            signer_device_id: keys.device_id,
            signature,
        });
        Ok(invite)
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

    pub fn verify(&self) -> Result<(), CryptoError> {
        if self.version != 1 || self.account_id != self.bundle.identity.account_id {
            return Err(CryptoError::InvalidInput);
        }
        self.bundle.verify()?;
        let signature = self.signature.as_ref().ok_or(CryptoError::BadSignature)?;
        if signature.signer_device_id != self.bundle.identity.device_id {
            return Err(CryptoError::BadSignature);
        }
        verify_signature(
            &self.bundle.identity.device_signing_public,
            &self.signed_payload()?,
            &signature.signature,
        )
    }

    fn signed_payload(&self) -> Result<Vec<u8>, CryptoError> {
        #[derive(Serialize)]
        struct InviteSignedPayload {
            version: u8,
            account_id: AccountId,
            mode: InviteMode,
            relay_hint: Option<String>,
            expires_unix: Option<u64>,
            bundle_hash: Key32,
        }

        serde_bytes(&InviteSignedPayload {
            version: self.version,
            account_id: self.account_id,
            mode: self.mode,
            relay_hint: self.relay_hint.clone(),
            expires_unix: self.expires_unix,
            bundle_hash: self.bundle.transcript_hash()?,
        })
    }
}
