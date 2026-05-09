use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce as AesNonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaChaNonce};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::fmt;
use thiserror::Error;

pub type Key32 = [u8; 32];
pub type Nonce12 = [u8; 12];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CipherSuite {
    #[default]
    ChaCha20Poly1305,
    Aes256Gcm,
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid cryptographic input")]
    InvalidInput,
    #[error("signature verification failed")]
    BadSignature,
    #[error("authenticated decryption failed")]
    DecryptionFailed,
    #[error("key derivation failed")]
    KdfFailed,
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("ratchet skipped-key limit exceeded")]
    TooManySkippedKeys,
    #[error("message replay or duplicate chain position")]
    ReplayOrDuplicate,
    #[error("protocol state is missing a required chain")]
    MissingChain,
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

pub fn sha256(parts: &[&[u8]]) -> Key32 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

pub fn hkdf_expand(
    salt: &[u8],
    ikm: &[u8],
    info: &[u8],
    len: usize,
) -> Result<Vec<u8>, CryptoError> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = vec![0u8; len];
    hk.expand(info, &mut out)
        .map_err(|_| CryptoError::KdfFailed)?;
    Ok(out)
}

pub fn derive_initial_secret(
    transcript_hash: &[u8],
    dh_outputs: &[Key32],
) -> Result<Key32, CryptoError> {
    let mut ikm = Vec::with_capacity(dh_outputs.len() * 32);
    for output in dh_outputs {
        ikm.extend_from_slice(output);
    }
    let out = hkdf_expand(
        b"secure-chat-v1/x3dh",
        &ikm,
        &[b"initial-secret".as_slice(), transcript_hash].concat(),
        32,
    )?;
    out.try_into().map_err(|_| CryptoError::KdfFailed)
}

pub fn kdf_root(root_key: &Key32, dh_output: &Key32) -> Result<(Key32, Key32), CryptoError> {
    let out = hkdf_expand(
        root_key,
        dh_output,
        b"secure-chat-v1/double-ratchet/root",
        64,
    )?;
    Ok((
        out[0..32].try_into().map_err(|_| CryptoError::KdfFailed)?,
        out[32..64].try_into().map_err(|_| CryptoError::KdfFailed)?,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSecrets {
    pub body_key: Key32,
    pub header_key: Key32,
}

pub fn kdf_chain(chain_key: &Key32) -> Result<(Key32, MessageSecrets), CryptoError> {
    let out = hkdf_expand(
        chain_key,
        b"message-step",
        b"secure-chat-v1/double-ratchet/chain",
        96,
    )?;
    Ok((
        out[0..32].try_into().map_err(|_| CryptoError::KdfFailed)?,
        MessageSecrets {
            body_key: out[32..64].try_into().map_err(|_| CryptoError::KdfFailed)?,
            header_key: out[64..96].try_into().map_err(|_| CryptoError::KdfFailed)?,
        },
    ))
}

pub fn encrypt_aead(
    suite: CipherSuite,
    key: &Key32,
    nonce: &Nonce12,
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    match suite {
        CipherSuite::ChaCha20Poly1305 => {
            let cipher =
                ChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::InvalidInput)?;
            cipher
                .encrypt(
                    ChaChaNonce::from_slice(nonce),
                    Payload {
                        msg: plaintext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| CryptoError::DecryptionFailed)
        }
        CipherSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidInput)?;
            cipher
                .encrypt(
                    AesNonce::from_slice(nonce),
                    Payload {
                        msg: plaintext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| CryptoError::DecryptionFailed)
        }
    }
}

pub fn decrypt_aead(
    suite: CipherSuite,
    key: &Key32,
    nonce: &Nonce12,
    ciphertext: &[u8],
    associated_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    match suite {
        CipherSuite::ChaCha20Poly1305 => {
            let cipher =
                ChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::InvalidInput)?;
            cipher
                .decrypt(
                    ChaChaNonce::from_slice(nonce),
                    Payload {
                        msg: ciphertext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| CryptoError::DecryptionFailed)
        }
        CipherSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidInput)?;
            cipher
                .decrypt(
                    AesNonce::from_slice(nonce),
                    Payload {
                        msg: ciphertext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| CryptoError::DecryptionFailed)
        }
    }
}

pub fn serde_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CryptoError> {
    serde_json::to_vec(value).map_err(|err| CryptoError::Serialization(err.to_string()))
}

pub mod base64_bytes {
    use super::*;

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(BytesVisitor)
    }

    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Vec<u8>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("base64url bytes or a legacy byte array")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            URL_SAFE_NO_PAD.decode(value).map_err(E::custom)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut bytes = Vec::new();
            while let Some(byte) = seq.next_element::<u8>()? {
                bytes.push(byte);
            }
            Ok(bytes)
        }
    }
}
