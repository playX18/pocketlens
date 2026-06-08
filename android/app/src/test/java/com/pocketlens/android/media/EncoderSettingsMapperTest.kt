package com.pocketlens.android.media

import com.pocketlens.android.protocol.AudioConfig
import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.protocol.SessionStartResponse
import com.pocketlens.android.protocol.VideoConfig
import kotlin.test.Test
import kotlin.test.assertEquals

class EncoderSettingsMapperTest {
    @Test
    fun responseDimensionsOverridePresetDefaultsButKeepPresetBitrates() {
        val settings = EncoderSettingsMapper.fromSessionStart(
            SessionStartResponse(
                sessionId = "sess-1",
                receiverHost = "127.0.0.1",
                videoRtpPort = 50_004,
                audioRtpPort = 50_006,
                videoPayloadType = 96,
                audioPayloadType = 97,
                ssrcVideo = 1,
                ssrcAudio = 2,
                qualityPreset = QualityPreset.HIGH,
                video = VideoConfig(width = 1_280, height = 720, fps = 60),
                audio = AudioConfig(sampleRateHz = 44_100, channels = 2),
            ),
        )

        assertEquals(1_280, settings.video.width)
        assertEquals(720, settings.video.height)
        assertEquals(60, settings.video.fps)
        assertEquals(4_500, settings.video.bitrateKbps)
        assertEquals(44_100, settings.audio.sampleRateHz)
        assertEquals(2, settings.audio.channelCount)
        assertEquals(96, settings.audio.bitrateKbps)
    }

    @Test
    fun presetSettingsAreUsedForLocalQualityChanges() {
        val settings = EncoderSettingsMapper.fromPreset(QualityPreset.LOW)

        assertEquals(854, settings.video.width)
        assertEquals(480, settings.video.height)
        assertEquals(48, settings.audio.bitrateKbps)
    }
}
