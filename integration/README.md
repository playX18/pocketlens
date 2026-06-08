# PocketLens Integration Plan

This directory holds cross-project contract verification and end-to-end test plans.

## Preconditions

- Android has protocol DTO tests passing.
- Linux has protocol DTO tests passing.
- Both sides either read `contracts/fixtures/` directly or maintain copied fixtures with drift tests.
- Linux receiver can run `--diagnose`.
- Android app can run against a fake receiver.

## Contract Verification

Add a single command that verifies:

- Every fixture in `contracts/fixtures/` is accepted by the Linux DTO tests.
- Every fixture in `contracts/fixtures/` is accepted by the Android DTO tests.
- Both sides agree on protocol version `1`.
- Both sides agree on service type `_pocketlens._udp.local`.
- Both sides agree on codec names `h264` and `opus`.
- Both sides agree on quality presets `low`, `balanced`, and `high`.

## Fake Receiver Flow

Before using real Linux media devices, verify Android against a fake control/media receiver:

1. Fake receiver advertises or is manually configured as `PocketLens Linux`.
2. Android pairs with PIN `123456`.
3. Android starts a session using the balanced preset.
4. Android sends synthetic or real RTP packets to ports returned by the fake receiver.
5. Fake receiver validates RTP sequence numbers, timestamps, payload types, and SSRC values.

## Synthetic Sender Flow

Before using a real Android phone, verify Linux against a synthetic sender:

1. Linux receiver starts with `--diagnose` clean or with expected dependency warnings.
2. Synthetic client calls `POST /pair`.
3. Synthetic client calls `POST /session/start`.
4. Synthetic client sends H.264 and Opus RTP samples.
5. Linux receiver emits stats over `WS /session/events`.

## End-to-End LAN Acceptance

A v1 end-to-end pass requires:

- Android device and Linux host on the same LAN.
- Linux receiver visible to Android via mDNS.
- PIN pairing succeeds.
- Android starts camera and microphone streaming.
- Linux shows active session stats.
- `PocketLens` appears as a camera in desktop app selectors.
- `PocketLens Microphone` appears as a microphone in desktop app selectors.
- Zoom or Discord can select both devices.
- Mute, pause video, camera flip, stop session, and reconnect work without restarting the daemon.

## Failure Cases

Integration tests should cover:

- Wrong PIN.
- Missing or expired token.
- Receiver missing v4l2loopback.
- Receiver missing PipeWire.
- RTP packet from unpaired IP.
- RTP packet with wrong payload type.
- Session start while another sender is active.

