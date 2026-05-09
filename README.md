# SecureChat

SecureChat is a self-hosted E2EE chat prototype with a Rust security core, a
Rust relay, a native macOS SwiftUI desktop client, and a native iOS SwiftUI
client that shares the same Rust FFI runtime and relay protocol.

It includes:

- X3DH-style asynchronous setup and a Double Ratchet message layer
- RFC 9420/OpenMLS-oriented group state, Welcome control messages, epoch
  rotation, encrypted group fan-out, and relay MLS KeyPackage publish/claim APIs
- anonymous accounts, per-device identity keys, invite links, and safety numbers
- ChaCha20-Poly1305 message encryption by default, with an AES-256-GCM suite enum
- HTTPS and QUIC relay transports with a shared encrypted-frame API
- Ed25519-signed relay requests for device registration, send, drain, and read
  receipt commands, plus private MLS and APNs push commands
- SQLite relay persistence for public pre-key bundles, offline ciphertext queues,
  delivery/read receipts, MLS KeyPackages, and APNs device tokens
- macOS Keychain storage for identity keys and a local storage key
- SQLite desktop storage for contacts, encrypted ratchet sessions, encrypted
  message bodies, and cached relay ciphertext
- SwiftUI login, contacts, invite import/copy, chat transcript, relay settings,
  group chats, background polling, APNs registration, notifications, and
  sent/delivered/read state display on macOS and iOS

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

The generated DMG is written to `dist/SecureChatMac-0.2.4.dmg`. It is ad-hoc
signed for local testing by default. For release gates, set
`SECURE_CHAT_MACOS_SIGN_IDENTITY` to a Developer ID identity and
`SECURE_CHAT_RELEASE_STRICT=1`; the script then rejects ad-hoc signing and runs
Gatekeeper assessment.

Build an Android release APK:

```bash
./script/build_android.sh release
```

Set `SECURE_CHAT_ANDROID_KEYSTORE`,
`SECURE_CHAT_ANDROID_KEYSTORE_PASSWORD`, `SECURE_CHAT_ANDROID_KEY_ALIAS`, and
`SECURE_CHAT_ANDROID_KEY_PASSWORD` to produce a signed release APK. Set
`SECURE_CHAT_REQUIRE_RELEASE_SIGNING=1` in release automation so unsigned or
debug-signed APKs cannot be shipped. If the script has to download Gradle, it
verifies the pinned SHA-256 checksum before unzipping it.

Run the Rust test suite:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo test
```

Run a local relay-backed E2EE delivery smoke:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p secure-chat-client --bin secure-chat-smoke
```

Run the group/APNs-token smoke:

```bash
SECURE_CHAT_SMOKE_MODE=group PATH="$HOME/.cargo/bin:$PATH" \
  cargo run -p secure-chat-client --bin secure-chat-smoke
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

Optional Apple Push provider configuration for production relay hosts:

```bash
SECURE_CHAT_APNS_TEAM_ID=TEAMID1234 \
SECURE_CHAT_APNS_KEY_ID=KEYID12345 \
SECURE_CHAT_APNS_PRIVATE_KEY_PATH=/etc/secure-chat/apns/AuthKey_KEYID12345.p8 \
SECURE_CHAT_APNS_TOPIC_IOS=com.example.securechat \
SECURE_CHAT_APNS_TOPIC_MACOS=com.example.securechat.mac \
SECURE_CHAT_APNS_ENV=production \
./script/run_relay.sh
```

APNs payloads are generic: they do not include contact names, group names,
message bodies, or ciphertext. If APNs variables are absent or APNs delivery
fails, clients keep using polling.

For server deployment and user-facing setup:

- One-command relay installer for Ubuntu 22.04/24.04 LTS:
  - no domain, use the server public IP: `./deploy/install-relay.sh --email you@example.com`
  - with a domain: `./deploy/install-relay.sh --domain chat.example.com --email you@example.com`
- Server maintenance command after install: `chatrelay`
- Relay installs and `chatrelay update` build with `cargo --locked` and write
  `/etc/secure-chat/build-info.txt` with git revision, `Cargo.lock` hash,
  binary hash, and Rust toolchain versions.
- English relay deployment: [docs/deploy-relay.md](docs/deploy-relay.md)
- 中文公共服务器部署指南：[docs/zh/public-server-deployment.md](docs/zh/public-server-deployment.md)
- 中文客户端安装与首次使用说明：[docs/zh/client-installation.md](docs/zh/client-installation.md)
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

## Group Flow

1. Create a group from the macOS, iOS, or Android client.
2. Add member devices from existing contacts. v0.2.0 treats each device identity
   as a separate member and does not merge multiple devices into one user.
3. The invite/Welcome control message is sent over the existing 1:1 E2EE
   channel, and group messages are encrypted once per group epoch then queued as
   opaque ciphertext for each member device.
4. The relay can publish and claim signed MLS KeyPackages through
   `/v1/mls/key-packages` and `/v1/mls/key-packages/claim`, so the group
   onboarding path can move to full OpenMLS Welcome exchange without changing
   relay auth or storage boundaries.

## Architecture

- `crates/secure-chat-core`: identity keys, pre-key bundles, invite links,
  X3DH-style session setup, Double Ratchet encryption, OpenMLS ciphersuite-bound
  group state, safety numbers, relay API types, and padded transport frames.
- `crates/secure-chat-client`: HTTP(S)/QUIC relay client, in-memory secure device
  runtime, invite-based session creation, encrypted send, drain, receipt, and
  decrypt flow.
- `crates/secure-chat-desktop`: macOS-oriented runtime with Keychain identity
  storage and SQLite persistence for contacts, encrypted sessions, encrypted
  groups, encrypted messages, and remote message IDs.
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
  also supports signed P2P UDP rendezvous, NAT candidate probing, fixed-size
  padding, jitter profiles, and cover-traffic flags.
- Relay API auth: device Ed25519 signatures bind action, request digest,
  timestamp, nonce, account ID, and device ID; the relay rejects unsigned,
  stale, and replayed private commands.
- Groups: v0.2.0 uses the OpenMLS RFC 9420 ciphersuite
  `MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519`, persists per-device
  group membership and epoch secrets locally, rotates the epoch when members
  change, and sends opaque group ciphertext through per-device relay queues.
- Push: APNs device tokens are registered with signed relay requests. The relay
  sends only a generic "New encrypted message" notification and a refresh hint.

## Current Limits

- Group chat is per-device. v0.2.0 does not aggregate multiple devices into a
  single user account and does not include Android FCM.
- P2P NAT traversal has signed UDP rendezvous and direct-path probing, with
  relay fallback for restrictive NATs.
- The macOS and iOS clients register for APNs, but real background delivery
  requires Apple Developer signing, push entitlements, bundle topics, and APNs
  provider secrets configured on the relay.
- Real iPhone/iPad installation requires setting an Apple development team and
  bundle identifier in the iOS Xcode project.
- The relay has durable SQLite queues, but it is not horizontally replicated.
- Current invite links sign the full invite metadata. Older
  unsigned invite links are rejected and should be regenerated.
- The cryptographic design and implementation still need third-party audit
  before public security claims.
