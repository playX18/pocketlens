# ACamera Linux Setup

This is the Linux receiver side for the Android ACamera sender. The receiver is a user-space daemon; it is not a kernel driver.

## Dependencies

Debian/Ubuntu-style packages:

```sh
sudo apt install v4l2loopback-dkms v4l2loopback-utils pipewire pipewire-pulse gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
```

Check all dependencies at once:

```sh
acamera-receiver --check-deps
```

## Quick Start (GTK App)

Launch the GTK dashboard:

```sh
acamera-gtk
```

The app provides a single-page dashboard with:

- **Setup All Devices** — creates the virtual camera (v4l2loopback) and virtual microphone (PipeWire/Pulse)
- **Start/Stop Receiver** — starts the HTTP control server
- **Install APK** — detects connected Android devices via adb and installs the bundled APK
- Collapsible **Settings**, **Log** sections

## Build and Install

Build dependencies (Debian/Ubuntu):

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config
```

From the repository root:

```sh
cd linux
cargo build --release
```

Install to `~/.local`:

```sh
acamera-receiver --install --prefix ~/.local
```

This copies both binaries to `~/.local/bin/`, creates a `.desktop` file, and places the bundled APK in `~/.local/share/acamera/`.

## Manual Setup (Headless)

### Virtual Camera

```sh
acamera-receiver --setup-camera --device /dev/video10
```

Remove it:

```sh
acamera-receiver --remove-camera
```

### Virtual Microphone

```sh
acamera-receiver --setup-virtual-mic
```

This creates:

- Sink for receiver audio: `acamera_sink` / `ACamera Audio Sink`
- Source for desktop apps: `acamera_microphone` / `ACamera Microphone`

Remove:

```sh
acamera-receiver --remove-virtual-mic
```

### Diagnostics

```sh
acamera-receiver --diagnose
```

Prints JSON with `v4l2loopback`, `pipewire`, `pactl`, `gst-launch-1.0`, and `acamera_microphone` readiness. The daemon is allowed to start with missing dependencies, but `/status` reports missing pieces and `/session/start` refuses to start media.

## Run Receiver

```sh
acamera-receiver --receiver-name "Desk" --control-port 47650 --camera-device /dev/video10 --microphone-sink acamera_sink
```

The v1 control API is:

- `GET /status`
- `POST /pair/request`
- `GET /pair/pending`
- `POST /pair/approve`
- `GET /pair/result?pairing_id=...`
- `POST /pair` as a debug/manual fallback
- `POST /session/start`
- `POST /session/stop`
- `WS /session/events?session_token=<token>[&session_id=<session_id>]`

The receiver advertises `_acamera._udp.local` over mDNS while the daemon is running. TXT records use these keys:

- `name`: receiver display name, for example `Desk`
- `version`: protocol version, currently `1`
- `control_port`: HTTP/WebSocket control port, default `47650`
- `capabilities`: comma-separated values, currently `h264,opus,rtp`

The WebSocket event endpoint validates the paired `session_token` before upgrade. When a session is already active, it sends an initial typed `stats` event, then forwards lifecycle `stats`, `warning`, and `error` events emitted by media start/stop/failure paths.

Media is negotiated as encrypted RTP/UDP H.264 video on port `5004` and encrypted Opus audio on port `5006` by default. Override those with `--video-port` and `--audio-port`.

On session start, the daemon owns two `gst-launch-1.0` processes:

- Video: `udpsrc` receives RTP/H.264, decodes it, converts raw video, and writes to `v4l2sink device=/dev/video10` unless `--camera-device` points elsewhere.
- Audio: `udpsrc` receives RTP/Opus, decodes it, converts/resamples audio, and writes to `pulsesink device=acamera_sink client-name="ACamera Microphone"`.

The `pulsesink` path relies on `pipewire-pulse`. The helper creates a null sink plus a remapped source so Zoom, Discord, and browsers see a stable microphone input instead of a transient playback/client node. If you skip virtual mic setup, `/status` will report the microphone as not ready and `/session/start` will refuse media.

## Cleanup

Kill all stale receiver and GStreamer processes:

```sh
acamera-receiver --cleanup
```

## Install APK to Android Device

Install the bundled APK to a connected Android device via adb:

```sh
acamera-receiver --install-apk
```

Requirements:

- `adb` installed (`sudo apt install adb`)
- USB debugging enabled on the Android device (Settings → Developer Options)
- Device connected via USB

## Manual Device Verification

Virtual camera:

```sh
v4l2-ctl --list-devices
```

Virtual microphone:

```sh
pactl list short sources
```

Desktop apps should eventually show:

- Camera: `ACamera`
- Microphone: `ACamera Microphone`

If `pactl list short sources` shows `acamera_microphone`, the source is present. Some desktops display it as `ACamera Microphone`; others show the technical source name.

## Zoom Device Selection

1. Start the Linux receiver and start streaming from Android.
2. Open Zoom Settings.
3. In Video, choose camera `ACamera`.
4. In Audio, choose microphone `ACamera Microphone`. If Zoom shows technical names, choose `acamera_microphone`.
5. Use Test Mic. The input meter should move when audio arrives from the Android device.

If Zoom only shows `Monitor of ACamera Audio Sink`, select that as a fallback. It is the monitor source feeding the remapped microphone.

## Discord Device Selection

1. Start the Linux receiver and start streaming from Android.
2. Open User Settings -> Voice & Video.
3. Set Input Device to `ACamera Microphone` or `acamera_microphone`.
4. Set Camera to `ACamera`.
5. Use Mic Test and Video Preview.

If Discord keeps using the previous microphone, set Input Device explicitly instead of Default, then restart the voice call or reload Discord.

## Known Limitations

- Media packets are encrypted on the LAN before the receiver decrypts and forwards local RTP to GStreamer on loopback.
- `v4l2loopback` still needs a loaded kernel module; the receiver does not load it automatically.
- The virtual microphone modules are per-user PipeWire/Pulse modules and may need to be recreated after logout, PipeWire restart, or `pactl unload-module`.
- Desktop apps cache device lists. If a device was created while the app was open, reopen the settings page or restart the app.
- mDNS visibility depends on the host firewall and Avahi/system resolver setup. Manual host entry in Android is the fallback.
