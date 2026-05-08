# SecureChat

SecureChat is a self-hosted E2EE chat prototype with a Rust security core, a
Rust relay, a native macOS SwiftUI desktop client, and a native iOS SwiftUI
client that shares the same Rust FFI runtime and relay protocol.

It includes:

- X3DH-style asynchronous setup and a Double Ratchet message layer
- anonymous accounts, per-device identity keys, invite links, and safety numbers
- ChaCha20-Poly1305 message encryption by default, with an AES-256-GCM suite enum
- HTTPS and QUIC relay transports with a shared encrypted-frame API
- Ed25519-signed relay requests for device registration, send, drain, and read
  receipt commands
- SQLite relay persistence for public pre-key bundles, offline ciphertext queues,
  and delivery/read receipts
- macOS Keychain storage for identity keys and a local storage key
- SQLite desktop storage for contacts, encrypted ratchet sessions, encrypted
  message bodies, and cached relay ciphertext
- SwiftUI login, contacts, invite import/copy, chat transcript, relay settings,
  background polling, notifications, and sent/delivered/read state display on
  macOS and iOS

This is a production-deployable prototype, not audited security software. Do not
market it as "absolutely secure" before an external cryptographic and
implementation review.

## Build And Run

Build and launch the macOS app:

```bash
./script/build_and_run.sh --verify
```

Build without launching, useful for CI:

```bash
./script/build_and_run.sh --build-only
```

Build the iOS simulator client and package the Rust FFI static library as an
XCFramework:

```bash
./script/build_ios.sh debug
open apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj
```

The iOS project expects `dist/SecureChatFFI.xcframework`, which the script
generates from the same `secure-chat-ffi` C ABI used by the macOS client.

Regenerate app icons from a 1024px source image:

```bash
./script/generate_app_icons.py /path/to/source-icon.png
```

Create a local macOS release DMG:

```bash
./script/package_macos.sh
```

The generated DMG is written to `dist/SecureChatMac-0.1.0.dmg`. It is ad-hoc
signed for local testing unless you replace the signing step with a Developer ID
certificate and notarization flow.

Run the Rust test suite:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo test
```

Run a local relay-backed E2EE delivery smoke:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p secure-chat-client --bin secure-chat-smoke
```

Start a local HTTP relay:

```bash
./script/run_relay.sh
```

Start HTTPS and QUIC listeners with a certificate:

```bash
SECURE_CHAT_RELAY_HTTP_ADDR=127.0.0.1:8787 \
SECURE_CHAT_RELAY_HTTPS_ADDR=0.0.0.0:443 \
SECURE_CHAT_RELAY_QUIC_ADDR=0.0.0.0:443 \
SECURE_CHAT_TLS_CERT=/etc/secure-chat/tls/fullchain.pem \
SECURE_CHAT_TLS_KEY=/etc/secure-chat/tls/privkey.pem \
SECURE_CHAT_RELAY_DB=/var/lib/secure-chat/relay.sqlite3 \
./script/run_relay.sh
```

For server deployment and user-facing setup:

- One-command relay installer for Ubuntu 22.04/24.04 LTS:
  - no domain, use the server public IP: `./deploy/install-relay.sh --email you@example.com`
  - with a domain: `./deploy/install-relay.sh --domain chat.example.com --email you@example.com`
- Server maintenance command after install: `chatrelay`
- English relay deployment: [docs/deploy-relay.md](docs/deploy-relay.md)
- 中文公共服务器部署指南：[docs/zh/public-server-deployment.md](docs/zh/public-server-deployment.md)
- 中文客户端使用教程：[docs/zh/usage-guide.md](docs/zh/usage-guide.md)
- 中文 iOS 构建与互联教程：[docs/zh/ios-client.md](docs/zh/ios-client.md)
- Production environment example: [deploy/relay.env.example](deploy/relay.env.example)

## Two-User Flow

1. Deploy or start one relay.
2. On every client, including macOS and iOS, set the same relay URL:
   - `https://chat.example.com`
   - or `quic://chat.example.com:443`
3. User A creates an invite link from the macOS or iOS app.
4. User B imports the invite through Add Contact on any supported platform.
5. Compare the safety code or QR through an out-of-band trusted channel.
6. Send messages. The app polls in the background, shows notifications, and
   updates sent/delivered/read states from relay receipts.

The relay receives only public pre-key bundles, opaque ciphertext frames, and
delivery/read receipts. Private relay operations are signed by the owning
device, so another client cannot drain a queue just by guessing a device ID.
Plaintext stays inside the endpoint runtimes.

## Architecture

- `crates/secure-chat-core`: identity keys, pre-key bundles, invite links,
  X3DH-style session setup, Double Ratchet encryption, safety numbers, relay API
  types, and padded transport frames.
- `crates/secure-chat-client`: HTTP(S)/QUIC relay client, in-memory secure device
  runtime, invite-based session creation, encrypted send, drain, receipt, and
  decrypt flow.
- `crates/secure-chat-desktop`: macOS-oriented runtime with Keychain identity
  storage and SQLite persistence for contacts, encrypted sessions, encrypted
  messages, and remote message IDs.
- `crates/secure-chat-relay`: Axum HTTPS relay plus Quinn QUIC relay with shared
  state, SQLite persistence, ciphertext queues, and receipt queues.
- `crates/secure-chat-ffi`: C ABI surface consumed by the SwiftUI app.
- `apps/macos/SecureChatMac`: native macOS SwiftUI client.
- `apps/ios/SecureChatIOS`: native iOS SwiftUI client. It links
  `dist/SecureChatFFI.xcframework` and uses the same JSON FFI commands,
  SQLite schema, Apple Keychain storage, invite format, relay API, and E2EE
  protocol as the macOS client.

## Protocol Snapshot

- Identity: anonymous account ID plus per-device Ed25519 signing keys and X25519
  identity keys.
- Authentication: account signing key signs each device identity; device signing
  key signs the current signed pre-key.
- Session setup: X3DH-style X25519 combinations over identity key, signed
  pre-key, ephemeral key, and optional one-time pre-key.
- Message security: Double Ratchet using X25519 DH ratchet and HKDF-SHA256 chain
  ratchets.
- AEAD: ChaCha20-Poly1305 by default. AES-256-GCM remains a supported suite enum.
- Header protection: message number and content type are AEAD-protected; ratchet
  recovery fields remain authenticated cleartext.
- OOB verification: safety number and QR payload are derived from both sides'
  account/device public-key digests.
- Transport: HTTPS and QUIC carry the same E2EE ciphertext envelopes. The core
  also supports fixed-size padding, jitter profiles, and cover-traffic flags.
- Relay API auth: device Ed25519 signatures bind action, request digest,
  timestamp, nonce, account ID, and device ID; the relay rejects unsigned,
  stale, and replayed private commands.

## Current Limits

- 1:1 chat only. Group chat should use MLS later rather than stretching this
  Double Ratchet design into large groups.
- P2P candidate probing is represented in the transport abstraction but is not
  yet a complete NAT-traversal stack.
- The macOS app uses background polling, not APNs push.
- The iOS app currently polls while running/foregrounded. Production iOS
  background delivery still needs APNs or PushKit-style server integration.
- Real iPhone/iPad installation requires setting an Apple development team and
  bundle identifier in the iOS Xcode project.
- The relay has durable SQLite queues, but it is not horizontally replicated.
- The cryptographic design and implementation still need third-party audit
  before public security claims.
