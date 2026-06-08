# PocketLens Linux Setup

This is the Linux receiver side for the Android PocketLens sender. The receiver is a user-space daemon; it is not a kernel driver.

## Dependencies

### Debian / Ubuntu

Runtime packages:

```sh
sudo apt install -y \
  v4l2loopback-dkms v4l-utils \
  pipewire pipewire-pulse pulseaudio-utils \
  gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav
```

Build packages (needed for `./install.sh` or `cargo build`):

```sh
sudo apt install -y rustc cargo pkg-config libgtk-4-dev libadwaita-1-dev
```

Optional but useful:

```sh
sudo apt install -y adb curl ffmpeg
```

Check all dependencies at once:

```sh
pocketlens-receiver --check-deps
```

### Other Linux distributions

Install equivalents for:

- **v4l2loopback** — kernel module for the virtual webcam
- **v4l-utils** — `v4l2-ctl` for camera diagnostics
- **PipeWire** with PulseAudio compatibility — `pipewire`, `pipewire-pulse`, and `pactl`
- **GStreamer 1.0** — `gst-launch-1.0` plus base, good, bad, ugly, and libav plugins
- **Rust** — `rustc` and `cargo` to build from source
- **GTK 4 / libadwaita** — development libraries and `pkg-config` for the GTK launcher
- **adb** (optional) — install APKs to a connected phone

Fedora example package names: `v4l2loopback`, `v4l-utils`, `pipewire`, `pipewire-pulseaudio`, `gstreamer1-plugins-{base,good,bad-free,ugly-free}`, `gstreamer1-libav`, `rust`, `cargo`, `gtk4-devel`, `libadwaita-devel`, `pkgconf-pkg-config`, `android-tools`.

Arch example package names: `v4l2loopback-dkms`, `v4l-utils`, `pipewire`, `pipewire-pulse`, `gst-plugins-{base,good,bad,ugly}`, `gst-libav`, `rust`, `gtk4`, `libadwaita`, `pkgconf`, `android-tools`.

Run `pocketlens-receiver --check-deps` after installing to confirm readiness.

## Quick Start (GTK App)

Install from the repository root:

```sh
./install.sh
```

Launch the GTK dashboard:

```sh
pocketlens-gtk
```

The app provides a single-page dashboard with:

- **Setup All Devices** — creates the virtual camera (v4l2loopback) and virtual microphone (PipeWire/Pulse)
- **Start/Stop Receiver** — starts the HTTP control server
- **Install APK** — detects connected Android devices via adb and installs the bundled APK
- Collapsible **Settings**, **Log** sections

## Build and Install

From the repository root:

```sh
./install.sh
```

Or build manually:

```sh
cd linux
cargo build --release
pocketlens-receiver --install --prefix ~/.local
```

This copies both binaries to `~/.local/bin/`, creates a `.desktop` file, and places the bundled APK in `~/.local/share/pocketlens/`.

## Manual Setup (Headless)

### Virtual Camera

```sh
pocketlens-receiver --setup-camera --device /dev/video10
```

Remove it:

```sh
pocketlens-receiver --remove-camera
```

### Virtual Microphone

```sh
pocketlens-receiver --setup-virtual-mic
```

This creates:

- Sink for receiver audio: `pocketlens_sink` / `PocketLens Audio Sink`
- Source for desktop apps: `pocketlens_microphone` / `PocketLens Microphone`

Remove:

```sh
pocketlens-receiver --remove-virtual-mic
```

### Diagnostics

```sh
pocketlens-receiver --diagnose
```

Prints JSON with `v4l2loopback`, `pipewire`, `pactl`, `gst-launch-1.0`, and `pocketlens_microphone` readiness. The daemon is allowed to start with missing dependencies, but `/status` reports missing pieces and `/session/start` refuses to start media.

## Run Receiver

```sh
pocketlens-receiver --receiver-name "Desk" --control-port 47650 --camera-device /dev/video10 --microphone-sink pocketlens_sink
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

The receiver advertises `_pocketlens._udp.local` over mDNS while the daemon is running. TXT records use these keys:

- `name`: receiver display name, for example `Desk`
- `version`: protocol version, currently `1`
- `control_port`: HTTP/WebSocket control port, default `47650`
- `capabilities`: comma-separated values, currently `h264,opus,rtp`

The WebSocket event endpoint validates the paired `session_token` before upgrade. When a session is already active, it sends an initial typed `stats` event, then forwards lifecycle `stats`, `warning`, and `error` events emitted by media start/stop/failure paths.

Media is negotiated as encrypted RTP/UDP H.264 video on port `5004` and encrypted Opus audio on port `5006` by default. Override those with `--video-port` and `--audio-port`.

On session start, the daemon owns two `gst-launch-1.0` processes:

- Video: `udpsrc` receives RTP/H.264, decodes it, converts raw video, and writes to `v4l2sink device=/dev/video10` unless `--camera-device` points elsewhere.
- Audio: `udpsrc` receives RTP/Opus, decodes it, converts/resamples audio, and writes to `pulsesink device=pocketlens_sink client-name="PocketLens Microphone"`.

The `pulsesink` path relies on `pipewire-pulse`. The helper creates a null sink plus a remapped source so Zoom, Discord, and browsers see a stable microphone input instead of a transient playback/client node. If you skip virtual mic setup, `/status` will report the microphone as not ready and `/session/start` will refuse media.

## Cleanup

Kill all stale receiver and GStreamer processes:

```sh
pocketlens-receiver --cleanup
```

## Install APK to Android Device

Install the bundled APK to a connected Android device via adb:

```sh
pocketlens-receiver --install-apk
```

Requirements:

- `adb` installed (Debian/Ubuntu: `sudo apt install adb`)
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

- Camera: `PocketLens`
- Microphone: `PocketLens Microphone`

If `pactl list short sources` shows `pocketlens_microphone`, the source is present. Some desktops display it as `PocketLens Microphone`; others show the technical source name.

## Zoom Device Selection

1. Start the Linux receiver and start streaming from Android.
2. Open Zoom Settings.
3. In Video, choose camera `PocketLens`.
4. In Audio, choose microphone `PocketLens Microphone`. If Zoom shows technical names, choose `pocketlens_microphone`.
5. Use Test Mic. The input meter should move when audio arrives from the Android device.

If Zoom only shows `Monitor of PocketLens Audio Sink`, select that as a fallback. It is the monitor source feeding the remapped microphone.

## Discord Device Selection

1. Start the Linux receiver and start streaming from Android.
2. Open User Settings -> Voice & Video.
3. Set Input Device to `PocketLens Microphone` or `pocketlens_microphone`.
4. Set Camera to `PocketLens`.
5. Use Mic Test and Video Preview.

If Discord keeps using the previous microphone, set Input Device explicitly instead of Default, then restart the voice call or reload Discord.

## Known Limitations

- Media packets are encrypted on the LAN before the receiver decrypts and forwards local RTP to GStreamer on loopback.
- `v4l2loopback` still needs a loaded kernel module; the receiver does not load it automatically.
- The virtual microphone modules are per-user PipeWire/Pulse modules and may need to be recreated after logout, PipeWire restart, or `pactl unload-module`.
- Desktop apps cache device lists. If a device was created while the app was open, reopen the settings page or restart the app.
- mDNS visibility depends on the host firewall and Avahi/system resolver setup. Manual host entry in Android is the fallback.
