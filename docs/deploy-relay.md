# SecureChat Relay Production Deployment

Chinese public-server deployment guide: [docs/zh/public-server-deployment.md](zh/public-server-deployment.md).

This guide deploys one SecureChat relay with HTTPS, QUIC, SQLite persistence,
systemd supervision, and Let's Encrypt TLS certificates.

The relay still never sees plaintext or E2EE session keys. It stores public
pre-key bundles, opaque ciphertext queues, and delivery/read receipts. Private
relay operations are authenticated with per-device Ed25519 request signatures.

## Server Requirements

- Ubuntu 22.04 or 24.04 LTS
- one public IPv4 address. A DNS name such as `chat.example.com` is optional
- open ports:
  - `80/tcp` for Let's Encrypt certificate issuance and renewal
  - `443/tcp` for HTTPS relay traffic
  - `443/udp` for QUIC relay traffic
  - `3478/udp` for signed P2P rendezvous and NAT candidate discovery
- at least 1 vCPU, 1 GB RAM, and persistent disk for `/var/lib/secure-chat`

## One-Command Systemd Deployment

For a fresh Ubuntu server, the recommended path is the installer script. It
installs packages, installs Rust when needed, builds the relay, creates the
`securechat` service user, issues a Let's Encrypt certificate, writes the
systemd/env files, opens firewall ports, installs a renewal hook, and installs
the server management command:

```bash
git clone https://github.com/ninjaz0/secure-chat.git
cd secure-chat
./deploy/install-relay.sh --email you@example.com
```

If you have a domain, pass it explicitly:

```bash
./deploy/install-relay.sh --domain chat.example.com --email you@example.com
```

Without `--domain`, the installer detects the server public IP address and
requests a Let's Encrypt IP address certificate. IP certificates are
short-lived, so the installer also creates a systemd renewal timer. It also
opens a signed UDP rendezvous listener on `3478/udp` so clients can discover
their public NAT mapping before trying direct P2P.

After deployment, open the maintenance menu on the server with:

```bash
chatrelay
```

Direct maintenance commands are also available:

```bash
chatrelay status
chatrelay logs
chatrelay restart
chatrelay health
chatrelay backup
chatrelay update
chatrelay renew
```

Use these client URLs:

```text
https://203.0.113.10
quic://203.0.113.10:443
```

The installer prints the exact URLs for your server at the end of deployment;
copy either one into the SecureChat client Relay URL field.

If certificates already exist at `/etc/secure-chat/tls/fullchain.pem` and
`/etc/secure-chat/tls/privkey.pem`, use:

```bash
./deploy/install-relay.sh --ip-address 203.0.113.10 --skip-certbot
```

## Manual Systemd Deployment

Install server packages:

```bash
sudo apt update
sudo apt install -y build-essential curl pkg-config libssl-dev certbot ufw
```

Install Rust if the server does not already have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Build the relay from this repository:

```bash
cargo build --release -p secure-chat-relay
```

Create the service user and install directories:

```bash
sudo useradd --system --home /var/lib/secure-chat --shell /usr/sbin/nologin securechat || true
sudo install -d -o securechat -g securechat -m 0750 /var/lib/secure-chat
sudo install -d -m 0755 /opt/secure-chat /etc/secure-chat
sudo install -d -o securechat -g securechat -m 0750 /etc/secure-chat/tls
sudo install -m 0755 target/release/secure-chat-relay /opt/secure-chat/secure-chat-relay
```

Issue a certificate. Replace `chat.example.com` with your real relay hostname:

```bash
sudo ufw allow 80/tcp
sudo certbot certonly --standalone -d chat.example.com
```

Copy the certificate into a directory readable by the `securechat` service user:

```bash
sudo DOMAIN=chat.example.com ./deploy/copy-letsencrypt-certs.sh
```

Create `/etc/secure-chat/relay.env`. You can start from
`deploy/relay.env.example`:

```bash
sudo cp deploy/relay.env.example /etc/secure-chat/relay.env
sudo nano /etc/secure-chat/relay.env
```

Install and start the service:

```bash
sudo cp deploy/secure-chat-relay.service /etc/systemd/system/secure-chat-relay.service
sudo systemctl daemon-reload
sudo systemctl enable --now secure-chat-relay
```

Open production traffic:

```bash
sudo ufw allow 443/tcp
sudo ufw allow 443/udp
sudo ufw allow 3478/udp
sudo ufw enable
```

Verify HTTPS:

```bash
curl -fsS https://chat.example.com/health
```

Verify QUIC from a checkout that can reach the server:

```bash
SECURE_CHAT_SMOKE_RELAY_URL=quic://chat.example.com:443 \
cargo run -p secure-chat-client --bin secure-chat-smoke
```

Configure desktop clients with either:

```text
https://chat.example.com
quic://chat.example.com:443
```

Use `quic://...` for QUIC-first operation. Keep `https://...` as the fallback
URL when testing firewalls that block UDP.

If the relay log says `invalid peer certificate: UnknownIssuer`, the client
rejected the server TLS certificate chain and then aborted the QUIC handshake.
Check the public certificate first:

```bash
curl -v https://chat.example.com/health
openssl s_client -connect chat.example.com:443 -servername chat.example.com -showcerts </dev/null
```

Common fixes:

- If you deployed with `--staging`, issue a real certificate by rerunning the
  installer without `--staging`.
- Make sure `SECURE_CHAT_TLS_CERT` points to `fullchain.pem`, not only the leaf
  `cert.pem`.
- For a private CA or self-signed test certificate, set
  `SECURE_CHAT_QUIC_CA_CERT=/path/to/ca.pem` when running Rust smoke tests. App
  builds should use a publicly trusted certificate unless you intentionally ship
  a private trust anchor.

## Certificate Renewal

Install a renewal hook so Certbot copies fresh certificates and restarts the
relay:

```bash
sudo install -m 0755 deploy/copy-letsencrypt-certs.sh /opt/secure-chat/copy-letsencrypt-certs.sh
sudo tee /etc/letsencrypt/renewal-hooks/deploy/secure-chat-relay >/dev/null <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
DOMAIN=chat.example.com /opt/secure-chat/copy-letsencrypt-certs.sh
systemctl restart secure-chat-relay
EOF
sudo chmod +x /etc/letsencrypt/renewal-hooks/deploy/secure-chat-relay
sudo certbot renew --dry-run
```

## Optional Docker Deployment

Prepare certificate files in `./certs/fullchain.pem` and `./certs/privkey.pem`.
The private key must be readable by UID `10001` inside the container:

```bash
mkdir -p certs data
sudo cp /etc/letsencrypt/live/chat.example.com/fullchain.pem certs/fullchain.pem
sudo cp /etc/letsencrypt/live/chat.example.com/privkey.pem certs/privkey.pem
sudo chown -R 10001:10001 certs data
sudo chmod 0644 certs/fullchain.pem
sudo chmod 0600 certs/privkey.pem
docker compose up -d --build
```

The Compose file exposes:

- `443/tcp` for HTTPS
- `443/udp` for QUIC
- `3478/udp` for signed P2P rendezvous

The unauthenticated HTTP listener is bound to `127.0.0.1:8787` inside the
container and is intentionally not published to the host. Use HTTPS or QUIC
for client traffic.

## Operations

Check service state and logs:

```bash
systemctl status secure-chat-relay
journalctl -u secure-chat-relay -f
```

Back up relay metadata and offline ciphertext queues:

```bash
sudo systemctl stop secure-chat-relay
sudo sqlite3 /var/lib/secure-chat/relay.sqlite3 ".backup '/var/backups/secure-chat-relay.sqlite3'"
sudo systemctl start secure-chat-relay
```

Upgrade:

```bash
git pull
cargo build --release -p secure-chat-relay
sudo install -m 0755 target/release/secure-chat-relay /opt/secure-chat/secure-chat-relay
sudo systemctl restart secure-chat-relay
```

## Security Notes

- Do not claim this is audited security software before an external review.
- Keep the OS, Rust toolchain, and dependencies patched.
- Run one relay hostname per trust boundary; do not mix test and production
  clients on the same SQLite database.
- Back up `/var/lib/secure-chat/relay.sqlite3`. Losing it removes queued offline
  ciphertext and published pre-key bundles, but not user plaintext.
- The relay stores ciphertext metadata needed for delivery. It does not store
  message bodies in plaintext, session keys, local device identity keys, or
  decrypted contact data.
- Device registration, send, drain, and receipt commands must be signed by the
  device key. Public pre-key lookup remains open by design for invite-based
  asynchronous session setup.
