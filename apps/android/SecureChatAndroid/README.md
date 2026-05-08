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

The script builds `libsecure_chat_ffi.so` for `arm64-v8a` and `x86_64`, copies them into `app/src/main/jniLibs`, then runs the Android Gradle build.

The app stores Rust runtime secrets in Android app-private no-backup storage and disables Android cloud backup in the manifest.
