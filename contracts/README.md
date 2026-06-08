# ACamera Shared Contract Fixtures

These fixtures are the source of truth for Android and Linux contract tests.

The v1 wire contract is intentionally small:

- Discovery service type: `_acamera._udp.local`
- Control API: JSON over HTTP plus WebSocket events
- Media API: RTP over UDP
- Video codec: H.264
- Audio codec: Opus
- Quality presets: `low`, `balanced`, `high`
- Virtual camera name: `ACamera`
- Virtual microphone name: `ACamera Microphone`

You may copy these fixtures into platform-specific test resources, but changes to field names or semantics must update both Android and Linux tests in the same change.

## HTTP Endpoints

- `GET /status` returns `receiver_status.*.json`.
- `POST /pair/request` starts phone-PIN pairing.
- `GET /pair/pending` lists pending phone-PIN approvals for the Linux UI.
- `POST /pair/approve` approves a pending phone-PIN request from the Linux UI.
- `GET /pair/result?pairing_id=...` returns pending/approved/expired/rejected state; approved results carry the encrypted pairing result.
- `POST /pair` accepts `pair.request.json` and returns `pair.success.json` or `pair.invalid_pin.json` as a debug/manual fallback.
- `POST /session/start` accepts `session_start.request.json` and returns `session_start.success.json`.
- `POST /session/stop` accepts `session_stop.request.json` and returns `session_stop.success.json`.
- `WS /session/events` emits messages shaped like `events.*.json`.

## Authentication

Session APIs use the bearer token returned by secure phone-PIN pairing or the debug/manual `POST /pair` fallback.

Example:

```text
Authorization: Bearer session_0123456789abcdef
```
