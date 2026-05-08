#!/usr/bin/env bash
set -euo pipefail

: "${DOMAIN:?set DOMAIN to the relay hostname, for example DOMAIN=chat.example.com}"

TLS_DIR="${TLS_DIR:-/etc/secure-chat/tls}"
OWNER="${OWNER:-securechat}"
GROUP="${GROUP:-securechat}"
SOURCE_DIR="/etc/letsencrypt/live/$DOMAIN"

install -d -o "$OWNER" -g "$GROUP" -m 0750 "$TLS_DIR"
install -o "$OWNER" -g "$GROUP" -m 0644 "$SOURCE_DIR/fullchain.pem" "$TLS_DIR/fullchain.pem"
install -o "$OWNER" -g "$GROUP" -m 0600 "$SOURCE_DIR/privkey.pem" "$TLS_DIR/privkey.pem"
