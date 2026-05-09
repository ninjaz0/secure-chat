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

## Versioned Content Payloads

SecureChat v0.2.5 keeps the ratchet wire format stable and extends the plaintext
JSON carried inside the encrypted body. New messages use a versioned content
payload with these logical kinds:

- `text`: normal text, including Unicode emoji entered by the platform input
  method.
- `image` and `file`: attachment metadata plus encrypted chunk state. The relay
  still only stores opaque ciphertext frames.
- `sticker`: a locally imported image/GIF sent as a lightweight image message;
  sticker packs are local only and are not synchronized as packs.
- `burn`: burn-after-reading text or attachment content with a burn ID.
- `destroy`: a best-effort destruction notice for a previously opened burn
  message.

The UI-facing `body` field remains a compatibility fallback for old text rows.
Receivers that do not understand a newer content kind should display the fallback
text instead of treating the whole message as undecryptable.

## Attachments And Stickers

Attachments are split before relay submission so each encrypted relay message
stays below the relay payload limit. Metadata records include attachment ID,
kind, file name, MIME type, byte size, sha256, local path, transfer state, and
chunk counters.

The desktop runtime currently uses 128 KiB raw file chunks. This leaves room for
base64, JSON, ratchet encryption, transport-frame padding, relay request
signatures, and HTTP body limits before the relay's 1 MiB ciphertext cap.

Image messages render from the reassembled local file. File messages expose the
file name, size, transfer status, and local path. Sticker messages use the same
encrypted attachment path but a compact bubble style. Importing a sticker pack is
a local client operation: selecting a sticker sends that image to the recipient,
so the recipient does not need to own the same pack.

## Burn-After-Reading

Burn-after-reading v1 is "destroy on open". When a receiver opens a burn message,
the runtime replaces the local encrypted body with a destroyed placeholder,
removes associated attachment files when present, and sends an encrypted destroy
notice back to the peer or group members. Destroy notices are delivered through
the normal relay queue and therefore are best effort within relay TTL.

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
- P2P rendezvous: a signed UDP probe endpoint on `3478/udp` records the
  server-reflexive address observed by the relay. Candidate publication and
  lookup are also available over the signed HTTP(S)/QUIC command API.

HTTP(S) and QUIC listeners share the same `AppState`, so a device can register
over HTTPS and drain over QUIC, or the reverse. Transport-layer TLS protects the
network path, but it never replaces the E2EE message layer.

## P2P NAT Traversal Flow

P2P uses the relay only as a rendezvous and fallback server:

1. Each device registers its signed pre-key bundle with the relay.
2. The device sends a signed `P2pProbeRequest` over UDP to the relay rendezvous
   socket. The relay verifies the device signature and records the source
   address it observed as a short-lived server-reflexive candidate.
3. Peers query each other's P2P candidates with signed relay commands.
4. Clients send simultaneous signed UDP punch packets to the candidate
   addresses. Direct packets use `P2pDirectDatagram`, which binds sender,
   receiver, timestamp, nonce, and payload hash to the sender device signing key.
5. If direct P2P succeeds, the encrypted transport frame can use the direct UDP
   path. If it fails or expires, the client keeps using relay-backed delivery.

The relay never accepts unauthenticated candidate updates, and candidates expire
quickly so stale NAT mappings are not reused for long.

## Relay-Backed Delivery Flow

The client SDK uses the relay as an opaque delivery queue:

1. Each device uploads its signed public pre-key bundle.
2. The sender imports an invite, verifies the invite metadata signature and the
   bundle signatures, and creates an X3DH-style initial message.
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
minute timestamp skew window and keeps a per-device nonce replay cache in memory
and in SQLite for persistent relays, so a restart does not reopen the timestamp
window. Public pre-key lookup remains unauthenticated so invite-based session
setup can fetch recipient bundles.

## Relay Persistence

The production relay can run with `SECURE_CHAT_RELAY_DB=/path/to/relay.sqlite3`.
SQLite stores:

- signed public device pre-key bundles
- offline ciphertext queues
- delivery/read receipt queues
- short-lived P2P candidate records

Expired ciphertext rows are deleted during startup and drain. Drained messages
and receipts are removed from SQLite after delivery. The database contains no
plaintext message body and no endpoint identity private keys.

## Local Persistence

The desktop runtime separates long-lived secrets from app records:

- macOS/iOS Keychain stores the device identity key material and a local
  storage key.
- Windows Credential Manager/DPAPI stores the device identity key material and
  local storage key. The app data directory is `%LOCALAPPDATA%\SecureChat`.
- SQLite stores profile metadata, contacts, encrypted Double Ratchet session
  state, encrypted message bodies, attachment metadata, local attachment paths,
  sticker packs/items, burn state, and cached relay ciphertext envelopes.
- Message bodies and session states are encrypted at rest with
  ChaCha20-Poly1305 using the local storage key from Keychain.

The desktop runtime also stores the relay message ID for outgoing messages. It
uses that ID to update local message state when `delivered` and `read` receipts
are drained in the background.

The UI decrypts local message bodies only through the Rust runtime when it
renders the current snapshot.

Deleting a contact is a local strong delete. The runtime removes the contact,
one-to-one messages, ratchet session state, and related incomplete attachment
state. It does not delete history from the other person's devices.
