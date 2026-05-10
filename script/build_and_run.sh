#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
APP_NAME="SecureChatMac"
BUNDLE_ID="dev.local.securechat.mac"
MIN_SYSTEM_VERSION="14.0"
VERSION="${SECURE_CHAT_VERSION:-0.2.7}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SWIFT_PACKAGE="$ROOT_DIR/apps/macos/SecureChatMac"
DIST_DIR="$ROOT_DIR/dist"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_FRAMEWORKS="$APP_CONTENTS/Frameworks"
APP_RESOURCES="$APP_CONTENTS/Resources"
APP_BINARY="$APP_MACOS/$APP_NAME"
INFO_PLIST="$APP_CONTENTS/Info.plist"
ICON_FILE="$ROOT_DIR/apps/macos/SecureChatMac/Resources/SecureChatMac.icns"
CARGO_BIN="${CARGO:-$HOME/.cargo/bin/cargo}"

export PATH="$HOME/.cargo/bin:$PATH"

pkill -x "$APP_NAME" >/dev/null 2>&1 || true

"$CARGO_BIN" build -p secure-chat-ffi

swift build \
  --package-path "$SWIFT_PACKAGE" \
  -Xlinker -L -Xlinker "$ROOT_DIR/target/debug" \
  -Xlinker -rpath -Xlinker "@executable_path/../Frameworks"

BUILD_BIN_DIR="$(swift build --package-path "$SWIFT_PACKAGE" --show-bin-path)"
BUILD_BINARY="$BUILD_BIN_DIR/$APP_NAME"
RUST_DYLIB="$ROOT_DIR/target/debug/libsecure_chat_ffi.dylib"

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_MACOS" "$APP_FRAMEWORKS" "$APP_RESOURCES"
cp "$BUILD_BINARY" "$APP_BINARY"
cp "$RUST_DYLIB" "$APP_FRAMEWORKS/"
if [[ -f "$ICON_FILE" ]]; then
  cp "$ICON_FILE" "$APP_RESOURCES/"
fi
chmod +x "$APP_BINARY"
install_name_tool -id "@rpath/libsecure_chat_ffi.dylib" "$APP_FRAMEWORKS/libsecure_chat_ffi.dylib" 2>/dev/null || true
install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP_BINARY" 2>/dev/null || true
while IFS= read -r linked_dylib; do
  install_name_tool -change "$linked_dylib" "@rpath/libsecure_chat_ffi.dylib" "$APP_BINARY" 2>/dev/null || true
done < <(otool -L "$APP_BINARY" | sed -n 's/^[[:space:]]*\(.*libsecure_chat_ffi\.dylib\) (compatibility version.*$/\1/p')

cat >"$INFO_PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$APP_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundleIconFile</key>
  <string>SecureChatMac.icns</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>9</string>
  <key>LSMinimumSystemVersion</key>
  <string>$MIN_SYSTEM_VERSION</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP_FRAMEWORKS/libsecure_chat_ffi.dylib" >/dev/null
codesign --force --sign - "$APP_BINARY" >/dev/null
codesign --force --sign - "$APP_BUNDLE" >/dev/null

open_app() {
  /usr/bin/open -n "$APP_BUNDLE"
}

case "$MODE" in
  run)
    open_app
    ;;
  --debug|debug)
    lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    open_app
    /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
    ;;
  --telemetry|telemetry)
    open_app
    /usr/bin/log stream --info --style compact --predicate "subsystem == \"$BUNDLE_ID\""
    ;;
  --verify|verify)
    open_app
    sleep 2
    pgrep -x "$APP_NAME" >/dev/null
    ;;
  --build-only|build-only)
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify|--build-only]" >&2
    exit 2
    ;;
esac
