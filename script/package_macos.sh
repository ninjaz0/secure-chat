#!/usr/bin/env bash
set -euo pipefail

APP_NAME="SecureChatMac"
BUNDLE_ID="dev.local.securechat.mac"
VERSION="${SECURE_CHAT_VERSION:-0.1.0}"
MIN_SYSTEM_VERSION="14.0"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SWIFT_PACKAGE="$ROOT_DIR/apps/macos/SecureChatMac"
DIST_DIR="$ROOT_DIR/dist"
RELEASE_DIR="$DIST_DIR/release"
APP_BUNDLE="$RELEASE_DIR/$APP_NAME.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_FRAMEWORKS="$APP_CONTENTS/Frameworks"
APP_RESOURCES="$APP_CONTENTS/Resources"
APP_BINARY="$APP_MACOS/$APP_NAME"
INFO_PLIST="$APP_CONTENTS/Info.plist"
ICON_FILE="$ROOT_DIR/apps/macos/SecureChatMac/Resources/SecureChatMac.icns"
DMG_STAGING="$RELEASE_DIR/dmg-staging"
DMG_PATH="$DIST_DIR/$APP_NAME-$VERSION.dmg"
CARGO_BIN="${CARGO:-$HOME/.cargo/bin/cargo}"

export PATH="$HOME/.cargo/bin:$PATH"

"$CARGO_BIN" build -p secure-chat-ffi --release

swift build \
  --package-path "$SWIFT_PACKAGE" \
  -c release \
  -Xlinker -L -Xlinker "$ROOT_DIR/target/release" \
  -Xlinker -rpath -Xlinker "@executable_path/../Frameworks"

BUILD_BIN_DIR="$(swift build --package-path "$SWIFT_PACKAGE" -c release --show-bin-path)"
BUILD_BINARY="$BUILD_BIN_DIR/$APP_NAME"
RUST_DYLIB="$ROOT_DIR/target/release/libsecure_chat_ffi.dylib"

rm -rf "$APP_BUNDLE" "$DMG_STAGING" "$DMG_PATH"
mkdir -p "$APP_MACOS" "$APP_FRAMEWORKS" "$APP_RESOURCES" "$DMG_STAGING"
cp "$BUILD_BINARY" "$APP_BINARY"
cp "$RUST_DYLIB" "$APP_FRAMEWORKS/"
cp "$ICON_FILE" "$APP_RESOURCES/"
chmod +x "$APP_BINARY"

install_name_tool -id "@rpath/libsecure_chat_ffi.dylib" "$APP_FRAMEWORKS/libsecure_chat_ffi.dylib" 2>/dev/null || true
install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP_BINARY" 2>/dev/null || true

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
  <key>CFBundleDisplayName</key>
  <string>SecureChat</string>
  <key>CFBundleIconFile</key>
  <string>SecureChatMac.icns</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>$MIN_SYSTEM_VERSION</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP_FRAMEWORKS/libsecure_chat_ffi.dylib"
codesign --force --sign - "$APP_BINARY"
codesign --force --sign - "$APP_BUNDLE"
codesign --verify --deep --strict "$APP_BUNDLE"

cp -R "$APP_BUNDLE" "$DMG_STAGING/"
ln -s /Applications "$DMG_STAGING/Applications"
hdiutil create \
  -volname "SecureChat" \
  -srcfolder "$DMG_STAGING" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

echo "app=$APP_BUNDLE"
echo "dmg=$DMG_PATH"
