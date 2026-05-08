#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-debug}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
HEADER_SRC="$ROOT_DIR/crates/secure-chat-ffi/include"
HEADER_DIR="$DIST_DIR/ios-headers"
PROJECT="$ROOT_DIR/apps/ios/SecureChatIOS/SecureChatIOS.xcodeproj"
SCHEME="SecureChatIOS"
CARGO_BIN="${CARGO:-$HOME/.cargo/bin/cargo}"
RUSTUP_BIN="${RUSTUP:-$HOME/.cargo/bin/rustup}"

case "$MODE" in
  debug)
    CARGO_PROFILE_ARG=""
    CARGO_PROFILE_DIR="debug"
    XCODE_CONFIGURATION="Debug"
    ;;
  release|--release)
    CARGO_PROFILE_ARG="--release"
    CARGO_PROFILE_DIR="release"
    XCODE_CONFIGURATION="Release"
    ;;
  *)
    echo "usage: $0 [debug|release]" >&2
    exit 2
    ;;
esac

export PATH="$HOME/.cargo/bin:$PATH"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"

"$RUSTUP_BIN" target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null

cd "$ROOT_DIR"
"$CARGO_BIN" build -p secure-chat-ffi --target aarch64-apple-ios ${CARGO_PROFILE_ARG}
"$CARGO_BIN" build -p secure-chat-ffi --target aarch64-apple-ios-sim ${CARGO_PROFILE_ARG}

rm -rf "$HEADER_DIR" "$DIST_DIR/SecureChatFFI.xcframework"
mkdir -p "$HEADER_DIR"
cp "$HEADER_SRC/secure_chat_ffi.h" "$HEADER_DIR/"
cp "$HEADER_SRC/module.modulemap" "$HEADER_DIR/"

xcodebuild -create-xcframework \
  -library "$ROOT_DIR/target/aarch64-apple-ios/$CARGO_PROFILE_DIR/libsecure_chat_ffi.a" \
  -headers "$HEADER_DIR" \
  -library "$ROOT_DIR/target/aarch64-apple-ios-sim/$CARGO_PROFILE_DIR/libsecure_chat_ffi.a" \
  -headers "$HEADER_DIR" \
  -output "$DIST_DIR/SecureChatFFI.xcframework"

xcodebuild \
  -project "$PROJECT" \
  -scheme "$SCHEME" \
  -configuration "$XCODE_CONFIGURATION" \
  -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath "$DIST_DIR/iOSDerivedData" \
  ARCHS=arm64 \
  ONLY_ACTIVE_ARCH=YES \
  CODE_SIGNING_ALLOWED=NO \
  build
