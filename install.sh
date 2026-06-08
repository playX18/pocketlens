#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PREFIX=${PREFIX:-"$HOME/.local"}
JAVA_HOME=${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}
ANDROID_HOME=${ANDROID_HOME:-"$HOME/.local/share/acamera/android-sdk"}
ANDROID_SDK_ROOT=${ANDROID_SDK_ROOT:-"$ANDROID_HOME"}

echo "==> Building Linux binaries"
(cd "$ROOT/linux" && cargo build --release)

echo "==> Installing Linux app to $PREFIX"
"$ROOT/linux/target/release/acamera-receiver" --install --prefix "$PREFIX"

if [ -x "$ROOT/android/gradlew" ] && [ -d "$ANDROID_HOME" ]; then
  echo "==> Building Android debug APK"
  (
    cd "$ROOT/android"
    JAVA_HOME="$JAVA_HOME" \
    ANDROID_HOME="$ANDROID_HOME" \
    ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
    ./gradlew assembleDebug
  )
  echo "==> Android APK: $ROOT/android/app/build/outputs/apk/debug/app-debug.apk"
else
  echo "==> Skipping Android build; SDK or gradlew not found"
fi

echo "==> Done"
echo "Run Linux UI: $PREFIX/bin/acamera-gtk"
