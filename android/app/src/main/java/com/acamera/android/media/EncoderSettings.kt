package com.acamera.android.media

import com.acamera.android.protocol.QualityPreset

data class VideoEncoderSettings(
    val width: Int,
    val height: Int,
    val fps: Int,
    val bitrateKbps: Int,
    val mimeType: String = "video/avc",
)

data class AudioEncoderSettings(
    val sampleRateHz: Int,
    val channelCount: Int,
    val bitrateKbps: Int,
    val mimeType: String = "audio/opus",
)

data class EncoderSettings(
    val video: VideoEncoderSettings,
    val audio: AudioEncoderSettings,
)

object PresetMapper {
    fun settingsFor(preset: QualityPreset): EncoderSettings =
        when (preset) {
            QualityPreset.LOW -> EncoderSettings(
                video = VideoEncoderSettings(width = 854, height = 480, fps = 30, bitrateKbps = 1_200),
                audio = AudioEncoderSettings(sampleRateHz = 48_000, channelCount = 1, bitrateKbps = 48),
            )
            QualityPreset.BALANCED -> EncoderSettings(
                video = VideoEncoderSettings(width = 1280, height = 720, fps = 30, bitrateKbps = 2_500),
                audio = AudioEncoderSettings(sampleRateHz = 48_000, channelCount = 1, bitrateKbps = 64),
            )
            QualityPreset.HIGH -> EncoderSettings(
                video = VideoEncoderSettings(width = 1920, height = 1080, fps = 30, bitrateKbps = 4_500),
                audio = AudioEncoderSettings(sampleRateHz = 48_000, channelCount = 1, bitrateKbps = 96),
            )
        }
}
