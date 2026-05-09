# SecureChat Android

Native Android client for SecureChat. It uses the same Rust E2EE core as macOS and iOS through JNI.

## Features

- Anonymous login, contacts, invite import/copy, one-to-one chat, group chat, temporary sessions, relay settings, background polling, and message status display.
- Contact nickname editing and local strong delete for contacts, one-to-one history, ratchet state, and unfinished attachment state.
- Text with Unicode emoji, image/file attachments, locally imported sticker images/GIFs, and burn-after-reading messages.
- A dedicated chat `LazyColumn` follows new messages only when the user is already near the bottom; history review is not interrupted and shows a lightweight new-message button.

## Build

Requirements:

- Android Studio SDK and NDK installed
- Rust with `rustup`
- Java 17+

From the repository root:

```bash
./script/build_android.sh debug
```

Release build:

```bash
./script/build_android.sh release
```

The script builds `libsecure_chat_ffi.so` for `arm64-v8a`, `armeabi-v7a`,
`x86`, and `x86_64`, copies them into `app/src/main/jniLibs`, verifies the
pinned Gradle distribution checksum when it has to download Gradle, then runs
the Android Gradle build.

The app stores Rust runtime secrets in Android app-private no-backup storage and
excludes the app-private files, databases, shared preferences, and external
state from Android cloud backup and device-transfer extraction rules.

The public v0.2.5 release asset is named `SecureChatAndroid-0.2.5.apk`. It is a
normal installable APK; the project still requires a real release keystore for
official store-style distribution.
