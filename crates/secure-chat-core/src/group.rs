use crate::crypto::{
    decrypt_aead, encrypt_aead, hkdf_expand, random_bytes, serde_bytes, CipherSuite, CryptoError,
    Key32, Nonce12,
};
use crate::identity::{
    sign_bytes, verify_signature, AccountId, DeviceId, DeviceKeyMaterial, PublicDeviceIdentity,
};
use openmls::prelude::Ciphersuite;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const GROUP_TRANSPORT_KIND: &str = "secure-chat/group-v1";
pub const GROUP_CONTROL_PREFIX: &str = "securechat-control:";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupMember {
    pub display_name: String,
    pub identity: PublicDeviceIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupState {
    pub group_id: Uuid,
    pub display_name: String,
    pub epoch: u64,
    pub secret: Key32,
    pub members: Vec<GroupMember>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupWelcome {
    pub protocol: String,
    pub mls_ciphersuite: String,
    pub group_id: Uuid,
    pub display_name: String,
    pub epoch: u64,
    pub secret: Key32,
    pub members: Vec<GroupMember>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupPlainMessage {
    pub sent_at_unix: u64,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupWireMessage {
    pub protocol: String,
    pub mls_ciphersuite: String,
    pub group_id: Uuid,
    pub epoch: u64,
    pub sender_account_id: AccountId,
    pub sender_device_id: DeviceId,
    pub nonce: Nonce12,
    pub ciphertext: Vec<u8>,
    #[serde(with = "serde_big_array::BigArray")]
    pub sender_signature: [u8; 64],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupTransportEnvelope {
    pub kind: String,
    pub group_id: Uuid,
    pub wire: GroupWireMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum GroupControlMessage {
    Welcome(GroupWelcome),
}

impl GroupState {
    pub fn create(
        display_name: impl Into<String>,
        creator_display_name: impl Into<String>,
        creator_identity: PublicDeviceIdentity,
    ) -> Result<Self, CryptoError> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(CryptoError::InvalidInput);
        }
        Ok(Self {
            group_id: Uuid::new_v4(),
            display_name,
            epoch: 1,
            secret: random_bytes::<32>(),
            members: vec![GroupMember {
                display_name: creator_display_name.into(),
                identity: creator_identity,
            }],
        })
    }

    pub fn from_welcome(welcome: GroupWelcome) -> Result<Self, CryptoError> {
        if welcome.protocol != group_protocol_label() {
            return Err(CryptoError::InvalidInput);
        }
        Ok(Self {
            group_id: welcome.group_id,
            display_name: welcome.display_name,
            epoch: welcome.epoch,
            secret: welcome.secret,
            members: welcome.members,
        })
    }

    pub fn add_member(
        &mut self,
        display_name: impl Into<String>,
        identity: PublicDeviceIdentity,
    ) -> Result<GroupWelcome, CryptoError> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(CryptoError::InvalidInput);
        }
        if !self
            .members
            .iter()
            .any(|member| member.identity.device_id == identity.device_id)
        {
            self.members.push(GroupMember {
                display_name,
                identity,
            });
        }
        self.rotate_epoch()?;
        Ok(self.welcome())
    }

    pub fn remove_member(&mut self, device_id: DeviceId) -> Result<(), CryptoError> {
        let original_len = self.members.len();
        self.members
            .retain(|member| member.identity.device_id != device_id);
        if self.members.len() == original_len || self.members.is_empty() {
            return Err(CryptoError::InvalidInput);
        }
        self.rotate_epoch()
    }

    pub fn welcome(&self) -> GroupWelcome {
        GroupWelcome {
            protocol: group_protocol_label(),
            mls_ciphersuite: openmls_ciphersuite_label(),
            group_id: self.group_id,
            display_name: self.display_name.clone(),
            epoch: self.epoch,
            secret: self.secret,
            members: self.members.clone(),
        }
    }

    pub fn encrypt_message(
        &self,
        sender_keys: &DeviceKeyMaterial,
        plain: GroupPlainMessage,
    ) -> Result<GroupWireMessage, CryptoError> {
        let sender = sender_keys.public_identity();
        if !self.members.iter().any(|member| member.identity == sender) {
            return Err(CryptoError::InvalidInput);
        }
        let nonce = random_bytes::<12>();
        let ad = group_associated_data(
            self.group_id,
            self.epoch,
            sender.account_id,
            sender.device_id,
        );
        let ciphertext = encrypt_aead(
            CipherSuite::default(),
            &self.secret,
            &nonce,
            &serde_bytes(&plain)?,
            &ad,
        )?;
        let sender_signature = sign_bytes(
            &sender_keys.device_signing_key(),
            &group_signature_payload(
                self.group_id,
                self.epoch,
                sender.account_id,
                sender.device_id,
                &nonce,
                &ciphertext,
            ),
        );
        Ok(GroupWireMessage {
            protocol: group_protocol_label(),
            mls_ciphersuite: openmls_ciphersuite_label(),
            group_id: self.group_id,
            epoch: self.epoch,
            sender_account_id: sender.account_id,
            sender_device_id: sender.device_id,
            nonce,
            ciphertext,
            sender_signature,
        })
    }

    pub fn decrypt_message(
        &self,
        wire: &GroupWireMessage,
    ) -> Result<GroupPlainMessage, CryptoError> {
        if wire.protocol != group_protocol_label()
            || wire.group_id != self.group_id
            || wire.epoch != self.epoch
        {
            return Err(CryptoError::InvalidInput);
        }
        let sender = self
            .members
            .iter()
            .find(|member| {
                member.identity.account_id == wire.sender_account_id
                    && member.identity.device_id == wire.sender_device_id
            })
            .ok_or(CryptoError::InvalidInput)?;
        verify_signature(
            &sender.identity.device_signing_public,
            &group_signature_payload(
                wire.group_id,
                wire.epoch,
                wire.sender_account_id,
                wire.sender_device_id,
                &wire.nonce,
                &wire.ciphertext,
            ),
            &wire.sender_signature,
        )?;
        let ad = group_associated_data(
            wire.group_id,
            wire.epoch,
            wire.sender_account_id,
            wire.sender_device_id,
        );
        let plain = decrypt_aead(
            CipherSuite::default(),
            &self.secret,
            &wire.nonce,
            &wire.ciphertext,
            &ad,
        )?;
        serde_json::from_slice(&plain).map_err(|_| CryptoError::InvalidInput)
    }

    pub fn transport_envelope(&self, wire: GroupWireMessage) -> GroupTransportEnvelope {
        GroupTransportEnvelope {
            kind: GROUP_TRANSPORT_KIND.to_string(),
            group_id: self.group_id,
            wire,
        }
    }

    fn rotate_epoch(&mut self) -> Result<(), CryptoError> {
        let next_epoch = self.epoch + 1;
        let secret = hkdf_expand(
            b"secure-chat-v0.2/group-epoch",
            &self.secret,
            &[
                self.group_id.as_bytes().as_slice(),
                &next_epoch.to_be_bytes(),
                &serde_bytes(&self.members)?,
            ]
            .concat(),
            32,
        )?;
        self.secret = secret
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::InvalidInput)?;
        self.epoch = next_epoch;
        Ok(())
    }
}

pub fn encode_group_control(control: &GroupControlMessage) -> Result<String, CryptoError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    Ok(format!(
        "{GROUP_CONTROL_PREFIX}{}",
        STANDARD.encode(serde_bytes(control)?)
    ))
}

pub fn decode_group_control(body: &str) -> Result<Option<GroupControlMessage>, CryptoError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let Some(encoded) = body.strip_prefix(GROUP_CONTROL_PREFIX) else {
        return Ok(None);
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| CryptoError::InvalidInput)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| CryptoError::InvalidInput)
}

pub fn group_protocol_label() -> String {
    "RFC9420-MLS/openmls".to_string()
}

pub fn openmls_ciphersuite_label() -> String {
    format!(
        "{:?}",
        Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519
    )
}

fn group_associated_data(
    group_id: Uuid,
    epoch: u64,
    sender_account_id: AccountId,
    sender_device_id: DeviceId,
) -> Vec<u8> {
    [
        b"secure-chat-v0.2/group-message".as_slice(),
        group_id.as_bytes().as_slice(),
        &epoch.to_be_bytes(),
        sender_account_id.as_bytes(),
        sender_device_id.as_bytes(),
    ]
    .concat()
}

fn group_signature_payload(
    group_id: Uuid,
    epoch: u64,
    sender_account_id: AccountId,
    sender_device_id: DeviceId,
    nonce: &Nonce12,
    ciphertext: &[u8],
) -> Vec<u8> {
    [
        b"secure-chat-v0.2/group-message-sender-signature".as_slice(),
        group_id.as_bytes().as_slice(),
        &epoch.to_be_bytes(),
        sender_account_id.as_bytes(),
        sender_device_id.as_bytes(),
        nonce.as_slice(),
        ciphertext,
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeviceKeyMaterial;

    #[test]
    fn group_round_trip_and_removed_member_cannot_decrypt_next_epoch() {
        let alice = DeviceKeyMaterial::generate(16);
        let bob = DeviceKeyMaterial::generate(16);
        let carol = DeviceKeyMaterial::generate(16);
        let mut group = GroupState::create("Weekend", "Alice", alice.public_identity()).unwrap();
        let bob_welcome = group
            .add_member("Bob", bob.public_identity())
            .expect("bob welcome");
        let bob_group = GroupState::from_welcome(bob_welcome).unwrap();

        let wire = group
            .encrypt_message(
                &alice,
                GroupPlainMessage {
                    sent_at_unix: 1,
                    body: "hi group".to_string(),
                },
            )
            .unwrap();
        assert_eq!(bob_group.decrypt_message(&wire).unwrap().body, "hi group");

        let stale_bob_group = bob_group.clone();
        group.add_member("Carol", carol.public_identity()).unwrap();
        group.remove_member(bob.device_id).unwrap();
        let wire = group
            .encrypt_message(
                &alice,
                GroupPlainMessage {
                    sent_at_unix: 2,
                    body: "after remove".to_string(),
                },
            )
            .unwrap();
        assert!(stale_bob_group.decrypt_message(&wire).is_err());
    }

    #[test]
    fn group_member_cannot_impersonate_another_member() {
        let alice = DeviceKeyMaterial::generate(16);
        let bob = DeviceKeyMaterial::generate(16);
        let carol = DeviceKeyMaterial::generate(16);
        let mut group = GroupState::create("Weekend", "Alice", alice.public_identity()).unwrap();
        group.add_member("Bob", bob.public_identity()).unwrap();
        group.add_member("Carol", carol.public_identity()).unwrap();
        let bob_group = GroupState::from_welcome(group.welcome()).unwrap();
        let carol_group = GroupState::from_welcome(group.welcome()).unwrap();

        let claimed_sender = alice.public_identity();
        let nonce = random_bytes::<12>();
        let plain = GroupPlainMessage {
            sent_at_unix: 3,
            body: "forged by Bob".to_string(),
        };
        let ad = group_associated_data(
            bob_group.group_id,
            bob_group.epoch,
            claimed_sender.account_id,
            claimed_sender.device_id,
        );
        let ciphertext = encrypt_aead(
            CipherSuite::default(),
            &bob_group.secret,
            &nonce,
            &serde_bytes(&plain).unwrap(),
            &ad,
        )
        .unwrap();
        let attacker_signature = sign_bytes(
            &bob.device_signing_key(),
            &group_signature_payload(
                bob_group.group_id,
                bob_group.epoch,
                claimed_sender.account_id,
                claimed_sender.device_id,
                &nonce,
                &ciphertext,
            ),
        );
        let forged_wire = GroupWireMessage {
            protocol: group_protocol_label(),
            mls_ciphersuite: openmls_ciphersuite_label(),
            group_id: bob_group.group_id,
            epoch: bob_group.epoch,
            sender_account_id: claimed_sender.account_id,
            sender_device_id: claimed_sender.device_id,
            nonce,
            ciphertext,
            sender_signature: attacker_signature,
        };

        assert!(carol_group.decrypt_message(&forged_wire).is_err());
    }
}
