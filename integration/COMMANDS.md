# ACamera Verification Commands

Use these commands from the repository root.

## Contract Fixtures

```sh
python3 integration/verify_contract_fixtures.py
```

## Android

The local Android SDK is persisted at:

```text
/home/adel/.local/share/acamera/android-sdk
```

`android/local.properties` points Gradle at that SDK.

```sh
cd android
JAVA_HOME=/opt/android-studio-for-platform/jbr \
ANDROID_HOME=/home/adel/.local/share/acamera/android-sdk \
ANDROID_SDK_ROOT=/home/adel/.local/share/acamera/android-sdk \
./gradlew test assembleDebug
```

Install the debug APK:

```sh
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

Launch it:

```sh
adb shell monkey -p com.acamera.android -c android.intent.category.LAUNCHER 1
```

## Linux

```sh
cd linux
cargo fmt --check
cargo check
cargo test
cargo run -p acamera-receiver -- --diagnose
```

