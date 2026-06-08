#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PREFIX=${PREFIX:-"$HOME/.local"}
JAVA_HOME=${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}
ANDROID_SDK_ROOT=${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}

echo "==> Building Linux binaries"
(cd "$ROOT/linux" && cargo build --release)

echo "==> Installing Linux app to $PREFIX"
"$ROOT/linux/target/release/pocketlens-receiver" --install --prefix "$PREFIX"

if [ -x "$ROOT/android/gradlew" ]; then
  if [ -n "$ANDROID_SDK_ROOT" ] && [ -d "$ANDROID_SDK_ROOT" ]; then
    echo "==> Building Android debug APK"
    (
      cd "$ROOT/android"
      JAVA_HOME="$JAVA_HOME" \
      ANDROID_HOME="$ANDROID_SDK_ROOT" \
      ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
      ./gradlew assembleDebug
    )
    echo "==> Android APK: $ROOT/android/app/build/outputs/apk/debug/app-debug.apk"
  elif [ -f "$ROOT/android/local.properties" ]; then
    echo "==> Building Android debug APK (using android/local.properties)"
    (
      cd "$ROOT/android"
      JAVA_HOME="$JAVA_HOME" \
      ./gradlew assembleDebug
    )
    echo "==> Android APK: $ROOT/android/app/build/outputs/apk/debug/app-debug.apk"
  else
    echo "==> Skipping Android build; configure the SDK first:"
    echo "    https://developer.android.com/studio"
    echo "    https://developer.android.com/tools/variables"
  fi
else
  echo "==> Skipping Android build; gradlew not found"
fi

echo "==> Done"
echo "Run Linux UI: $PREFIX/bin/pocketlens-gtk"
