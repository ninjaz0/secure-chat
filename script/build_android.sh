#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-debug}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ANDROID_APP_DIR="$ROOT_DIR/apps/android/SecureChatAndroid"
JNI_LIBS_DIR="$ANDROID_APP_DIR/app/src/main/jniLibs"
CARGO_BIN="${CARGO:-$HOME/.cargo/bin/cargo}"
RUSTUP_BIN="${RUSTUP:-$HOME/.cargo/bin/rustup}"
ANDROID_API="${ANDROID_API:-26}"
GRADLE_VERSION="${GRADLE_VERSION:-8.10.2}"

case "$MODE" in
  debug)
    CARGO_PROFILE_ARG=""
    CARGO_PROFILE_DIR="debug"
    GRADLE_TASK="assembleDebug"
    ;;
  release|--release)
    CARGO_PROFILE_ARG="--release"
    CARGO_PROFILE_DIR="release"
    GRADLE_TASK="assembleRelease"
    ;;
  *)
    echo "usage: $0 [debug|release]" >&2
    exit 2
    ;;
esac

export PATH="$HOME/.cargo/bin:$PATH"

find_android_sdk() {
  if [[ -n "${ANDROID_HOME:-}" && -d "$ANDROID_HOME" ]]; then
    echo "$ANDROID_HOME"
    return
  fi
  if [[ -n "${ANDROID_SDK_ROOT:-}" && -d "$ANDROID_SDK_ROOT" ]]; then
    echo "$ANDROID_SDK_ROOT"
    return
  fi
  if [[ -d "$HOME/Library/Android/sdk" ]]; then
    echo "$HOME/Library/Android/sdk"
    return
  fi
  return 1
}

find_android_ndk() {
  if [[ -n "${ANDROID_NDK_HOME:-}" && -d "$ANDROID_NDK_HOME" ]]; then
    echo "$ANDROID_NDK_HOME"
    return
  fi
  if [[ -n "${ANDROID_NDK_ROOT:-}" && -d "$ANDROID_NDK_ROOT" ]]; then
    echo "$ANDROID_NDK_ROOT"
    return
  fi
  local sdk="$1"
  local newest
  newest="$(find "$sdk/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort | tail -1 || true)"
  if [[ -n "$newest" ]]; then
    echo "$newest"
    return
  fi
  return 1
}

find_ndk_toolchain() {
  local ndk="$1"
  local toolchain
  toolchain="$(find "$ndk/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | head -1 || true)"
  if [[ -n "$toolchain" ]]; then
    echo "$toolchain"
    return
  fi
  return 1
}

gradle_cmd() {
  if [[ -n "${GRADLE:-}" ]]; then
    echo "$GRADLE"
    return
  fi
  if command -v gradle >/dev/null 2>&1; then
    command -v gradle
    return
  fi
  local gradle_dir="$ROOT_DIR/dist/gradle/gradle-$GRADLE_VERSION"
  local gradle_zip="$ROOT_DIR/dist/gradle/gradle-$GRADLE_VERSION-bin.zip"
  if [[ ! -x "$gradle_dir/bin/gradle" ]]; then
    mkdir -p "$ROOT_DIR/dist/gradle"
    curl -L "https://services.gradle.org/distributions/gradle-$GRADLE_VERSION-bin.zip" -o "$gradle_zip"
    unzip -q "$gradle_zip" -d "$ROOT_DIR/dist/gradle"
  fi
  echo "$gradle_dir/bin/gradle"
}

SDK_DIR="$(find_android_sdk || true)"
if [[ -z "$SDK_DIR" ]]; then
  echo "Android SDK not found. Install Android Studio or set ANDROID_HOME / ANDROID_SDK_ROOT." >&2
  exit 1
fi

NDK_DIR="$(find_android_ndk "$SDK_DIR" || true)"
if [[ -z "$NDK_DIR" ]]; then
  echo "Android NDK not found. Install it in Android Studio SDK Manager or set ANDROID_NDK_HOME." >&2
  exit 1
fi

TOOLCHAIN_DIR="$(find_ndk_toolchain "$NDK_DIR" || true)"
if [[ -z "$TOOLCHAIN_DIR" ]]; then
  echo "Android NDK LLVM toolchain not found under $NDK_DIR." >&2
  exit 1
fi

TARGETS=("aarch64-linux-android" "x86_64-linux-android")

abi_for_target() {
  case "$1" in
    aarch64-linux-android)
      echo "arm64-v8a"
      ;;
    x86_64-linux-android)
      echo "x86_64"
      ;;
    *)
      echo "unknown Android target: $1" >&2
      return 1
      ;;
  esac
}

"$RUSTUP_BIN" target add "${TARGETS[@]}" >/dev/null

export AR_aarch64_linux_android="$TOOLCHAIN_DIR/bin/llvm-ar"
export CC_aarch64_linux_android="$TOOLCHAIN_DIR/bin/aarch64-linux-android${ANDROID_API}-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN_DIR/bin/aarch64-linux-android${ANDROID_API}-clang"
export AR_x86_64_linux_android="$TOOLCHAIN_DIR/bin/llvm-ar"
export CC_x86_64_linux_android="$TOOLCHAIN_DIR/bin/x86_64-linux-android${ANDROID_API}-clang"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$TOOLCHAIN_DIR/bin/x86_64-linux-android${ANDROID_API}-clang"

rm -rf "$JNI_LIBS_DIR"
mkdir -p "$JNI_LIBS_DIR"

for target in "${TARGETS[@]}"; do
  abi="$(abi_for_target "$target")"
  "$CARGO_BIN" build -p secure-chat-ffi --target "$target" $CARGO_PROFILE_ARG
  mkdir -p "$JNI_LIBS_DIR/$abi"
  cp "$ROOT_DIR/target/$target/$CARGO_PROFILE_DIR/libsecure_chat_ffi.so" "$JNI_LIBS_DIR/$abi/"
done

GRADLE_BIN="$(gradle_cmd)"
(cd "$ANDROID_APP_DIR" && "$GRADLE_BIN" "$GRADLE_TASK")

echo "Android APK built under $ANDROID_APP_DIR/app/build/outputs/apk/$MODE"
