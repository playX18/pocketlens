package com.pocketlens.android.media

import com.pocketlens.android.protocol.QualityPreset
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class MediaPipelinePolicyTest {
    @Test
    fun muteStopsAudioSendingAndPauseStopsVideoSending() {
        val muted = MediaPipelinePolicyFactory.create(
            preset = QualityPreset.BALANCED,
            microphoneMuted = true,
            videoPaused = false,
        )
        val paused = MediaPipelinePolicyFactory.create(
            preset = QualityPreset.BALANCED,
            microphoneMuted = false,
            videoPaused = true,
        )

        assertFalse(muted.sendAudio)
        assertTrue(muted.sendVideo)
        assertTrue(paused.sendAudio)
        assertFalse(paused.sendVideo)
    }

    @Test
    fun policyCarriesPresetEncoderSettings() {
        val policy = MediaPipelinePolicyFactory.create(
            preset = QualityPreset.HIGH,
            microphoneMuted = false,
            videoPaused = false,
        )

        assertEquals(1080, policy.settings.video.height)
        assertEquals(96, policy.settings.audio.bitrateKbps)
    }
}
