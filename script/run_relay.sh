#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export SECURE_CHAT_RELAY_HTTP_ADDR="${SECURE_CHAT_RELAY_HTTP_ADDR:-${SECURE_CHAT_RELAY_ADDR:-127.0.0.1:8787}}"
export SECURE_CHAT_RELAY_P2P_ADDR="${SECURE_CHAT_RELAY_P2P_ADDR:-0.0.0.0:3478}"

cd "$ROOT_DIR"
echo "SecureChat HTTP relay listening on $SECURE_CHAT_RELAY_HTTP_ADDR"
if [[ -n "${SECURE_CHAT_RELAY_HTTPS_ADDR:-}" ]]; then
  echo "SecureChat HTTPS relay listening on $SECURE_CHAT_RELAY_HTTPS_ADDR"
fi
if [[ -n "${SECURE_CHAT_RELAY_QUIC_ADDR:-}" ]]; then
  echo "SecureChat QUIC relay listening on $SECURE_CHAT_RELAY_QUIC_ADDR"
fi
echo "SecureChat P2P rendezvous listening on $SECURE_CHAT_RELAY_P2P_ADDR"
if [[ -n "${SECURE_CHAT_RELAY_DB:-}" ]]; then
  echo "SecureChat relay SQLite DB: $SECURE_CHAT_RELAY_DB"
fi
cargo run -p secure-chat-relay
