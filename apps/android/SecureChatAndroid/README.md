# SecureChat Android

Native Android client for SecureChat. It uses the same Rust E2EE core as macOS and iOS through JNI.

## Build

Requirements:

- Android Studio SDK and NDK installed
- Rust with `rustup`
- Java 17+

From the repository root:

```bash
./script/build_android.sh debug
```

The script builds `libsecure_chat_ffi.so` for `arm64-v8a`, `armeabi-v7a`,
`x86`, and `x86_64`, copies them into `app/src/main/jniLibs`, verifies the
pinned Gradle distribution checksum when it has to download Gradle, then runs
the Android Gradle build.

The app stores Rust runtime secrets in Android app-private no-backup storage and
excludes the app-private files, databases, shared preferences, and external
state from Android cloud backup and device-transfer extraction rules.
