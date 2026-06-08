package com.pocketlens.android.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

object PocketLensJson {
    val instance: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = false
        classDiscriminator = "type"
    }
}

@Serializable
enum class QualityPreset {
    @SerialName("low")
    LOW,

    @SerialName("balanced")
    BALANCED,

    @SerialName("high")
    HIGH,
}

@Serializable
data class ReceiverStatus(
    @SerialName("receiver_name")
    val receiverName: String,
    @SerialName("protocol_version")
    val protocolVersion: Int,
    @SerialName("service_type")
    val serviceType: String,
    val paired: Boolean,
    @SerialName("active_session")
    val activeSession: Boolean,
    val capabilities: ReceiverCapabilities,
    @SerialName("virtual_devices")
    val virtualDevices: VirtualDevices,
    val diagnostics: List<ReceiverDiagnostic> = emptyList(),
)

@Serializable
data class ReceiverCapabilities(
    @SerialName("video_codecs")
    val videoCodecs: List<String>,
    @SerialName("audio_codecs")
    val audioCodecs: List<String>,
    @SerialName("quality_presets")
    val qualityPresets: List<QualityPreset>,
    @SerialName("adaptive_quality")
    val adaptiveQuality: Boolean,
    @SerialName("secure_pairing")
    val securePairing: Boolean = false,
    @SerialName("encrypted_rtp")
    val encryptedRtp: Boolean = false,
)

@Serializable
data class VirtualDevices(
    val camera: VirtualDevice,
    val microphone: VirtualDevice,
)

@Serializable
data class VirtualDevice(
    val name: String,
    val ready: Boolean,
    val backend: String,
)

@Serializable
data class ReceiverDiagnostic(
    val code: String,
    val severity: String,
    val message: String,
)

@Serializable
data class PairRequest(
    val pin: String,
    @SerialName("device_name")
    val deviceName: String,
)

@Serializable
data class PairResponse(
    @SerialName("session_token")
    val sessionToken: String,
    @SerialName("receiver_name")
    val receiverName: String,
    @SerialName("protocol_version")
    val protocolVersion: Int,
    @SerialName("expires_in_seconds")
    val expiresInSeconds: Long,
)

@Serializable
data class SecurePairRequest(
    @SerialName("pairing_id")
    val pairingId: String,
    @SerialName("device_name")
    val deviceName: String,
    @SerialName("phone_nonce")
    val phoneNonce: String,
    @SerialName("phone_public_key")
    val phonePublicKey: String,
)

@Serializable
data class SecurePairRequestResponse(
    @SerialName("pairing_id")
    val pairingId: String,
    @SerialName("receiver_nonce")
    val receiverNonce: String,
    @SerialName("receiver_public_key")
    val receiverPublicKey: String,
    @SerialName("expires_in_seconds")
    val expiresInSeconds: Long,
)

@Serializable
enum class SecurePairingStatus {
    @SerialName("pending")
    PENDING,

    @SerialName("approved")
    APPROVED,

    @SerialName("expired")
    EXPIRED,

    @SerialName("rejected")
    REJECTED,
}

@Serializable
data class SecurePairResultResponse(
    @SerialName("pairing_id")
    val pairingId: String,
    val status: SecurePairingStatus,
    @SerialName("encrypted_result")
    val encryptedResult: String? = null,
)

@Serializable
data class ErrorEnvelope(
    val error: ContractError,
)

@Serializable
data class ContractError(
    val code: String,
    val message: String,
)

@Serializable
data class SessionStartRequest(
    @SerialName("session_token")
    val sessionToken: String,
    @SerialName("quality_preset")
    val qualityPreset: QualityPreset = QualityPreset.BALANCED,
    val video: VideoConfig = VideoConfig(),
    val audio: AudioConfig = AudioConfig(),
)

@Serializable
data class VideoConfig(
    val codec: String = "h264",
    val width: Int = 1280,
    val height: Int = 720,
    val fps: Int = 30,
)

@Serializable
data class AudioConfig(
    val codec: String = "opus",
    @SerialName("sample_rate_hz")
    val sampleRateHz: Int = 48_000,
    val channels: Int = 1,
)

@Serializable
data class SessionStartResponse(
    @SerialName("session_id")
    val sessionId: String,
    @SerialName("receiver_host")
    val receiverHost: String,
    @SerialName("video_rtp_port")
    val videoRtpPort: Int,
    @SerialName("audio_rtp_port")
    val audioRtpPort: Int,
    @SerialName("video_payload_type")
    val videoPayloadType: Int,
    @SerialName("audio_payload_type")
    val audioPayloadType: Int,
    @SerialName("ssrc_video")
    val ssrcVideo: Long,
    @SerialName("ssrc_audio")
    val ssrcAudio: Long,
    @SerialName("quality_preset")
    val qualityPreset: QualityPreset,
    val video: VideoConfig,
    val audio: AudioConfig,
    @SerialName("media_encryption")
    val mediaEncryption: MediaEncryption? = null,
)

@Serializable
data class MediaEncryption(
    val mode: MediaEncryptionMode,
    @SerialName("video_key")
    val videoKey: String,
    @SerialName("audio_key")
    val audioKey: String,
)

@Serializable
enum class MediaEncryptionMode {
    @SerialName("aes_256_gcm_v1")
    AES_256_GCM_V1,
}

@Serializable
data class SessionStopRequest(
    @SerialName("session_token")
    val sessionToken: String,
    @SerialName("session_id")
    val sessionId: String,
)

@Serializable
data class SessionStopResponse(
    @SerialName("session_id")
    val sessionId: String,
    val stopped: Boolean,
)

@Serializable
sealed interface ReceiverEvent {
    @Serializable
    @SerialName("stats")
    data class Stats(
        @SerialName("session_id")
        val sessionId: String,
        @SerialName("video_packets")
        val videoPackets: Long,
        @SerialName("audio_packets")
        val audioPackets: Long,
        @SerialName("video_packets_lost")
        val videoPacketsLost: Long,
        @SerialName("audio_packets_lost")
        val audioPacketsLost: Long,
        @SerialName("estimated_bitrate_kbps")
        val estimatedBitrateKbps: Int,
        @SerialName("quality_preset")
        val qualityPreset: QualityPreset,
    ) : ReceiverEvent

    @Serializable
    @SerialName("warning")
    data class Warning(
        @SerialName("session_id")
        val sessionId: String?,
        val code: String,
        val message: String,
    ) : ReceiverEvent

    @Serializable
    @SerialName("error")
    data class Error(
        @SerialName("session_id")
        val sessionId: String?,
        val code: String,
        val message: String,
    ) : ReceiverEvent
}
