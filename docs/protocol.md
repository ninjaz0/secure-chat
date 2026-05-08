# SecureChat Protocol v1

## Identity

Each account is anonymous and identified by a random UUID. Each device has:

- Ed25519 device signing key
- X25519 identity key
- X25519 signed pre-key
- a pool of X25519 one-time pre-keys

The account signing key signs device identity records. The device signing key
signs the signed pre-key. A pre-key bundle is invalid if either signature fails.

## Session Setup

The initiator fetches the recipient's pre-key bundle and verifies it before
creating an initial message. The initial shared secret is derived with
HKDF-SHA256 from these X25519 outputs:

- initiator identity key with recipient signed pre-key
- initiator ephemeral key with recipient identity key
- initiator ephemeral key with recipient signed pre-key
- initiator ephemeral key with recipient one-time pre-key, when present

The transcript hash binds the protocol version, cipher suite, session ID,
initiator identity, initiator ephemeral public key, recipient device ID, selected
pre-key IDs, and recipient bundle hash.

## Double Ratchet

The message layer keeps a root chain, sending chain, receiving chain, and skipped
message-key cache. Each message advances the sending or receiving chain through
HKDF-SHA256 and produces separate body and protected-header keys.

When a new ratchet public key arrives, the receiver:

1. stores skipped keys up to the advertised previous-chain length
2. performs the DH ratchet with the incoming X25519 public key
3. derives a receiving chain and a new sending chain
4. attempts protected-header decryption while advancing up to `MAX_SKIP`

`MAX_SKIP` is 64 in this prototype.

## Message Format

Clear fields:

- protocol version
- cipher suite
- session ID
- sender device ID
- recipient device ID
- current ratchet public key
- previous sending-chain length
- header nonce
- body nonce

Protected header:

- message number
- content type

Body:

- JSON plaintext payload encrypted with AEAD

All clear header fields and protected header contents are authenticated as AEAD
associated data.

## Out-Of-Band Verification

The safety number is derived from a canonical digest of both sides' public device
sets. Adding or removing a device changes the safety number. The UI must treat
identity-key or device-list changes as a verification event rather than silently
upgrading trust.

## Transport

Transport frames preserve a common E2EE payload format across direct and relayed
paths. The current core includes fixed-size padding, jitter configuration, cover
traffic flags, and profiles for QUIC/UDP and WebSocket/TLS fallback.

The relay exposes both HTTP(S) and QUIC command transports:

- HTTP(S): REST endpoints for health, account/device registration, pre-key
  lookup, ciphertext enqueue/drain, and receipt enqueue/drain.
- QUIC: Quinn bidirectional streams carrying the same `RelayCommand` and
  `RelayCommandResponse` JSON command model over TLS 1.3, with ALPN
  `secure-chat-relay/1`.

HTTP(S) and QUIC listeners share the same `AppState`, so a device can register
over HTTPS and drain over QUIC, or the reverse. Transport-layer TLS protects the
network path, but it never replaces the E2EE message layer.

## Relay-Backed Delivery Flow

The client SDK uses the relay as an opaque delivery queue:

1. Each device uploads its signed public pre-key bundle.
2. The sender imports an invite, verifies the bundle signatures, and creates an
   X3DH-style initial message.
3. The sender encrypts the plaintext with its Double Ratchet session.
4. The client wraps `{ initial, wire }` into a relay envelope, then pads it as a
   `TransportFrame`.
5. The relay stores only the serialized transport frame under the recipient
   device queue.
6. The recipient drains its queue, exposes the transport frame payload, accepts
   the initial message if no session exists, and decrypts the wire message.
7. Draining a ciphertext queue creates a `delivered` receipt for the sender.
8. The recipient desktop runtime sends a `read` receipt after decrypting and
   rendering the message.

The relay does not parse the E2EE envelope, protected header, plaintext, or
ratchet state.

## Relay Device Authentication

Mutating and private relay commands are authenticated with per-device Ed25519
request signatures:

- device registration signs the registration body with the registering device
  signing key from the pre-key bundle
- message send signs the send body with the sender device signing key
- message and receipt drains sign the drain body with the receiving device
  signing key
- read receipt send signs the receipt body with the reader device signing key

The signed payload binds the action name, canonical request digest, account ID,
device ID, issued timestamp, and a 128-bit nonce. The relay enforces a five
minute timestamp skew window and keeps an in-memory nonce replay cache per
device. Public pre-key lookup remains unauthenticated so invite-based session
setup can fetch recipient bundles.

## Relay Persistence

The production relay can run with `SECURE_CHAT_RELAY_DB=/path/to/relay.sqlite3`.
SQLite stores:

- signed public device pre-key bundles
- offline ciphertext queues
- delivery/read receipt queues

Expired ciphertext rows are deleted during startup and drain. Drained messages
and receipts are removed from SQLite after delivery. The database contains no
plaintext message body and no endpoint identity private keys.

## Local Persistence

The desktop runtime separates long-lived secrets from app records:

- macOS Keychain stores the device identity key material and a local storage key.
- SQLite stores profile metadata, contacts, encrypted Double Ratchet session
  state, encrypted message bodies, and cached relay ciphertext envelopes.
- Message bodies and session states are encrypted at rest with
  ChaCha20-Poly1305 using the local storage key from Keychain.

The desktop runtime also stores the relay message ID for outgoing messages. It
uses that ID to update local message state when `delivered` and `read` receipts
are drained in the background.

The UI decrypts local message bodies only through the Rust runtime when it
renders the current snapshot.
