#!/usr/bin/env bash
set -euo pipefail

SERVICE="${CHATRELAY_SERVICE:-secure-chat-relay}"
CONFIG_DIR="${CHATRELAY_CONFIG_DIR:-/etc/secure-chat}"
ENV_FILE="${CHATRELAY_ENV_FILE:-$CONFIG_DIR/relay.env}"
DEPLOY_CONF="${CHATRELAY_DEPLOY_CONF:-$CONFIG_DIR/deploy.conf}"
BACKUP_DIR="${CHATRELAY_BACKUP_DIR:-/var/backups/secure-chat}"
BIN_PATH="${CHATRELAY_BIN_PATH:-/opt/secure-chat/secure-chat-relay}"
REPO_DIR="${CHATRELAY_REPO_DIR:-}"
DOMAIN="${CHATRELAY_DOMAIN:-}"

if [[ -r "$DEPLOY_CONF" ]]; then
  # shellcheck disable=SC1090
  source "$DEPLOY_CONF"
fi

if [[ -r "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  source "$ENV_FILE"
fi

REPO_DIR="${CHATRELAY_REPO_DIR:-$REPO_DIR}"
DOMAIN="${CHATRELAY_DOMAIN:-$DOMAIN}"

need_sudo() {
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

print_header() {
  printf '\n== %s ==\n' "$1"
}

service_status() {
  need_sudo systemctl status "$SERVICE" --no-pager
}

service_logs() {
  need_sudo journalctl -u "$SERVICE" -f
}

service_tail() {
  need_sudo journalctl -u "$SERVICE" -n "${1:-120}" --no-pager
}

service_start() {
  need_sudo systemctl start "$SERVICE"
  service_status
}

service_stop() {
  need_sudo systemctl stop "$SERVICE"
  service_status
}

service_restart() {
  need_sudo systemctl restart "$SERVICE"
  service_status
}

health_check() {
  print_header "Local HTTP health"
  curl -fsS "http://127.0.0.1:8787/health" || true
  printf '\n'

  if [[ -n "${DOMAIN:-}" ]]; then
    print_header "Public HTTPS health"
    curl -fsS "https://$DOMAIN/health" || true
    printf '\n'
    printf 'Client URLs:\n  https://%s\n  quic://%s:443\n' "$DOMAIN" "$DOMAIN"
  else
    printf 'No CHATRELAY_DOMAIN found in %s; skipping public health check.\n' "$DEPLOY_CONF"
  fi
}

show_config() {
  print_header "Deployment config"
  if [[ -r "$DEPLOY_CONF" ]]; then
    sed -n '1,160p' "$DEPLOY_CONF"
  else
    printf 'Missing %s\n' "$DEPLOY_CONF"
  fi

  print_header "Relay env"
  if [[ -r "$ENV_FILE" ]]; then
    sed -n '1,200p' "$ENV_FILE"
  else
    printf 'Missing %s\n' "$ENV_FILE"
  fi
}

edit_config() {
  local editor="${EDITOR:-nano}"
  need_sudo "$editor" "$ENV_FILE"
  service_restart
}

backup_db() {
  local db="${SECURE_CHAT_RELAY_DB:-/var/lib/secure-chat/relay.sqlite3}"
  local stamp
  stamp="$(date +%Y%m%d-%H%M%S)"
  local backup="$BACKUP_DIR/relay-$stamp.sqlite3"

  if [[ ! -f "$db" ]]; then
    printf 'Database not found: %s\n' "$db" >&2
    exit 1
  fi

  need_sudo install -d -m 0750 "$BACKUP_DIR"
  need_sudo sqlite3 "$db" ".backup '$backup'"
  need_sudo chmod 0640 "$backup"
  printf 'Backup written: %s\n' "$backup"
}

renew_cert() {
  if [[ -z "${DOMAIN:-}" ]]; then
    printf 'CHATRELAY_DOMAIN is missing in %s\n' "$DEPLOY_CONF" >&2
    exit 1
  fi

  need_sudo certbot renew
  need_sudo env DOMAIN="$DOMAIN" /opt/secure-chat/copy-letsencrypt-certs.sh
  service_restart
}

update_relay() {
  local repo="${REPO_DIR:-}"
  if [[ -z "$repo" || ! -d "$repo" ]]; then
    printf 'Repository directory is missing. Set CHATRELAY_REPO_DIR in %s\n' "$DEPLOY_CONF" >&2
    exit 1
  fi

  print_header "Updating source"
  if [[ -d "$repo/.git" ]]; then
    git -C "$repo" pull --ff-only
  else
    printf 'No .git directory found at %s; rebuilding current source only.\n' "$repo"
  fi

  print_header "Building relay"
  PATH="$HOME/.cargo/bin:$PATH" cargo build --manifest-path "$repo/Cargo.toml" --release -p secure-chat-relay

  print_header "Installing binary"
  need_sudo install -m 0755 "$repo/target/release/secure-chat-relay" "$BIN_PATH"
  service_restart
}

firewall_status() {
  need_sudo ufw status verbose || true
}

print_info() {
  print_header "SecureChat Relay"
  printf 'Service: %s\n' "$SERVICE"
  printf 'Binary:  %s\n' "$BIN_PATH"
  printf 'Config:  %s\n' "$ENV_FILE"
  printf 'DB:      %s\n' "${SECURE_CHAT_RELAY_DB:-/var/lib/secure-chat/relay.sqlite3}"
  printf 'Repo:    %s\n' "${REPO_DIR:-not configured}"
  if [[ -n "${DOMAIN:-}" ]]; then
    printf 'HTTPS:   https://%s\n' "$DOMAIN"
    printf 'QUIC:    quic://%s:443\n' "$DOMAIN"
  fi
}

usage() {
  cat <<'EOF'
Usage: chatrelay [command]

Commands:
  menu       Open interactive menu
  status     Show systemd status
  logs       Follow relay logs
  tail       Show recent logs
  start      Start relay
  stop       Stop relay
  restart    Restart relay
  health     Check local and public health endpoints
  config     Show deployment and relay config
  edit       Edit /etc/secure-chat/relay.env and restart
  backup     Back up the relay SQLite database
  renew      Renew/copy TLS certificate and restart
  update     git pull, rebuild, install, restart
  firewall   Show UFW status
  info       Show relay URLs and paths
  help       Show this help
EOF
}

menu() {
  while true; do
    print_info
    cat <<'EOF'

1) Status
2) Logs
3) Restart
4) Health check
5) Back up database
6) Update relay
7) Renew certificate
8) Show config
9) Edit relay env
10) Firewall status
0) Exit
EOF
    read -r -p "Choose: " choice
    case "$choice" in
      1) service_status ;;
      2) service_logs ;;
      3) service_restart ;;
      4) health_check ;;
      5) backup_db ;;
      6) update_relay ;;
      7) renew_cert ;;
      8) show_config ;;
      9) edit_config ;;
      10) firewall_status ;;
      0) exit 0 ;;
      *) printf 'Unknown choice: %s\n' "$choice" ;;
    esac
    printf '\n'
    read -r -p "Press Enter to continue..." _
  done
}

main() {
  local command="${1:-menu}"
  case "$command" in
    menu) menu ;;
    status) service_status ;;
    logs) service_logs ;;
    tail) service_tail "${2:-120}" ;;
    start) service_start ;;
    stop) service_stop ;;
    restart) service_restart ;;
    health) health_check ;;
    config) show_config ;;
    edit) edit_config ;;
    backup) backup_db ;;
    renew) renew_cert ;;
    update) update_relay ;;
    firewall) firewall_status ;;
    info) print_info ;;
    help|-h|--help) usage ;;
    *) usage; exit 2 ;;
  esac
}

main "$@"
