#!/usr/bin/env bash
set -euo pipefail

TLS_DIR="${TLS_DIR:-/etc/secure-chat/tls}"
OWNER="${OWNER:-securechat}"
GROUP="${GROUP:-securechat}"
CERT_NAME="${CERT_NAME:-${DOMAIN:-${IP_ADDRESS:-}}}"

if [[ -z "$CERT_NAME" ]]; then
  printf 'set CERT_NAME, DOMAIN, or IP_ADDRESS to the certificate name under /etc/letsencrypt/live\n' >&2
  exit 2
fi

SOURCE_DIR="/etc/letsencrypt/live/$CERT_NAME"

install -d -o "$OWNER" -g "$GROUP" -m 0750 "$TLS_DIR"
install -o "$OWNER" -g "$GROUP" -m 0644 "$SOURCE_DIR/fullchain.pem" "$TLS_DIR/fullchain.pem"
install -o "$OWNER" -g "$GROUP" -m 0600 "$SOURCE_DIR/privkey.pem" "$TLS_DIR/privkey.pem"
