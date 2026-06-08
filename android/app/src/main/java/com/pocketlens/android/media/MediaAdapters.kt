package com.pocketlens.android.media

import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.state.CameraFacing

data class EncodedFrame(
    val payload: ByteArray,
    val presentationTimeUs: Long,
    val keyFrame: Boolean = false,
) {
    override fun equals(other: Any?): Boolean =
        other is EncodedFrame &&
            payload.contentEquals(other.payload) &&
            presentationTimeUs == other.presentationTimeUs &&
            keyFrame == other.keyFrame

    override fun hashCode(): Int {
        var result = payload.contentHashCode()
        result = 31 * result + presentationTimeUs.hashCode()
        result = 31 * result + keyFrame.hashCode()
        return result
    }
}

interface VideoCaptureController {
    suspend fun start(facing: CameraFacing, settings: VideoEncoderSettings)
    suspend fun flip(to: CameraFacing)
    suspend fun pause(paused: Boolean)
    suspend fun stop()
}

interface MicrophoneCaptureController {
    suspend fun start(settings: AudioEncoderSettings)
    suspend fun mute(muted: Boolean)
    suspend fun stop()
}

interface VideoEncoder {
    fun configure(settings: VideoEncoderSettings)
    fun encodeFrame(frame: ByteArray, presentationTimeUs: Long): EncodedFrame
}

interface AudioEncoder {
    fun configure(settings: AudioEncoderSettings)
    fun encodeFrame(frame: ByteArray, presentationTimeUs: Long): EncodedFrame
}

data class MediaPipelinePolicy(
    val sendVideo: Boolean,
    val sendAudio: Boolean,
    val settings: EncoderSettings,
)

object MediaPipelinePolicyFactory {
    fun create(
        preset: QualityPreset,
        microphoneMuted: Boolean,
        videoPaused: Boolean,
    ): MediaPipelinePolicy =
        MediaPipelinePolicy(
            sendVideo = !videoPaused,
            sendAudio = !microphoneMuted,
            settings = PresetMapper.settingsFor(preset),
        )
}
