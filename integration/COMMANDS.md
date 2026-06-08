# PocketLens Verification Commands

Use these commands from the repository root.

## Contract Fixtures

```sh
python3 integration/verify_contract_fixtures.py
```

## Android

Install and configure the Android SDK per the
[Android Studio](https://developer.android.com/studio) or
[command-line tools](https://developer.android.com/studio#command-line-tools-only)
documentation. Set `ANDROID_HOME` / `ANDROID_SDK_ROOT` as described in
[Configure SDK environment variables](https://developer.android.com/tools/variables),
or let Android Studio generate `android/local.properties` (gitignored).

```sh
cd android
./gradlew test assembleDebug
```

Install the debug APK:

```sh
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

Launch it:

```sh
adb shell monkey -p com.pocketlens.android -c android.intent.category.LAUNCHER 1
```

## Linux

```sh
cd linux
cargo fmt --check
cargo check
cargo test
cargo run -p pocketlens-receiver -- --diagnose
```
