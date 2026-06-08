# PocketLens

Use an Android phone as a Linux camera and microphone for Zoom, Discord, browser
calls, OBS, and normal camera apps.

The Android app sends camera and microphone to this Linux machine. The Linux
receiver creates:

- `PocketLens` as a virtual webcam
- `PocketLens Microphone` as a virtual microphone

> **Disclaimer:** This project was developed with help from [Codex](https://openai.com/codex/). Use it at your own risk.

## Requirements

### Debian / Ubuntu

```sh
sudo apt install -y \
  adb curl ffmpeg pkg-config rustc cargo \
  libgtk-4-dev libadwaita-1-dev \
  gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav \
  pipewire pipewire-pulse pulseaudio-utils v4l-utils v4l2loopback-dkms
```

Verify runtime dependencies:

```sh
pocketlens-receiver --check-deps
```

### Other Linux distributions

Install the equivalent packages for your distro. You need:

| Purpose | What to install |
| --- | --- |
| Build | Rust toolchain (`rustc`, `cargo`), GTK 4 and libadwaita development headers, `pkg-config` |
| Virtual camera | `v4l2loopback` kernel module (often packaged as `v4l2loopback-dkms` or similar), `v4l-utils` |
| Virtual microphone | PipeWire with PulseAudio compatibility (`pipewire`, `pipewire-pulse` or equivalent), `pactl` from `pulseaudio-utils` or your distro's PipeWire tools |
| Media decode | GStreamer 1.0 CLI (`gst-launch-1.0`) plus base, good, bad, ugly, and libav plugin sets |
| Android APK install (optional) | `adb` |
| Troubleshooting (optional) | `curl`, `ffmpeg` |

After installing, run `pocketlens-receiver --check-deps` to confirm everything is present.

## Install

From the repository root:

```sh
./install.sh
```

This builds the Linux receiver and GTK app, installs them under `~/.local`, and
builds the Android debug APK when the Android SDK is configured (see below).

Open the launcher:

```sh
~/.local/bin/pocketlens-gtk
```

Then:

1. Click **Setup Camera**.
2. Click **Setup Microphone**.
3. Click **Start Receiver**.
4. Open the Android app.
5. Tap the discovered receiver, then enter the phone PIN in the Linux app when it appears.
6. Tap **Start**.
7. In Zoom, Discord, etc., choose `PocketLens` and `PocketLens Microphone`.

Manual fallback still accepts a direct host and port if mDNS discovery fails:

```text
Host: <your-lan-ip>
Port: 47650
```

Find your LAN IP with:

```sh
ip -4 addr show scope global
```

## Build and Install the Android App

Install the [Android SDK](https://developer.android.com/studio) (Android Studio or
[command-line tools](https://developer.android.com/studio#command-line-tools-only))
and set `ANDROID_HOME` or `ANDROID_SDK_ROOT` as described in the
[official environment variable guide](https://developer.android.com/tools/variables).
Android Studio writes `android/local.properties` automatically; for CLI-only
setups, follow the
[SDK setup instructions](https://developer.android.com/studio/intro/update) and
point Gradle at your SDK install.

Build:

```sh
cd android
./gradlew test assembleDebug
```

If Gradle cannot find Java, set `JAVA_HOME` to a JDK 17+ install per the
[Android Studio JDK requirements](https://developer.android.com/studio/intro/studio-config#jdk).

Install on a connected phone:

```sh
adb devices
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell pm grant com.pocketlens.android android.permission.CAMERA
adb shell pm grant com.pocketlens.android android.permission.RECORD_AUDIO
adb shell monkey -p com.pocketlens.android -c android.intent.category.LAUNCHER 1
```

Or install from the Linux side after `./install.sh`:

```sh
pocketlens-receiver --install-apk
```

## Command-Line Use

If you do not want the GTK launcher, run the same pieces manually.

Create the virtual camera:

```sh
pocketlens-receiver --setup-camera --device /dev/video10
```

Create the virtual microphone:

```sh
pocketlens-receiver --setup-virtual-mic
```

Start the receiver:

```sh
pocketlens-receiver \
  --control-port 47650 \
  --video-port 5004 \
  --audio-port 5006 \
  --receiver-host <your-lan-ip> \
  --camera-device /dev/video10
```

Now pair and start from the Android app.

See [docs/linux-setup.md](docs/linux-setup.md) for the full control API, mDNS
details, and desktop app device selection notes.

## Check It Works

Receiver status:

```sh
curl -sS http://127.0.0.1:47650/status
```

Virtual camera:

```sh
v4l2-ctl -d /dev/video10 --all
```

When streaming is active, it should show:

```text
Card type        : PocketLens
Video input      : 0 (loopback: ok)
Width/Height     : 1280/720
Frames per second: 30.000 (30/1)
```

Grab one frame:

```sh
ffmpeg -hide_banner -loglevel error \
  -f v4l2 -i /dev/video10 \
  -frames:v 1 \
  /tmp/pocketlens-frame.png
```

Open `/tmp/pocketlens-frame.png` to confirm the picture.

Virtual microphone:

```sh
pactl list short sources | grep pocketlens
```

## Cleanup

Stop the Android app:

```sh
adb shell am force-stop com.pocketlens.android
```

Stop receiver processes:

```sh
pocketlens-receiver --cleanup
```

Remove virtual devices:

```sh
pocketlens-receiver --remove-virtual-mic
pocketlens-receiver --remove-camera
```

Uninstall the Android app:

```sh
adb uninstall com.pocketlens.android
```

Remove the installed Linux launcher:

```sh
rm -f ~/.local/bin/pocketlens-gtk ~/.local/bin/pocketlens-receiver
rm -f ~/.local/share/applications/pocketlens.desktop
```

## Troubleshooting

If the camera is black:

```sh
v4l2-ctl --list-devices
pgrep -af 'pocketlens-receiver|gst-launch-1.0'
curl -sS http://127.0.0.1:47650/status
pocketlens-receiver --diagnose
```

Make sure apps select `PocketLens`, not another `/dev/video*` device. If the phone
screen says the session is active but the picture is black, try the Android
**Flip** button once.

If it worked and then stopped, clear stale receiver pipelines and start again:

```sh
pocketlens-receiver --cleanup
pocketlens-receiver --setup-camera --device /dev/video10
pocketlens-receiver --setup-virtual-mic
```

If pairing fails, make sure the Android host field matches this machine's IP:

```sh
ip -4 addr show scope global
```

If the GTK app cannot set up the camera, run:

```sh
pocketlens-receiver --setup-camera --device /dev/video10
```
