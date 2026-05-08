use crate::crypto::{random_bytes, serde_bytes, CryptoError, Key32};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use uuid::Uuid;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

pub type AccountId = Uuid;
pub type DeviceId = Uuid;
pub type PreKeyId = u32;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicDeviceIdentity {
    pub account_id: AccountId,
    pub device_id: DeviceId,
    pub account_signing_public: Key32,
    pub device_signing_public: Key32,
    pub identity_x25519_public: Key32,
    #[serde(with = "BigArray")]
    pub device_cert_signature: [u8; 64],
    #[serde(default = "empty_signature", with = "BigArray")]
    pub account_device_signature: [u8; 64],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicOneTimePreKey {
    pub id: PreKeyId,
    pub public_key: Key32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevicePreKeyBundle {
    pub identity: PublicDeviceIdentity,
    pub signed_pre_key_id: PreKeyId,
    pub signed_pre_key_public: Key32,
    #[serde(with = "BigArray")]
    pub signed_pre_key_signature: [u8; 64],
    pub one_time_pre_key: Option<PublicOneTimePreKey>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OneTimePreKeyMaterial {
    pub id: PreKeyId,
    pub secret: Key32,
    pub public_key: Key32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceKeyMaterial {
    pub account_id: AccountId,
    pub account_signing_secret: Key32,
    pub account_signing_public: Key32,
    pub device_id: DeviceId,
    pub device_signing_secret: Key32,
    pub device_signing_public: Key32,
    pub identity_x25519_secret: Key32,
    pub identity_x25519_public: Key32,
    pub signed_pre_key_id: PreKeyId,
    pub signed_pre_key_secret: Key32,
    pub signed_pre_key_public: Key32,
    #[serde(with = "BigArray")]
    pub signed_pre_key_signature: [u8; 64],
    pub one_time_pre_keys: Vec<OneTimePreKeyMaterial>,
    #[serde(with = "BigArray")]
    pub device_cert_signature: [u8; 64],
    #[serde(default = "empty_signature", with = "BigArray")]
    pub account_device_signature: [u8; 64],
}

fn empty_signature() -> [u8; 64] {
    [0u8; 64]
}

impl DeviceKeyMaterial {
    pub fn generate(one_time_pre_key_count: usize) -> Self {
        let mut rng = OsRng;
        let account_signing = SigningKey::generate(&mut rng);
        let device_signing = SigningKey::generate(&mut rng);
        let identity_secret = StaticSecret::random_from_rng(&mut rng);
        let signed_pre_key_secret = StaticSecret::random_from_rng(&mut rng);
        let signed_pre_key_public = X25519PublicKey::from(&signed_pre_key_secret).to_bytes();
        let device_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let identity_x25519_public = X25519PublicKey::from(&identity_secret).to_bytes();
        let device_signing_public = device_signing.verifying_key().to_bytes();
        let account_signing_public = account_signing.verifying_key().to_bytes();
        let signed_pre_key_id = 1;
        let identity_payload = identity_binding_payload(
            account_id,
            &account_signing_public,
            device_id,
            &device_signing_public,
            &identity_x25519_public,
        );
        let signed_pre_key_signature = sign_bytes(
            &device_signing,
            &signed_pre_key_payload(
                account_id,
                device_id,
                signed_pre_key_id,
                &signed_pre_key_public,
            ),
        );
        let device_cert_signature = sign_bytes(&account_signing, &identity_payload);
        let account_device_signature = sign_bytes(&device_signing, &identity_payload);
        let one_time_pre_keys = (0..one_time_pre_key_count)
            .map(|idx| {
                let secret = StaticSecret::random_from_rng(&mut rng);
                OneTimePreKeyMaterial {
                    id: idx as PreKeyId + 1,
                    public_key: X25519PublicKey::from(&secret).to_bytes(),
                    secret: secret.to_bytes(),
                }
            })
            .collect();

        Self {
            account_id,
            account_signing_secret: account_signing.to_bytes(),
            account_signing_public,
            device_id,
            device_signing_secret: device_signing.to_bytes(),
            device_signing_public,
            identity_x25519_secret: identity_secret.to_bytes(),
            identity_x25519_public,
            signed_pre_key_id,
            signed_pre_key_secret: signed_pre_key_secret.to_bytes(),
            signed_pre_key_public,
            signed_pre_key_signature,
            one_time_pre_keys,
            device_cert_signature,
            account_device_signature,
        }
    }

    pub fn refresh_signatures(&mut self) {
        let account_signing = self.account_signing_key();
        let device_signing = self.device_signing_key();
        self.account_signing_public = account_signing.verifying_key().to_bytes();
        self.device_signing_public = device_signing.verifying_key().to_bytes();
        let identity_secret = StaticSecret::from(self.identity_x25519_secret);
        self.identity_x25519_public = X25519PublicKey::from(&identity_secret).to_bytes();
        let signed_pre_key_secret = StaticSecret::from(self.signed_pre_key_secret);
        self.signed_pre_key_public = X25519PublicKey::from(&signed_pre_key_secret).to_bytes();
        self.signed_pre_key_signature = sign_bytes(
            &device_signing,
            &signed_pre_key_payload(
                self.account_id,
                self.device_id,
                self.signed_pre_key_id,
                &self.signed_pre_key_public,
            ),
        );
        for one_time in &mut self.one_time_pre_keys {
            let secret = StaticSecret::from(one_time.secret);
            one_time.public_key = X25519PublicKey::from(&secret).to_bytes();
        }
        let identity_payload = identity_binding_payload(
            self.account_id,
            &self.account_signing_public,
            self.device_id,
            &self.device_signing_public,
            &self.identity_x25519_public,
        );
        self.device_cert_signature = sign_bytes(&account_signing, &identity_payload);
        self.account_device_signature = sign_bytes(&device_signing, &identity_payload);
    }

    pub fn ensure_current_signatures(&mut self) -> Result<bool, CryptoError> {
        if self.pre_key_bundle().verify().is_ok() {
            return Ok(false);
        }
        self.refresh_signatures();
        self.pre_key_bundle().verify()?;
        Ok(true)
    }

    pub fn public_identity(&self) -> PublicDeviceIdentity {
        PublicDeviceIdentity {
            account_id: self.account_id,
            device_id: self.device_id,
            account_signing_public: self.account_signing_public,
            device_signing_public: self.device_signing_public,
            identity_x25519_public: self.identity_x25519_public,
            device_cert_signature: self.device_cert_signature,
            account_device_signature: self.account_device_signature,
        }
    }

    pub fn pre_key_bundle(&self) -> DevicePreKeyBundle {
        DevicePreKeyBundle {
            identity: self.public_identity(),
            signed_pre_key_id: self.signed_pre_key_id,
            signed_pre_key_public: self.signed_pre_key_public,
            signed_pre_key_signature: self.signed_pre_key_signature,
            one_time_pre_key: self
                .one_time_pre_keys
                .first()
                .map(|key| PublicOneTimePreKey {
                    id: key.id,
                    public_key: key.public_key,
                }),
        }
    }

    pub fn find_one_time_pre_key_secret(&self, id: PreKeyId) -> Option<Key32> {
        self.one_time_pre_keys
            .iter()
            .find(|key| key.id == id)
            .map(|key| key.secret)
    }

    pub fn consume_one_time_pre_key(&mut self, id: PreKeyId) -> Option<Key32> {
        let idx = self.one_time_pre_keys.iter().position(|key| key.id == id)?;
        Some(self.one_time_pre_keys.remove(idx).secret)
    }

    pub fn device_signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.device_signing_secret)
    }

    pub fn account_signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.account_signing_secret)
    }
}

impl DevicePreKeyBundle {
    pub fn verify(&self) -> Result<(), CryptoError> {
        verify_signature(
            &self.identity.account_signing_public,
            &identity_binding_payload(
                self.identity.account_id,
                &self.identity.account_signing_public,
                self.identity.device_id,
                &self.identity.device_signing_public,
                &self.identity.identity_x25519_public,
            ),
            &self.identity.device_cert_signature,
        )?;
        verify_signature(
            &self.identity.device_signing_public,
            &identity_binding_payload(
                self.identity.account_id,
                &self.identity.account_signing_public,
                self.identity.device_id,
                &self.identity.device_signing_public,
                &self.identity.identity_x25519_public,
            ),
            &self.identity.account_device_signature,
        )?;
        verify_signature(
            &self.identity.device_signing_public,
            &signed_pre_key_payload(
                self.identity.account_id,
                self.identity.device_id,
                self.signed_pre_key_id,
                &self.signed_pre_key_public,
            ),
            &self.signed_pre_key_signature,
        )
    }

    pub fn transcript_hash(&self) -> Result<Key32, CryptoError> {
        Ok(crate::crypto::sha256(&[&serde_bytes(self)?]))
    }
}

pub fn sign_bytes(signing_key: &SigningKey, payload: &[u8]) -> [u8; 64] {
    signing_key.sign(payload).to_bytes()
}

pub fn verify_signature(
    public_key: &Key32,
    payload: &[u8],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    let verifying_key =
        VerifyingKey::from_bytes(public_key).map_err(|_| CryptoError::InvalidInput)?;
    let signature = Signature::from_bytes(signature);
    verifying_key
        .verify(payload, &signature)
        .map_err(|_| CryptoError::BadSignature)
}

pub fn identity_binding_payload(
    account_id: AccountId,
    account_signing_public: &Key32,
    device_id: DeviceId,
    device_signing_public: &Key32,
    identity_x25519_public: &Key32,
) -> Vec<u8> {
    [
        b"secure-chat-v2/identity-binding".as_slice(),
        account_id.as_bytes(),
        account_signing_public.as_slice(),
        device_id.as_bytes(),
        device_signing_public.as_slice(),
        identity_x25519_public.as_slice(),
    ]
    .concat()
}

pub fn signed_pre_key_payload(
    account_id: AccountId,
    device_id: DeviceId,
    pre_key_id: PreKeyId,
    signed_pre_key_public: &Key32,
) -> Vec<u8> {
    [
        b"secure-chat-v2/signed-pre-key".as_slice(),
        account_id.as_bytes(),
        device_id.as_bytes(),
        &pre_key_id.to_be_bytes(),
        signed_pre_key_public.as_slice(),
    ]
    .concat()
}

pub fn x25519(secret: &Key32, public: &Key32) -> Result<Key32, CryptoError> {
    let secret = StaticSecret::from(*secret);
    let public = X25519PublicKey::from(*public);
    let shared = secret.diffie_hellman(&public).to_bytes();
    if shared.iter().all(|byte| *byte == 0) {
        return Err(CryptoError::InvalidInput);
    }
    Ok(shared)
}

pub fn new_x25519_keypair() -> (Key32, Key32) {
    let secret = StaticSecret::from(random_bytes::<32>());
    let public = X25519PublicKey::from(&secret).to_bytes();
    (secret.to_bytes(), public)
}
