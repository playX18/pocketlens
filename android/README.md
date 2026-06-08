# ACamera Android

This Android project owns the sender app only.

## Local Requirements

- Android Studio or Android SDK with API 36 installed.
- Gradle 9.5.1 or the generated Gradle wrapper.
- JDK compatible with the selected Android Gradle Plugin.

This workspace currently has `adb`, but no configured Android SDK environment variables and only a very old system Gradle. On a configured Android workstation, run:

```sh
cd android
gradle wrapper --gradle-version 9.5.1
./gradlew test
```

## Current Scope

- Protocol DTOs and JSON fixtures for the shared v1 API.
- Testable reducers for app start, permissions, discovery, pairing, session, and controls.
- mDNS TXT parsing helpers.
- RTP packet, H.264 FU-A/single NAL packetization, and Opus packetization helpers.
- Compose shell for discovery, manual receiver entry, pairing, streaming controls, and receiver status/errors.
- Control-plane HTTP calls and receiver WebSocket event consumption after session start.
- Runtime media adapters for Camera2 -> MediaCodec H.264 surface encoding and AudioRecord -> MediaCodec Opus encoding.

## Runtime Notes

- The video adapter opens the selected Camera2 device, renders a record capture session into the H.264 encoder input surface, drains encoded output, splits Annex B or length-prefixed access units into NAL units, and sends them to the RTP coordinator.
- The audio adapter reads 16-bit PCM from `AudioRecord`, queues it into the Opus `MediaCodec`, drains encoded frames, and suppresses frame delivery while muted.
- JVM unit tests cover control/event wiring and pure packet/capture helpers. Actual Camera2, microphone, and codec behavior still requires an Android device smoke test because emulator/device codec support varies.
- If an Android device lacks an Opus encoder or rejects the negotiated H.264 profile/size, session start will fail from the media adapter and surface the error in the app UI.
