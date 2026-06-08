# ACamera

Use an Android phone as a Linux camera and microphone for Zoom, Discord, browser
calls, OBS, and normal camera apps.

The Android app sends camera and microphone to this Linux machine. The Linux
receiver creates:

- `ACamera` as a virtual webcam
- `ACamera Microphone` as a virtual microphone

## Easiest Way

Install the Linux launcher:

```sh
sudo apt install -y \
  adb curl ffmpeg libgtk-4-dev libadwaita-1-dev pkg-config \
  gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav \
  pipewire-pulse pulseaudio-utils v4l-utils v4l2loopback-dkms

scripts/install-linux-app.sh
```

Open it:

```sh
~/.local/bin/acamera-gtk
```

Then:

1. Click `Setup Camera`.
2. Click `Setup Microphone`.
3. Click `Start Receiver`.
4. Open the Android app.
5. Tap the discovered receiver, then enter the phone PIN in the Linux app when it appears.
6. Tap `Start`.
7. In Zoom/Discord/etc, choose `ACamera` and `ACamera Microphone`.

Manual fallback still accepts a direct host and port:

```text
Host: 192.168.100.128
Port: 3769
```

## Build and Install the Android App

The Android SDK is kept here:

```text
/home/adel/.local/share/acamera/android-sdk
```

Build:

```sh
cd android
JAVA_HOME=/opt/android-studio-for-platform/jbr \
ANDROID_HOME=/home/adel/.local/share/acamera/android-sdk \
ANDROID_SDK_ROOT=/home/adel/.local/share/acamera/android-sdk \
./gradlew test assembleDebug
```

Install on the connected phone:

```sh
adb devices
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell pm grant com.acamera.android android.permission.CAMERA
adb shell pm grant com.acamera.android android.permission.RECORD_AUDIO
adb shell monkey -p com.acamera.android -c android.intent.category.LAUNCHER 1
```

## Command-Line Use

If you do not want the GTK launcher, run the same pieces manually.

Create the virtual camera:

```sh
scripts/linux-virtual-camera.sh setup
```

Create the virtual microphone:

```sh
scripts/linux-virtual-mic.sh setup
```

Start the receiver:

```sh
cd linux
cargo run -p acamera-receiver -- \
  --control-port 3769 \
  --video-port 5004 \
  --audio-port 5006 \
  --receiver-host 192.168.100.128 \
  --camera-device /dev/video10
```

Now pair and start from the Android app.

## Check It Works

Receiver status:

```sh
curl -sS http://127.0.0.1:3769/status
```

Virtual camera:

```sh
v4l2-ctl -d /dev/video10 --all
```

When streaming is active, it should show:

```text
Card type        : ACamera
Video input      : 0 (loopback: ok)
Width/Height     : 1280/720
Frames per second: 30.000 (30/1)
```

Grab one frame:

```sh
ffmpeg -hide_banner -loglevel error \
  -f v4l2 -i /dev/video10 \
  -frames:v 1 \
  /tmp/acamera-frame.png
```

Open `/tmp/acamera-frame.png` to confirm the picture.

Virtual microphone:

```sh
pactl list short sources | grep acamera
```

## Cleanup

Stop the Android app:

```sh
adb shell am force-stop com.acamera.android
```

Stop receiver processes:

```sh
scripts/acamera-cleanup.sh
```

Remove virtual devices:

```sh
scripts/linux-virtual-mic.sh remove
scripts/linux-virtual-camera.sh remove
```

Uninstall the Android app:

```sh
adb uninstall com.acamera.android
```

Remove the installed Linux launcher:

```sh
rm -f ~/.local/bin/acamera-gtk ~/.local/bin/acamera-receiver
rm -f ~/.local/share/applications/acamera.desktop
```

## Troubleshooting

If the camera is black:

```sh
scripts/linux-virtual-camera.sh status
pgrep -af 'acamera-receiver|gst-launch-1.0'
curl -sS http://127.0.0.1:3769/status
```

Make sure apps select `ACamera`, not another `/dev/video*` device. If the phone
screen says the session is active but the picture is black, try the Android
`Flip` button once.

If it worked and then stopped, clear stale receiver pipelines and start again:

```sh
scripts/acamera-cleanup.sh
scripts/linux-virtual-camera.sh setup
scripts/linux-virtual-mic.sh setup
```

If pairing fails, make sure the Android host field matches this machine's IP:

```sh
ip addr
```

If the GTK app cannot set up the camera, run:

```sh
scripts/linux-virtual-camera.sh setup
```
