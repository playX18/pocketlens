use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;
pub const SERVICE_TYPE: &str = "_acamera._udp.local";
pub const DEVICE_CAMERA_NAME: &str = "ACamera";
pub const DEVICE_MICROPHONE_NAME: &str = "ACamera Microphone";
pub const RECEIVER_NAME: &str = "ACamera Linux";
pub const DEFAULT_RECEIVER_HOST: &str = "192.168.1.25";
pub const DEFAULT_VIDEO_RTP_PORT: u16 = 5004;
pub const DEFAULT_AUDIO_RTP_PORT: u16 = 5006;
pub const DEFAULT_VIDEO_PAYLOAD_TYPE: u8 = 96;
pub const DEFAULT_AUDIO_PAYLOAD_TYPE: u8 = 97;
pub const DEFAULT_VIDEO_SSRC: u32 = 0x1234_5678;
pub const DEFAULT_AUDIO_SSRC: u32 = 0x9ABC_DEF0;
pub const SESSION_TOKEN_EXPIRES_IN_SECONDS: u64 = 86_400;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum QualityPreset {
    Low,
    Balanced,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyStatus {
    pub name: String,
    pub present: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusResponse {
    pub receiver_name: String,
    pub protocol_version: u16,
    pub service_type: String,
    pub paired: bool,
    pub active_session: bool,
    pub capabilities: Capabilities,
    pub virtual_devices: VirtualDevices,
    pub diagnostics: Vec<Diagnostic>,
}

impl StatusResponse {
    pub fn validate_protocol_version(&self) -> Result<(), ProtocolError> {
        if self.protocol_version == PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(ProtocolError::UnsupportedVersion(self.protocol_version))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub video_codecs: Vec<Codec>,
    pub audio_codecs: Vec<Codec>,
    pub quality_presets: Vec<QualityPreset>,
    pub adaptive_quality: bool,
    #[serde(default)]
    pub secure_pairing: bool,
    #[serde(default)]
    pub encrypted_rtp: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            video_codecs: vec![Codec::H264],
            audio_codecs: vec![Codec::Opus],
            quality_presets: vec![
                QualityPreset::Low,
                QualityPreset::Balanced,
                QualityPreset::High,
            ],
            adaptive_quality: true,
            secure_pairing: true,
            encrypted_rtp: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VirtualDevices {
    pub camera: VirtualDevice,
    pub microphone: VirtualDevice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VirtualDevice {
    pub name: String,
    pub ready: bool,
    pub backend: VirtualDeviceBackend,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VirtualDeviceBackend {
    V4l2loopback,
    Pipewire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    MissingV4l2loopback,
    MissingPipewire,
    MissingGstreamer,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairRequest {
    pub pin: String,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairResponse {
    pub session_token: String,
    pub receiver_name: String,
    pub protocol_version: u16,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurePairRequest {
    pub pairing_id: String,
    pub device_name: String,
    pub phone_nonce: String,
    pub phone_public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurePairRequestResponse {
    pub pairing_id: String,
    pub receiver_nonce: String,
    pub receiver_public_key: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingPairingRequest {
    pub pairing_id: String,
    pub device_name: String,
    pub requested_at_unix_ms: u64,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingPairingResponse {
    pub requests: Vec<PendingPairingRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurePairApproveRequest {
    pub pairing_id: String,
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurePairingStatus {
    Pending,
    Approved,
    Expired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurePairApproveResponse {
    pub pairing_id: String,
    pub status: SecurePairingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurePairResultResponse {
    pub pairing_id: String,
    pub status: SecurePairingStatus,
    pub encrypted_result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiErrorBody {
    pub code: ApiErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    InvalidPin,
    Unauthorized,
    MissingDependencies,
    InvalidSessionState,
    UnsupportedProtocolVersion,
    BadRequest,
    MediaPipelineFailed,
    NetworkDegraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStartRequest {
    pub session_token: String,
    pub quality_preset: QualityPreset,
    pub video: VideoSettings,
    pub audio: AudioSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStartResponse {
    pub session_id: String,
    pub receiver_host: String,
    pub video_rtp_port: u16,
    pub audio_rtp_port: u16,
    pub video_payload_type: u8,
    pub audio_payload_type: u8,
    pub ssrc_video: u32,
    pub ssrc_audio: u32,
    pub quality_preset: QualityPreset,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_encryption: Option<MediaEncryption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaEncryption {
    pub mode: MediaEncryptionMode,
    pub video_key: String,
    pub audio_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaEncryptionMode {
    #[serde(rename = "aes_256_gcm_v1")]
    Aes256GcmV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStopRequest {
    pub session_token: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStopResponse {
    pub session_id: String,
    pub stopped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoSettings {
    pub codec: Codec,
    pub width: u16,
    pub height: u16,
    pub fps: u8,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            width: 1280,
            height: 720,
            fps: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSettings {
    pub codec: Codec,
    pub sample_rate_hz: u32,
    pub channels: u8,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            codec: Codec::Opus,
            sample_rate_hz: 48_000,
            channels: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    Opus,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStateName {
    Idle,
    Paired,
    Starting,
    Active,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventMessage {
    Stats {
        session_id: String,
        video_packets: u64,
        audio_packets: u64,
        video_packets_lost: u64,
        audio_packets_lost: u64,
        estimated_bitrate_kbps: u32,
        quality_preset: QualityPreset,
    },
    Warning {
        session_id: String,
        code: ApiErrorCode,
        message: String,
    },
    Error {
        session_id: String,
        code: ApiErrorCode,
        message: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(json: &str) -> T
    where
        T: for<'de> Deserialize<'de> + Serialize + PartialEq + std::fmt::Debug,
    {
        let parsed: T = serde_json::from_str(json).unwrap();
        let encoded = serde_json::to_string(&parsed).unwrap();
        serde_json::from_str(&encoded).unwrap()
    }

    #[test]
    fn fixtures_match_dtos() {
        let _: StatusResponse = round_trip(include_str!(
            "../../receiver/tests/fixtures/receiver_status.ready.json"
        ));
        let _: StatusResponse = round_trip(include_str!(
            "../../receiver/tests/fixtures/receiver_status.missing_dependencies.json"
        ));
        let _: PairResponse = round_trip(include_str!(
            "../../receiver/tests/fixtures/pair.success.json"
        ));
        let _: ApiErrorResponse = round_trip(include_str!(
            "../../receiver/tests/fixtures/pair.invalid_pin.json"
        ));
        let _: SessionStartResponse = round_trip(include_str!(
            "../../receiver/tests/fixtures/session_start.success.json"
        ));
        let _: EventMessage = round_trip(include_str!(
            "../../receiver/tests/fixtures/events.stats.json"
        ));
        let _: EventMessage = round_trip(include_str!(
            "../../receiver/tests/fixtures/events.warning.json"
        ));
        let _: EventMessage = round_trip(include_str!(
            "../../receiver/tests/fixtures/events.error.json"
        ));
    }

    #[test]
    fn unknown_protocol_versions_are_typed_errors() {
        let status = StatusResponse {
            receiver_name: RECEIVER_NAME.to_string(),
            protocol_version: 99,
            service_type: SERVICE_TYPE.to_string(),
            paired: false,
            active_session: false,
            capabilities: Capabilities::default(),
            virtual_devices: VirtualDevices {
                camera: VirtualDevice {
                    name: DEVICE_CAMERA_NAME.to_string(),
                    ready: true,
                    backend: VirtualDeviceBackend::V4l2loopback,
                },
                microphone: VirtualDevice {
                    name: DEVICE_MICROPHONE_NAME.to_string(),
                    ready: true,
                    backend: VirtualDeviceBackend::Pipewire,
                },
            },
            diagnostics: vec![],
        };
        assert_eq!(
            status.validate_protocol_version(),
            Err(ProtocolError::UnsupportedVersion(99))
        );
    }
}
