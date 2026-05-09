use crate::crypto::{
    base64_bytes, decrypt_aead, derive_initial_secret, encrypt_aead, kdf_chain, kdf_root,
    random_bytes, serde_bytes, sha256, CipherSuite, CryptoError, Key32, MessageSecrets, Nonce12,
};
use crate::identity::{
    identity_binding_payload, new_x25519_keypair, verify_signature, x25519, DeviceId,
    DeviceKeyMaterial, DevicePreKeyBundle, PreKeyId, PublicDeviceIdentity,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_SKIP: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitialMessage {
    pub version: u8,
    pub suite: CipherSuite,
    pub session_id: Uuid,
    pub initiator_identity: PublicDeviceIdentity,
    pub initiator_ephemeral_public: Key32,
    pub recipient_device_id: DeviceId,
    pub recipient_signed_pre_key_id: PreKeyId,
    pub recipient_one_time_pre_key_id: Option<PreKeyId>,
    pub recipient_bundle_hash: Key32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlainMessage {
    pub sent_at_unix: u64,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireMessage {
    pub version: u8,
    pub suite: CipherSuite,
    pub session_id: Uuid,
    pub sender_device_id: DeviceId,
    pub recipient_device_id: DeviceId,
    pub ratchet_public: Key32,
    pub previous_chain_len: u32,
    pub header_nonce: Nonce12,
    #[serde(with = "base64_bytes")]
    pub encrypted_header: Vec<u8>,
    pub body_nonce: Nonce12,
    #[serde(with = "base64_bytes")]
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ProtectedHeader {
    n: u32,
    content_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct HeaderAd {
    version: u8,
    suite: CipherSuite,
    session_id: Uuid,
    sender_device_id: DeviceId,
    recipient_device_id: DeviceId,
    ratchet_public: Key32,
    previous_chain_len: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BodyAd {
    header: HeaderAd,
    protected_header: ProtectedHeader,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SkippedMessageKey {
    ratchet_public: Key32,
    n: u32,
    secrets: MessageSecrets,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RatchetSession {
    pub session_id: Uuid,
    pub suite: CipherSuite,
    pub local_identity: PublicDeviceIdentity,
    pub remote_identity: PublicDeviceIdentity,
    pub verified: bool,
    dhs_secret: Key32,
    dhs_public: Key32,
    dhr_public: Option<Key32>,
    root_key: Key32,
    cks: Option<Key32>,
    ckr: Option<Key32>,
    ns: u32,
    nr: u32,
    pn: u32,
    skipped: Vec<SkippedMessageKey>,
}

pub fn start_session_as_initiator(
    local: &DeviceKeyMaterial,
    remote_bundle: &DevicePreKeyBundle,
    suite: CipherSuite,
) -> Result<(InitialMessage, RatchetSession), CryptoError> {
    local.public_identity().verify()?;
    remote_bundle.verify()?;
    let (ephemeral_secret, ephemeral_public) = new_x25519_keypair();
    let initial = InitialMessage {
        version: PROTOCOL_VERSION,
        suite,
        session_id: Uuid::new_v4(),
        initiator_identity: local.public_identity(),
        initiator_ephemeral_public: ephemeral_public,
        recipient_device_id: remote_bundle.identity.device_id,
        recipient_signed_pre_key_id: remote_bundle.signed_pre_key_id,
        recipient_one_time_pre_key_id: remote_bundle.one_time_pre_key.as_ref().map(|key| key.id),
        recipient_bundle_hash: remote_bundle.transcript_hash()?,
    };
    let transcript_hash = sha256(&[&serde_bytes(&initial)?]);
    let mut dh_outputs = vec![
        x25519(
            &local.identity_x25519_secret,
            &remote_bundle.signed_pre_key_public,
        )?,
        x25519(
            &ephemeral_secret,
            &remote_bundle.identity.identity_x25519_public,
        )?,
        x25519(&ephemeral_secret, &remote_bundle.signed_pre_key_public)?,
    ];
    if let Some(one_time) = &remote_bundle.one_time_pre_key {
        dh_outputs.push(x25519(&ephemeral_secret, &one_time.public_key)?);
    }
    let sk = derive_initial_secret(&transcript_hash, &dh_outputs)?;
    let (dhs_secret, dhs_public) = new_x25519_keypair();
    let (root_key, cks) = kdf_root(
        &sk,
        &x25519(&dhs_secret, &remote_bundle.signed_pre_key_public)?,
    )?;
    let session = RatchetSession {
        session_id: initial.session_id,
        suite,
        local_identity: local.public_identity(),
        remote_identity: remote_bundle.identity.clone(),
        verified: false,
        dhs_secret,
        dhs_public,
        dhr_public: Some(remote_bundle.signed_pre_key_public),
        root_key,
        cks: Some(cks),
        ckr: None,
        ns: 0,
        nr: 0,
        pn: 0,
        skipped: Vec::new(),
    };
    Ok((initial, session))
}

fn accept_session_as_responder(
    local: &DeviceKeyMaterial,
    initial: &InitialMessage,
) -> Result<RatchetSession, CryptoError> {
    local.public_identity().verify()?;
    initial.initiator_identity.verify()?;
    if initial.version != PROTOCOL_VERSION
        || initial.recipient_device_id != local.device_id
        || initial.recipient_signed_pre_key_id != local.signed_pre_key_id
    {
        return Err(CryptoError::InvalidInput);
    }
    let local_bundle = local.pre_key_bundle();
    if initial.recipient_bundle_hash != local_bundle.transcript_hash()? {
        return Err(CryptoError::InvalidInput);
    }
    let transcript_hash = sha256(&[&serde_bytes(initial)?]);
    let mut dh_outputs = vec![
        x25519(
            &local.signed_pre_key_secret,
            &initial.initiator_identity.identity_x25519_public,
        )?,
        x25519(
            &local.identity_x25519_secret,
            &initial.initiator_ephemeral_public,
        )?,
        x25519(
            &local.signed_pre_key_secret,
            &initial.initiator_ephemeral_public,
        )?,
    ];
    if let Some(id) = initial.recipient_one_time_pre_key_id {
        let one_time_secret = local
            .find_one_time_pre_key_secret(id)
            .ok_or(CryptoError::InvalidInput)?;
        dh_outputs.push(x25519(
            &one_time_secret,
            &initial.initiator_ephemeral_public,
        )?);
    }
    let sk = derive_initial_secret(&transcript_hash, &dh_outputs)?;
    Ok(RatchetSession {
        session_id: initial.session_id,
        suite: initial.suite,
        local_identity: local.public_identity(),
        remote_identity: initial.initiator_identity.clone(),
        verified: false,
        dhs_secret: local.signed_pre_key_secret,
        dhs_public: local.signed_pre_key_public,
        dhr_public: None,
        root_key: sk,
        cks: None,
        ckr: None,
        ns: 0,
        nr: 0,
        pn: 0,
        skipped: Vec::new(),
    })
}

pub fn accept_session_as_responder_consuming_prekey(
    local: &mut DeviceKeyMaterial,
    initial: &InitialMessage,
) -> Result<RatchetSession, CryptoError> {
    let session = accept_session_as_responder(local, initial)?;
    if let Some(id) = initial.recipient_one_time_pre_key_id {
        local
            .consume_one_time_pre_key(id)
            .ok_or(CryptoError::ReplayOrDuplicate)?;
    }
    Ok(session)
}

impl RatchetSession {
    pub fn mark_verified(&mut self) {
        self.verified = true;
    }

    pub fn encrypt(&mut self, plaintext: PlainMessage) -> Result<WireMessage, CryptoError> {
        let chain_key = self.cks.ok_or(CryptoError::MissingChain)?;
        let (next_chain_key, secrets) = kdf_chain(&chain_key)?;
        self.cks = Some(next_chain_key);
        let n = self.ns;
        self.ns = self
            .ns
            .checked_add(1)
            .ok_or(CryptoError::TooManySkippedKeys)?;

        let header_ad = HeaderAd {
            version: PROTOCOL_VERSION,
            suite: self.suite,
            session_id: self.session_id,
            sender_device_id: self.local_identity.device_id,
            recipient_device_id: self.remote_identity.device_id,
            ratchet_public: self.dhs_public,
            previous_chain_len: self.pn,
        };
        let protected_header = ProtectedHeader {
            n,
            content_type: "text/plain+json".to_string(),
        };
        let header_nonce = random_bytes::<12>();
        let body_nonce = random_bytes::<12>();
        let header_ad_bytes = serde_bytes(&header_ad)?;
        let encrypted_header = encrypt_aead(
            self.suite,
            &secrets.header_key,
            &header_nonce,
            &serde_bytes(&protected_header)?,
            &header_ad_bytes,
        )?;
        let body_ad = BodyAd {
            header: header_ad,
            protected_header,
        };
        let ciphertext = encrypt_aead(
            self.suite,
            &secrets.body_key,
            &body_nonce,
            &serde_bytes(&plaintext)?,
            &serde_bytes(&body_ad)?,
        )?;

        Ok(WireMessage {
            version: PROTOCOL_VERSION,
            suite: self.suite,
            session_id: self.session_id,
            sender_device_id: self.local_identity.device_id,
            recipient_device_id: self.remote_identity.device_id,
            ratchet_public: self.dhs_public,
            previous_chain_len: self.pn,
            header_nonce,
            encrypted_header,
            body_nonce,
            ciphertext,
        })
    }

    pub fn decrypt(&mut self, wire: &WireMessage) -> Result<PlainMessage, CryptoError> {
        if wire.version != PROTOCOL_VERSION
            || wire.session_id != self.session_id
            || wire.suite != self.suite
            || wire.sender_device_id != self.remote_identity.device_id
            || wire.recipient_device_id != self.local_identity.device_id
        {
            return Err(CryptoError::InvalidInput);
        }

        if let Some((idx, protected_header, secrets)) = self.try_skipped_message_key(wire)? {
            let plaintext = self.decrypt_body(wire, &protected_header, &secrets)?;
            self.skipped.remove(idx);
            return Ok(plaintext);
        }

        let mut working = self.clone();
        if working.dhr_public != Some(wire.ratchet_public) {
            working.skip_message_keys(wire.previous_chain_len)?;
            working.dh_ratchet(wire.ratchet_public)?;
        }

        let current_ratchet = working.dhr_public.ok_or(CryptoError::MissingChain)?;
        let mut generated = Vec::new();
        for _ in 0..=MAX_SKIP {
            let n = working.nr;
            let chain_key = working.ckr.ok_or(CryptoError::MissingChain)?;
            let (next_chain_key, secrets) = kdf_chain(&chain_key)?;
            working.ckr = Some(next_chain_key);
            working.nr = working
                .nr
                .checked_add(1)
                .ok_or(CryptoError::TooManySkippedKeys)?;

            if let Ok(protected_header) = decrypt_header(wire, &secrets) {
                if protected_header.n != n {
                    return Err(CryptoError::ReplayOrDuplicate);
                }
                for skipped in generated {
                    working.push_skipped(skipped)?;
                }
                let plaintext = working.decrypt_body(wire, &protected_header, &secrets)?;
                *self = working;
                return Ok(plaintext);
            }

            generated.push(SkippedMessageKey {
                ratchet_public: current_ratchet,
                n,
                secrets,
            });
        }
        Err(CryptoError::TooManySkippedKeys)
    }

    fn try_skipped_message_key(
        &self,
        wire: &WireMessage,
    ) -> Result<Option<(usize, ProtectedHeader, MessageSecrets)>, CryptoError> {
        for (idx, skipped) in self.skipped.iter().enumerate() {
            if skipped.ratchet_public != wire.ratchet_public {
                continue;
            }
            if let Ok(protected_header) = decrypt_header(wire, &skipped.secrets) {
                if protected_header.n == skipped.n {
                    return Ok(Some((idx, protected_header, skipped.secrets)));
                }
            }
        }
        Ok(None)
    }

    fn decrypt_body(
        &self,
        wire: &WireMessage,
        protected_header: &ProtectedHeader,
        secrets: &MessageSecrets,
    ) -> Result<PlainMessage, CryptoError> {
        let body_ad = BodyAd {
            header: wire.header_ad(),
            protected_header: protected_header.clone(),
        };
        let plaintext = decrypt_aead(
            self.suite,
            &secrets.body_key,
            &wire.body_nonce,
            &wire.ciphertext,
            &serde_bytes(&body_ad)?,
        )?;
        serde_json::from_slice(&plaintext)
            .map_err(|err| CryptoError::Serialization(err.to_string()))
    }

    fn skip_message_keys(&mut self, until: u32) -> Result<(), CryptoError> {
        if self.nr.saturating_add(MAX_SKIP as u32) < until {
            return Err(CryptoError::TooManySkippedKeys);
        }
        let ratchet_public = match self.dhr_public {
            Some(public) => public,
            None => return Ok(()),
        };
        while self.nr < until {
            let chain_key = self.ckr.ok_or(CryptoError::MissingChain)?;
            let (next_chain_key, secrets) = kdf_chain(&chain_key)?;
            self.ckr = Some(next_chain_key);
            let skipped = SkippedMessageKey {
                ratchet_public,
                n: self.nr,
                secrets,
            };
            self.nr = self
                .nr
                .checked_add(1)
                .ok_or(CryptoError::TooManySkippedKeys)?;
            self.push_skipped(skipped)?;
        }
        Ok(())
    }

    fn dh_ratchet(&mut self, remote_ratchet_public: Key32) -> Result<(), CryptoError> {
        self.pn = self.ns;
        self.ns = 0;
        self.nr = 0;
        self.dhr_public = Some(remote_ratchet_public);

        let dh_recv = x25519(&self.dhs_secret, &remote_ratchet_public)?;
        let (root_key, ckr) = kdf_root(&self.root_key, &dh_recv)?;
        self.root_key = root_key;
        self.ckr = Some(ckr);

        let (new_secret, new_public) = new_x25519_keypair();
        self.dhs_secret = new_secret;
        self.dhs_public = new_public;
        let dh_send = x25519(&self.dhs_secret, &remote_ratchet_public)?;
        let (root_key, cks) = kdf_root(&self.root_key, &dh_send)?;
        self.root_key = root_key;
        self.cks = Some(cks);
        Ok(())
    }

    fn push_skipped(&mut self, skipped: SkippedMessageKey) -> Result<(), CryptoError> {
        if self.skipped.len() >= MAX_SKIP {
            return Err(CryptoError::TooManySkippedKeys);
        }
        self.skipped.push(skipped);
        Ok(())
    }
}

impl WireMessage {
    fn header_ad(&self) -> HeaderAd {
        HeaderAd {
            version: self.version,
            suite: self.suite,
            session_id: self.session_id,
            sender_device_id: self.sender_device_id,
            recipient_device_id: self.recipient_device_id,
            ratchet_public: self.ratchet_public,
            previous_chain_len: self.previous_chain_len,
        }
    }
}

impl PublicDeviceIdentity {
    pub fn verify(&self) -> Result<(), CryptoError> {
        verify_signature(
            &self.account_signing_public,
            &identity_binding_payload(
                self.account_id,
                &self.account_signing_public,
                self.device_id,
                &self.device_signing_public,
                &self.identity_x25519_public,
            ),
            &self.device_cert_signature,
        )?;
        verify_signature(
            &self.device_signing_public,
            &identity_binding_payload(
                self.account_id,
                &self.account_signing_public,
                self.device_id,
                &self.device_signing_public,
                &self.identity_x25519_public,
            ),
            &self.account_device_signature,
        )
    }
}

fn decrypt_header(
    wire: &WireMessage,
    secrets: &MessageSecrets,
) -> Result<ProtectedHeader, CryptoError> {
    let plaintext = decrypt_aead(
        wire.suite,
        &secrets.header_key,
        &wire.header_nonce,
        &wire.encrypted_header,
        &serde_bytes(&wire.header_ad())?,
    )?;
    serde_json::from_slice(&plaintext).map_err(|err| CryptoError::Serialization(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invite::{Invite, InviteMode};
    use crate::safety::safety_number;
    use crate::transport::{
        ObfuscationProfile, P2pDirectDatagram, P2pDirectReplayCache, TransportFrame,
    };

    fn paired_sessions() -> (RatchetSession, RatchetSession) {
        let alice = DeviceKeyMaterial::generate(4);
        let bob = DeviceKeyMaterial::generate(4);
        let bob_bundle = bob.pre_key_bundle();
        let (initial, alice_session) =
            start_session_as_initiator(&alice, &bob_bundle, CipherSuite::ChaCha20Poly1305).unwrap();
        let bob_session = accept_session_as_responder(&bob, &initial).unwrap();
        (alice_session, bob_session)
    }

    #[test]
    fn x3dh_and_double_ratchet_round_trip() {
        let (mut alice_session, mut bob_session) = paired_sessions();
        let wire = alice_session
            .encrypt(PlainMessage {
                sent_at_unix: 1,
                body: "hello bob".to_string(),
            })
            .unwrap();
        let opened = bob_session.decrypt(&wire).unwrap();
        assert_eq!(opened.body, "hello bob");

        let reply = bob_session
            .encrypt(PlainMessage {
                sent_at_unix: 2,
                body: "hello alice".to_string(),
            })
            .unwrap();
        let opened = alice_session.decrypt(&reply).unwrap();
        assert_eq!(opened.body, "hello alice");
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let (mut alice_session, mut bob_session) = paired_sessions();
        let mut wire = alice_session
            .encrypt(PlainMessage {
                sent_at_unix: 1,
                body: "auth me".to_string(),
            })
            .unwrap();
        let last = wire.ciphertext.last_mut().unwrap();
        *last ^= 0x55;
        assert!(bob_session.decrypt(&wire).is_err());
    }

    #[test]
    fn tampered_header_ad_is_rejected() {
        let (mut alice_session, mut bob_session) = paired_sessions();
        let mut wire = alice_session
            .encrypt(PlainMessage {
                sent_at_unix: 1,
                body: "header-bound".to_string(),
            })
            .unwrap();
        wire.previous_chain_len += 1;
        assert!(bob_session.decrypt(&wire).is_err());
    }

    #[test]
    fn out_of_order_messages_are_recovered_with_skipped_keys() {
        let (mut alice_session, mut bob_session) = paired_sessions();
        let first = alice_session
            .encrypt(PlainMessage {
                sent_at_unix: 1,
                body: "first".to_string(),
            })
            .unwrap();
        let second = alice_session
            .encrypt(PlainMessage {
                sent_at_unix: 2,
                body: "second".to_string(),
            })
            .unwrap();

        let opened_second = bob_session.decrypt(&second).unwrap();
        assert_eq!(opened_second.body, "second");
        let opened_first = bob_session.decrypt(&first).unwrap();
        assert_eq!(opened_first.body, "first");
    }

    #[test]
    fn safety_number_changes_when_device_set_changes() {
        let alice = DeviceKeyMaterial::generate(1);
        let bob = DeviceKeyMaterial::generate(1);
        let bob_new_device = DeviceKeyMaterial::generate(1);
        let first = safety_number(&[alice.public_identity()], &[bob.public_identity()]);
        let changed = safety_number(
            &[alice.public_identity()],
            &[bob.public_identity(), bob_new_device.public_identity()],
        );
        assert_ne!(first.number, changed.number);
    }

    #[test]
    fn invite_uri_round_trip_verifies_bundle() {
        let alice = DeviceKeyMaterial::generate(1);
        let invite = Invite::new(
            &alice,
            Some("https://relay.local".to_string()),
            Some(1_900_000_000),
        )
        .unwrap();
        let uri = invite.to_uri().unwrap();
        let decoded = Invite::from_uri(&uri).unwrap();
        decoded.verify().unwrap();
        assert_eq!(decoded.account_id, alice.account_id);
    }

    #[test]
    fn temporary_invite_round_trip_preserves_ephemeral_mode() {
        let alice = DeviceKeyMaterial::generate(1);
        let invite = Invite::temporary(
            &alice,
            Some("https://relay.local".to_string()),
            Some(1_900_000_000),
        )
        .unwrap();
        let uri = invite.to_uri().unwrap();
        let decoded = Invite::from_uri(&uri).unwrap();
        decoded.verify().unwrap();
        assert_eq!(decoded.mode, InviteMode::Temporary);
        assert_eq!(decoded.expires_unix, Some(1_900_000_000));
        assert_eq!(decoded.account_id, alice.account_id);
    }

    #[test]
    fn invite_rejects_account_identity_mismatch() {
        let alice = DeviceKeyMaterial::generate(1);
        let mut invite = Invite::new(&alice, None, None).unwrap();
        invite.account_id = Uuid::new_v4();
        assert!(matches!(invite.verify(), Err(CryptoError::InvalidInput)));
    }

    #[test]
    fn invite_rejects_tampered_metadata() {
        let alice = DeviceKeyMaterial::generate(1);
        let mut invite = Invite::temporary(
            &alice,
            Some("https://relay.local".to_string()),
            Some(1_900_000_000),
        )
        .unwrap();
        invite.mode = InviteMode::Permanent;
        assert!(matches!(invite.verify(), Err(CryptoError::BadSignature)));

        let mut invite = Invite::temporary(
            &alice,
            Some("https://relay.local".to_string()),
            Some(1_900_000_000),
        )
        .unwrap();
        invite.expires_unix = Some(2_000_000_000);
        assert!(matches!(invite.verify(), Err(CryptoError::BadSignature)));

        let mut invite = Invite::temporary(
            &alice,
            Some("https://relay.local".to_string()),
            Some(1_900_000_000),
        )
        .unwrap();
        invite.relay_hint = Some("https://evil.example".to_string());
        assert!(matches!(invite.verify(), Err(CryptoError::BadSignature)));
    }

    #[test]
    fn identity_binding_rejects_reaccounted_device() {
        let bob = DeviceKeyMaterial::generate(1);
        let mut bundle = bob.pre_key_bundle();
        bundle.identity.account_id = Uuid::new_v4();
        assert!(matches!(bundle.verify(), Err(CryptoError::BadSignature)));
    }

    #[test]
    fn local_key_material_can_refresh_legacy_signatures() {
        let mut keys = DeviceKeyMaterial::generate(1);
        keys.account_device_signature = [0u8; 64];
        keys.device_cert_signature = [0u8; 64];
        assert!(keys.public_identity().verify().is_err());
        assert!(keys.ensure_current_signatures().unwrap());
        keys.pre_key_bundle().verify().unwrap();
    }

    #[test]
    fn responder_rejects_low_order_ephemeral_public_key() {
        let alice = DeviceKeyMaterial::generate(1);
        let bob = DeviceKeyMaterial::generate(1);
        let (mut initial, _) = start_session_as_initiator(
            &alice,
            &bob.pre_key_bundle(),
            CipherSuite::ChaCha20Poly1305,
        )
        .unwrap();
        initial.initiator_ephemeral_public = [0u8; 32];
        assert!(matches!(
            accept_session_as_responder(&bob, &initial),
            Err(CryptoError::InvalidInput)
        ));
    }

    #[test]
    fn responder_consumes_one_time_prekey() {
        let alice = DeviceKeyMaterial::generate(1);
        let mut bob = DeviceKeyMaterial::generate(2);
        let first_bundle = bob.pre_key_bundle();
        let first_otk_id = first_bundle.one_time_pre_key.as_ref().unwrap().id;
        let (initial, _) =
            start_session_as_initiator(&alice, &first_bundle, CipherSuite::ChaCha20Poly1305)
                .unwrap();

        accept_session_as_responder_consuming_prekey(&mut bob, &initial).unwrap();

        assert!(bob.find_one_time_pre_key_secret(first_otk_id).is_none());
        assert_ne!(
            bob.pre_key_bundle()
                .one_time_pre_key
                .as_ref()
                .map(|key| key.id),
            Some(first_otk_id)
        );
        assert!(matches!(
            accept_session_as_responder_consuming_prekey(&mut bob, &initial),
            Err(CryptoError::InvalidInput)
        ));
    }

    #[test]
    fn transport_padding_preserves_payload() {
        let profile = ObfuscationProfile::stealth_quic();
        let frame = TransportFrame::protect(b"ciphertext", &profile).unwrap();
        assert_eq!(frame.padded_body.len(), profile.fixed_frame_size);
        assert_eq!(frame.expose().unwrap(), b"ciphertext");
    }

    #[test]
    fn transport_frame_rejects_wrong_version() {
        let profile = ObfuscationProfile::stealth_quic();
        let mut frame = TransportFrame::protect(b"ciphertext", &profile).unwrap();
        frame.version = 0;
        assert!(matches!(frame.expose(), Err(CryptoError::InvalidInput)));
    }

    #[test]
    fn p2p_direct_datagram_replay_is_rejected() {
        let alice = DeviceKeyMaterial::generate(1);
        let bob = DeviceKeyMaterial::generate(1);
        let frame =
            TransportFrame::protect(b"direct", &ObfuscationProfile::stealth_quic()).unwrap();
        let datagram = P2pDirectDatagram::sign(
            &alice,
            &bob.public_identity(),
            1_900_000_000,
            serde_json::to_vec(&frame).unwrap(),
        );
        let mut cache = P2pDirectReplayCache::new();
        datagram
            .verify_fresh(
                &alice.public_identity(),
                &bob.public_identity(),
                1_900_000_001,
                &mut cache,
            )
            .unwrap();
        assert!(matches!(
            datagram.verify_fresh(
                &alice.public_identity(),
                &bob.public_identity(),
                1_900_000_002,
                &mut cache,
            ),
            Err(CryptoError::ReplayOrDuplicate)
        ));
    }
}
