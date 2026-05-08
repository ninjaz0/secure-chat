#!/usr/bin/env bash
set -euo pipefail

SERVICE="secure-chat-relay"
SERVICE_USER="securechat"
INSTALL_DIR="/opt/secure-chat"
CONFIG_DIR="/etc/secure-chat"
TLS_DIR="$CONFIG_DIR/tls"
DATA_DIR="/var/lib/secure-chat"
BACKUP_DIR="/var/backups/secure-chat"
BIN_PATH="$INSTALL_DIR/secure-chat-relay"
MANAGER_PATH="/usr/local/bin/chatrelay"
HTTP_ADDR="127.0.0.1:8787"
HTTPS_ADDR="0.0.0.0:443"
QUIC_ADDR="0.0.0.0:443"
DOMAIN="${DOMAIN:-}"
EMAIL="${EMAIL:-}"
SKIP_CERTBOT=0
NO_UFW=0
STAGING=0
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  ./deploy/install-relay.sh --domain chat.example.com [options]

Supported OS:
  Ubuntu 22.04 LTS or 24.04 LTS

Options:
  --domain NAME       Public relay hostname. Required unless SKIP_CERTBOT=1.
  --email EMAIL       Email for Let's Encrypt registration.
  --skip-certbot      Do not issue/copy certificates. Use existing TLS files.
  --staging           Use Let's Encrypt staging environment.
  --no-ufw            Do not change UFW firewall rules.
  --repo-dir PATH     Repository path to build and later update from.
  -h, --help          Show this help.

After deployment, run:
  chatrelay
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --domain)
      DOMAIN="${2:?missing value for --domain}"
      shift 2
      ;;
    --email)
      EMAIL="${2:?missing value for --email}"
      shift 2
      ;;
    --skip-certbot)
      SKIP_CERTBOT=1
      shift
      ;;
    --staging)
      STAGING=1
      shift
      ;;
    --no-ufw)
      NO_UFW=1
      shift
      ;;
    --repo-dir)
      REPO_DIR="$(cd "${2:?missing value for --repo-dir}" && pwd)"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown option: %s\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$DOMAIN" && "$SKIP_CERTBOT" -eq 0 ]]; then
  printf 'Missing --domain. Example: ./deploy/install-relay.sh --domain chat.example.com\n' >&2
  exit 2
fi

if [[ ! -f "$REPO_DIR/Cargo.toml" || ! -f "$REPO_DIR/deploy/secure-chat-relay.service" ]]; then
  printf 'Repository path is invalid: %s\n' "$REPO_DIR" >&2
  exit 1
fi

need_sudo() {
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

section() {
  printf '\n== %s ==\n' "$1"
}

check_supported_os() {
  section "Checking operating system"
  if [[ ! -r /etc/os-release ]]; then
    printf 'Cannot detect OS: /etc/os-release is missing. This installer supports Ubuntu 22.04/24.04 LTS.\n' >&2
    exit 1
  fi

  # shellcheck disable=SC1091
  source /etc/os-release
  if [[ "${ID:-}" != "ubuntu" ]]; then
    printf 'Unsupported OS: %s. This installer supports Ubuntu 22.04/24.04 LTS.\n' "${PRETTY_NAME:-unknown}" >&2
    exit 1
  fi

  case "${VERSION_ID:-}" in
    22.04|24.04)
      printf 'Detected supported Ubuntu release: %s\n' "${PRETTY_NAME:-Ubuntu $VERSION_ID}"
      ;;
    *)
      printf 'Detected Ubuntu %s. The tested targets are Ubuntu 22.04/24.04 LTS; continuing because apt/systemd/ufw are compatible.\n' "${VERSION_ID:-unknown}"
      ;;
  esac
}

install_packages() {
  section "Installing server packages"
  if command -v apt-get >/dev/null 2>&1; then
    need_sudo apt-get update
    need_sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y \
      build-essential curl git pkg-config libssl-dev ca-certificates \
      certbot ufw sqlite3
  else
    printf 'apt-get not found. Install build-essential, curl, git, pkg-config, libssl-dev, certbot, ufw, sqlite3 manually.\n' >&2
    exit 1
  fi
}

ensure_rust() {
  section "Checking Rust toolchain"
  if command -v cargo >/dev/null 2>&1; then
    cargo --version
    return
  fi

  if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    cargo --version
    return
  fi

  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
  rustup default stable
  cargo --version
}

build_relay() {
  section "Building release relay"
  PATH="$HOME/.cargo/bin:$PATH" cargo build --manifest-path "$REPO_DIR/Cargo.toml" --release -p secure-chat-relay
}

install_files() {
  section "Installing service files"
  need_sudo useradd --system --home "$DATA_DIR" --shell /usr/sbin/nologin "$SERVICE_USER" 2>/dev/null || true
  need_sudo install -d -o "$SERVICE_USER" -g "$SERVICE_USER" -m 0750 "$DATA_DIR"
  need_sudo install -d -m 0755 "$INSTALL_DIR" "$CONFIG_DIR"
  need_sudo install -d -o "$SERVICE_USER" -g "$SERVICE_USER" -m 0750 "$TLS_DIR" "$BACKUP_DIR"
  need_sudo install -m 0755 "$REPO_DIR/target/release/secure-chat-relay" "$BIN_PATH"
  need_sudo install -m 0644 "$REPO_DIR/deploy/secure-chat-relay.service" "/etc/systemd/system/$SERVICE.service"
  need_sudo install -m 0755 "$REPO_DIR/deploy/copy-letsencrypt-certs.sh" "$INSTALL_DIR/copy-letsencrypt-certs.sh"
  need_sudo install -m 0755 "$REPO_DIR/deploy/chatrelay-manager.sh" "$MANAGER_PATH"
}

issue_certificate() {
  if [[ "$SKIP_CERTBOT" -eq 1 ]]; then
    section "Skipping certbot"
    return
  fi

  section "Issuing TLS certificate"
  if [[ "$NO_UFW" -eq 0 ]]; then
    need_sudo ufw allow 80/tcp
  fi

  local certbot_args=(certonly --standalone -d "$DOMAIN" --agree-tos --non-interactive)
  if [[ -n "$EMAIL" ]]; then
    certbot_args+=(--email "$EMAIL")
  else
    certbot_args+=(--register-unsafely-without-email)
  fi
  if [[ "$STAGING" -eq 1 ]]; then
    certbot_args+=(--staging)
  fi

  need_sudo certbot "${certbot_args[@]}"
  need_sudo env DOMAIN="$DOMAIN" TLS_DIR="$TLS_DIR" OWNER="$SERVICE_USER" GROUP="$SERVICE_USER" "$INSTALL_DIR/copy-letsencrypt-certs.sh"
}

write_env() {
  section "Writing relay configuration"
  local tmp_env
  tmp_env="$(mktemp)"
  cat >"$tmp_env" <<EOF
SECURE_CHAT_RELAY_HTTP_ADDR=$HTTP_ADDR
SECURE_CHAT_RELAY_HTTPS_ADDR=$HTTPS_ADDR
SECURE_CHAT_RELAY_QUIC_ADDR=$QUIC_ADDR
SECURE_CHAT_TLS_CERT=$TLS_DIR/fullchain.pem
SECURE_CHAT_TLS_KEY=$TLS_DIR/privkey.pem
SECURE_CHAT_RELAY_DB=$DATA_DIR/relay.sqlite3
RUST_LOG=secure_chat_relay=info,tower_http=warn
EOF
  need_sudo install -m 0640 -o root -g "$SERVICE_USER" "$tmp_env" "$CONFIG_DIR/relay.env"
  rm -f "$tmp_env"

  local tmp_conf
  tmp_conf="$(mktemp)"
  {
    printf 'CHATRELAY_DOMAIN=%q\n' "$DOMAIN"
    printf 'CHATRELAY_REPO_DIR=%q\n' "$REPO_DIR"
    printf 'CHATRELAY_SERVICE=%q\n' "$SERVICE"
    printf 'CHATRELAY_CONFIG_DIR=%q\n' "$CONFIG_DIR"
    printf 'CHATRELAY_ENV_FILE=%q\n' "$CONFIG_DIR/relay.env"
    printf 'CHATRELAY_BIN_PATH=%q\n' "$BIN_PATH"
    printf 'CHATRELAY_BACKUP_DIR=%q\n' "$BACKUP_DIR"
  } >"$tmp_conf"
  need_sudo install -m 0644 "$tmp_conf" "$CONFIG_DIR/deploy.conf"
  rm -f "$tmp_conf"
}

install_cert_hook() {
  if [[ "$SKIP_CERTBOT" -eq 1 || -z "$DOMAIN" ]]; then
    return
  fi

  section "Installing certbot renewal hook"
  need_sudo install -d -m 0755 /etc/letsencrypt/renewal-hooks/deploy
  local hook
  hook="$(mktemp)"
  cat >"$hook" <<EOF
#!/usr/bin/env bash
set -euo pipefail
DOMAIN=$DOMAIN $INSTALL_DIR/copy-letsencrypt-certs.sh
systemctl restart $SERVICE
EOF
  need_sudo install -m 0755 "$hook" "/etc/letsencrypt/renewal-hooks/deploy/$SERVICE"
  rm -f "$hook"
}

configure_firewall() {
  if [[ "$NO_UFW" -eq 1 ]]; then
    section "Skipping UFW configuration"
    return
  fi

  section "Configuring UFW"
  need_sudo ufw allow 443/tcp
  need_sudo ufw allow 443/udp
  need_sudo ufw --force enable
  need_sudo ufw status
}

start_service() {
  section "Starting relay service"
  need_sudo systemctl daemon-reload
  need_sudo systemctl enable --now "$SERVICE"
  need_sudo systemctl status "$SERVICE" --no-pager
}

wait_for_url() {
  local url="$1"
  local curl_insecure="${2:-0}"
  local curl_args=(-fsS)
  if [[ "$curl_insecure" -eq 1 ]]; then
    curl_args+=(-k)
  fi

  for _ in {1..20}; do
    if curl "${curl_args[@]}" "$url"; then
      printf '\n'
      return 0
    fi
    sleep 1
  done

  printf 'Health check failed: %s\n' "$url" >&2
  return 1
}

verify_deploy() {
  section "Verifying local health"
  wait_for_url "http://127.0.0.1:8787/health"

  if [[ -n "$DOMAIN" && "$SKIP_CERTBOT" -eq 0 ]]; then
    section "Verifying public HTTPS health"
    wait_for_url "https://$DOMAIN/health" "$STAGING"
  fi
}

print_done() {
  section "Done"
  printf 'SecureChat relay is installed.\n'
  if [[ -n "$DOMAIN" ]]; then
    printf 'Client HTTPS URL: https://%s\n' "$DOMAIN"
    printf 'Client QUIC URL:  quic://%s:443\n' "$DOMAIN"
  fi
  printf 'Run "chatrelay" to open the management menu.\n'
}

main() {
  check_supported_os
  install_packages
  ensure_rust
  build_relay
  install_files
  issue_certificate
  write_env
  install_cert_hook
  configure_firewall
  start_service
  verify_deploy
  print_done
}

main "$@"
