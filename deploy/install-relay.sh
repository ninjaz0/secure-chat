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
P2P_ADDR="0.0.0.0:3478"
DOMAIN="${DOMAIN:-}"
EMAIL="${EMAIL:-}"
PUBLIC_IP="${PUBLIC_IP:-}"
CERTBOT_CMD="${CERTBOT_CMD:-certbot}"
SKIP_CERTBOT=0
NO_UFW=0
STAGING=0
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CERT_MODE=""
CERT_NAME=""
CLIENT_HTTPS_URL=""
CLIENT_QUIC_URL=""

usage() {
  cat <<'EOF'
Usage:
  ./deploy/install-relay.sh [options]
  ./deploy/install-relay.sh --domain chat.example.com [options]

Supported OS:
  Ubuntu 22.04 LTS or 24.04 LTS

Options:
  --domain NAME       Public relay hostname. If omitted, the installer uses the server public IP.
  --ip-address ADDR   Public IPv4 address to use when there is no domain.
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
    --ip-address|--public-ip)
      PUBLIC_IP="${2:?missing value for --ip-address}"
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

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
    return
  fi
  printf 'No SHA-256 tool found. Install sha256sum or shasum.\n' >&2
  return 1
}

is_tcp_port() {
  local port="$1"
  [[ "$port" =~ ^[0-9]+$ ]] && ((port >= 1 && port <= 65535))
}

is_ipv4() {
  local ip="$1"
  [[ "$ip" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || return 1

  local IFS=.
  local octets
  read -r -a octets <<<"$ip"
  local octet
  for octet in "${octets[@]}"; do
    [[ "$octet" =~ ^[0-9]+$ ]] || return 1
    ((octet >= 0 && octet <= 255)) || return 1
  done
}

is_private_ipv4() {
  local ip="$1"
  local IFS=.
  local a b c d
  read -r a b c d <<<"$ip"
  [[ "$a" == "10" ]] && return 0
  [[ "$a" == "127" ]] && return 0
  [[ "$a" == "169" && "$b" == "254" ]] && return 0
  [[ "$a" == "172" && "$b" -ge 16 && "$b" -le 31 ]] && return 0
  [[ "$a" == "192" && "$b" == "168" ]] && return 0
  [[ "$a" == "100" && "$b" -ge 64 && "$b" -le 127 ]] && return 0
  [[ "$a" == "0" ]] && return 0
  [[ "$a" -ge 224 ]] && return 0
  return 1
}

looks_like_ip() {
  local value="$1"
  is_ipv4 "$value" || [[ "$value" == *:* ]]
}

url_host() {
  local host="$1"
  if [[ "$host" == *:* && "$host" != \[* ]]; then
    printf '[%s]' "$host"
  else
    printf '%s' "$host"
  fi
}

detect_public_ip() {
  local url ip
  for url in \
    "https://api.ipify.org" \
    "https://ifconfig.me/ip" \
    "https://icanhazip.com"; do
    ip="$(curl -fsS4 --max-time 8 "$url" 2>/dev/null | tr -d '[:space:]' || true)"
    if is_ipv4 "$ip" && ! is_private_ipv4 "$ip"; then
      printf '%s' "$ip"
      return 0
    fi
  done
  return 1
}

detect_ssh_ports() {
  local detected=()
  local client_ip client_port server_ip server_port rest
  if [[ -n "${SSH_CONNECTION:-}" ]]; then
    read -r client_ip client_port server_ip server_port rest <<<"$SSH_CONNECTION"
    if is_tcp_port "$server_port"; then
      detected+=("$server_port")
    fi
  fi

  local sshd_cmd=""
  if command -v sshd >/dev/null 2>&1; then
    sshd_cmd="$(command -v sshd)"
  elif [[ -x /usr/sbin/sshd ]]; then
    sshd_cmd="/usr/sbin/sshd"
  fi
  if [[ -n "$sshd_cmd" ]]; then
    local port
    while read -r port; do
      if is_tcp_port "$port"; then
        detected+=("$port")
      fi
    done < <(need_sudo "$sshd_cmd" -T 2>/dev/null | awk '$1 == "port" {print $2}' || true)
  fi

  local config
  for config in /etc/ssh/sshd_config /etc/ssh/sshd_config.d/*.conf; do
    [[ -r "$config" ]] || continue
    local port
    while read -r port; do
      if is_tcp_port "$port"; then
        detected+=("$port")
      fi
    done < <(awk 'tolower($1) == "port" && $2 ~ /^[0-9]+$/ {print $2}' "$config")
  done

  if [[ "${#detected[@]}" -eq 0 ]]; then
    detected+=(22)
  fi

  printf '%s\n' "${detected[@]}" | awk '!seen[$0]++'
}

prepare_endpoint_identity() {
  section "Choosing public relay address"

  if [[ -n "$DOMAIN" ]] && looks_like_ip "$DOMAIN"; then
    printf 'Treating --domain value as an IP address. Prefer --ip-address next time.\n'
    PUBLIC_IP="$DOMAIN"
    DOMAIN=""
  fi

  if [[ -n "$DOMAIN" ]]; then
    CERT_MODE="domain"
    CERT_NAME="$DOMAIN"
  else
    CERT_MODE="ip"
    if [[ -z "$PUBLIC_IP" ]]; then
      PUBLIC_IP="$(detect_public_ip || true)"
    fi
    if [[ -z "$PUBLIC_IP" ]]; then
      printf 'Could not detect a public IPv4 address. Re-run with --ip-address YOUR_PUBLIC_IP.\n' >&2
      exit 2
    fi
    if ! is_ipv4 "$PUBLIC_IP"; then
      printf 'IP-only deployment currently supports public IPv4 addresses. Use a DNS name for IPv6: %s\n' "$PUBLIC_IP" >&2
      exit 2
    fi
    if is_ipv4 "$PUBLIC_IP" && is_private_ipv4 "$PUBLIC_IP"; then
      printf 'Refusing private/non-routable IP address for public TLS: %s\n' "$PUBLIC_IP" >&2
      exit 2
    fi
    CERT_NAME="$PUBLIC_IP"
  fi

  local formatted_host
  formatted_host="$(url_host "$CERT_NAME")"
  CLIENT_HTTPS_URL="https://$formatted_host"
  CLIENT_QUIC_URL="quic://$formatted_host:443"

  if [[ "$CERT_MODE" == "domain" ]]; then
    printf 'Using DNS name: %s\n' "$CERT_NAME"
  else
    printf 'Using public IP address: %s\n' "$CERT_NAME"
    printf 'IP certificates are short-lived; the installer will add an automatic renewal timer.\n'
  fi
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
      certbot snapd ufw sqlite3
  else
    printf 'apt-get not found. Install build-essential, curl, git, pkg-config, libssl-dev, certbot, snapd, ufw, sqlite3 manually.\n' >&2
    exit 1
  fi
}

certbot_supports_ip() {
  "$CERTBOT_CMD" --help all 2>/dev/null | grep -q -- '--ip-address'
}

ensure_certbot_for_ip() {
  if [[ "$CERT_MODE" != "ip" || "$SKIP_CERTBOT" -eq 1 ]]; then
    return
  fi

  section "Checking Certbot IP certificate support"
  if certbot_supports_ip; then
    "$CERTBOT_CMD" --version
    return
  fi

  printf 'Installed Certbot does not support --ip-address; installing current Certbot via snap.\n'
  need_sudo systemctl enable --now snapd.socket
  if ! snap list core >/dev/null 2>&1; then
    need_sudo snap install core
  else
    need_sudo snap refresh core
  fi
  if ! snap list certbot >/dev/null 2>&1; then
    need_sudo snap install --classic certbot
  else
    need_sudo snap refresh certbot
  fi

  CERTBOT_CMD="/snap/bin/certbot"
  if ! certbot_supports_ip; then
    printf 'Certbot still does not support --ip-address. Install Certbot 5.4+ and rerun.\n' >&2
    exit 1
  fi
  "$CERTBOT_CMD" --version
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
  PATH="$HOME/.cargo/bin:$PATH" cargo build --manifest-path "$REPO_DIR/Cargo.toml" --locked --release -p secure-chat-relay
}

write_build_info() {
  local binary="$REPO_DIR/target/release/secure-chat-relay"
  local lock_hash="missing"
  local git_revision="unknown"
  local tmp_info

  if [[ -f "$REPO_DIR/Cargo.lock" ]]; then
    lock_hash="$(sha256_file "$REPO_DIR/Cargo.lock")"
  fi
  if command -v git >/dev/null 2>&1 && [[ -d "$REPO_DIR/.git" ]]; then
    git_revision="$(git -C "$REPO_DIR" rev-parse HEAD 2>/dev/null || printf 'unknown')"
  fi

  tmp_info="$(mktemp)"
  {
    printf 'installed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf 'repo_dir=%s\n' "$REPO_DIR"
    printf 'git_revision=%s\n' "$git_revision"
    printf 'cargo_lock_sha256=%s\n' "$lock_hash"
    printf 'binary_sha256=%s\n' "$(sha256_file "$binary")"
    printf 'cargo_version=%s\n' "$(PATH="$HOME/.cargo/bin:$PATH" cargo --version)"
    printf 'rustc_version=%s\n' "$(PATH="$HOME/.cargo/bin:$PATH" rustc --version)"
    printf 'cargo_build_args=--locked --release -p secure-chat-relay\n'
  } >"$tmp_info"
  need_sudo install -m 0644 "$tmp_info" "$CONFIG_DIR/build-info.txt"
  rm -f "$tmp_info"
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
  write_build_info
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

  local certbot_args=(certonly --standalone --agree-tos --non-interactive)
  if [[ "$CERT_MODE" == "domain" ]]; then
    certbot_args+=(-d "$CERT_NAME")
  else
    certbot_args+=(--preferred-profile shortlived --ip-address "$CERT_NAME")
  fi
  if [[ -n "$EMAIL" ]]; then
    certbot_args+=(--email "$EMAIL")
  else
    certbot_args+=(--register-unsafely-without-email)
  fi
  if [[ "$STAGING" -eq 1 ]]; then
    certbot_args+=(--staging)
  fi

  need_sudo "$CERTBOT_CMD" "${certbot_args[@]}"
  need_sudo env CERT_NAME="$CERT_NAME" TLS_DIR="$TLS_DIR" OWNER="$SERVICE_USER" GROUP="$SERVICE_USER" "$INSTALL_DIR/copy-letsencrypt-certs.sh"
}

write_env() {
  section "Writing relay configuration"
  local tmp_env
  tmp_env="$(mktemp)"
  cat >"$tmp_env" <<EOF
SECURE_CHAT_RELAY_HTTP_ADDR=$HTTP_ADDR
SECURE_CHAT_RELAY_HTTPS_ADDR=$HTTPS_ADDR
SECURE_CHAT_RELAY_QUIC_ADDR=$QUIC_ADDR
SECURE_CHAT_RELAY_P2P_ADDR=$P2P_ADDR
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
    printf 'CHATRELAY_PUBLIC_IP=%q\n' "$PUBLIC_IP"
    printf 'CHATRELAY_CERT_MODE=%q\n' "$CERT_MODE"
    printf 'CHATRELAY_CERT_NAME=%q\n' "$CERT_NAME"
    printf 'CHATRELAY_CLIENT_HTTPS_URL=%q\n' "$CLIENT_HTTPS_URL"
    printf 'CHATRELAY_CLIENT_QUIC_URL=%q\n' "$CLIENT_QUIC_URL"
    printf 'CHATRELAY_CERTBOT_CMD=%q\n' "$CERTBOT_CMD"
    printf 'CHATRELAY_STAGING=%q\n' "$STAGING"
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
  if [[ "$SKIP_CERTBOT" -eq 1 || -z "$CERT_NAME" ]]; then
    return
  fi

  section "Installing certbot renewal hook"
  need_sudo install -d -m 0755 /etc/letsencrypt/renewal-hooks/deploy
  local hook
  hook="$(mktemp)"
  cat >"$hook" <<EOF
#!/usr/bin/env bash
set -euo pipefail
CERT_NAME=$CERT_NAME TLS_DIR=$TLS_DIR OWNER=$SERVICE_USER GROUP=$SERVICE_USER $INSTALL_DIR/copy-letsencrypt-certs.sh
systemctl restart $SERVICE
EOF
  need_sudo install -m 0755 "$hook" "/etc/letsencrypt/renewal-hooks/deploy/$SERVICE"
  rm -f "$hook"
}

install_renewal_timer() {
  if [[ "$SKIP_CERTBOT" -eq 1 ]]; then
    return
  fi

  section "Installing certificate renewal timer"
  local service_file timer_file
  service_file="$(mktemp)"
  timer_file="$(mktemp)"

  cat >"$service_file" <<EOF
[Unit]
Description=Renew SecureChat Relay TLS certificate
Wants=network-online.target
After=network-online.target

[Service]
Type=oneshot
ExecStart=$MANAGER_PATH renew
EOF

  cat >"$timer_file" <<'EOF'
[Unit]
Description=Run SecureChat Relay TLS renewal regularly

[Timer]
OnBootSec=15min
OnUnitActiveSec=12h
RandomizedDelaySec=30min
Persistent=true

[Install]
WantedBy=timers.target
EOF

  need_sudo install -m 0644 "$service_file" "/etc/systemd/system/$SERVICE-cert-renew.service"
  need_sudo install -m 0644 "$timer_file" "/etc/systemd/system/$SERVICE-cert-renew.timer"
  rm -f "$service_file" "$timer_file"
  need_sudo systemctl daemon-reload
  need_sudo systemctl enable --now "$SERVICE-cert-renew.timer"
}

configure_firewall() {
  if [[ "$NO_UFW" -eq 1 ]]; then
    section "Skipping UFW configuration"
    return
  fi

  section "Configuring UFW"
  local ssh_port
  while read -r ssh_port; do
    [[ -n "$ssh_port" ]] || continue
    printf 'Allowing SSH on tcp/%s before enabling UFW.\n' "$ssh_port"
    need_sudo ufw allow "$ssh_port/tcp"
  done < <(detect_ssh_ports)
  need_sudo ufw allow 443/tcp
  need_sudo ufw allow 443/udp
  need_sudo ufw allow 3478/udp
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

  if [[ -n "$CLIENT_HTTPS_URL" && "$SKIP_CERTBOT" -eq 0 ]]; then
    section "Verifying public HTTPS health"
    wait_for_url "$CLIENT_HTTPS_URL/health" "$STAGING"
  fi
}

print_done() {
  section "Done"
  printf 'SecureChat relay is installed.\n'
  printf '\nCopy one of these into the SecureChat client Relay URL field:\n'
  printf '  %s\n' "$CLIENT_HTTPS_URL"
  printf '  %s\n' "$CLIENT_QUIC_URL"
  printf '\nP2P NAT traversal rendezvous is listening on UDP port 3478.\n'
  if [[ "$CERT_MODE" == "ip" && "$SKIP_CERTBOT" -eq 0 ]]; then
    printf '\nNote: IP TLS certificates are short-lived. Automatic renewal is installed with %s-cert-renew.timer.\n' "$SERVICE"
  fi
  if [[ "$STAGING" -eq 1 ]]; then
    printf '\nWarning: --staging issued a test certificate that normal clients do not trust.\n'
    printf 'Re-run without --staging before using the printed HTTPS/QUIC URLs in apps.\n'
  fi
  printf 'Run "chatrelay" to open the management menu.\n'
}

main() {
  check_supported_os
  install_packages
  prepare_endpoint_identity
  ensure_certbot_for_ip
  ensure_rust
  build_relay
  install_files
  issue_certificate
  write_env
  install_cert_hook
  install_renewal_timer
  configure_firewall
  start_service
  verify_deploy
  print_done
}

main "$@"
