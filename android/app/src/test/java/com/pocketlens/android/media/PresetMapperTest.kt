package com.pocketlens.android.media

import com.pocketlens.android.protocol.QualityPreset
import kotlin.test.Test
import kotlin.test.assertEquals

class PresetMapperTest {
    @Test
    fun mapsBalancedPresetToDefault720pLowLatency() {
        val settings = PresetMapper.settingsFor(QualityPreset.BALANCED)

        assertEquals(1280, settings.video.width)
        assertEquals(720, settings.video.height)
        assertEquals(30, settings.video.fps)
        assertEquals("video/avc", settings.video.mimeType)
        assertEquals(48_000, settings.audio.sampleRateHz)
        assertEquals("audio/opus", settings.audio.mimeType)
    }

    @Test
    fun mapsLowAndHighPresetResolutions() {
        assertEquals(480, PresetMapper.settingsFor(QualityPreset.LOW).video.height)
        assertEquals(1080, PresetMapper.settingsFor(QualityPreset.HIGH).video.height)
    }
}
