package com.pocketlens.android.media

import com.pocketlens.android.protocol.AudioConfig
import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.protocol.SessionStartResponse
import com.pocketlens.android.protocol.VideoConfig
import com.pocketlens.android.rtp.RtpPacket
import com.pocketlens.android.rtp.RtpSender
import com.pocketlens.android.rtp.RtpTarget
import com.pocketlens.android.state.CameraFacing
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SessionStreamCoordinatorTest {
    @Test
    fun startMapsNegotiatedResponseToCaptureSettingsAndRtpTargets() = runBlocking {
        val videoSender = RecordingRtpSender()
        val audioSender = RecordingRtpSender()
        val videoCapture = RecordingVideoCaptureController()
        val microphone = RecordingMicrophoneCaptureController()
        val coordinator = SessionStreamCoordinator(videoSender, audioSender, videoCapture, microphone)

        coordinator.start(
            response = response(),
            controls = StreamControls(cameraFacing = CameraFacing.FRONT),
        )
        coordinator.sendVideoNal(byteArrayOf(0x65, 1, 2), frameIndex = 0)
        coordinator.sendAudioFrame(byteArrayOf(0x11, 0x22), packetIndex = 0)

        assertEquals(CameraFacing.FRONT, videoCapture.startedFacing)
        assertEquals(VideoEncoderSettings(width = 640, height = 360, fps = 24, bitrateKbps = 2_500), videoCapture.startedSettings)
        assertEquals(AudioEncoderSettings(sampleRateHz = 48_000, channelCount = 1, bitrateKbps = 64), microphone.startedSettings)
        assertEquals(RtpTarget("10.0.0.2", 50_004), videoSender.sent.single().target)
        assertEquals(RtpTarget("10.0.0.2", 50_006), audioSender.sent.single().target)
        assertEquals(110, videoSender.sent.single().packet.payloadType)
        assertEquals(111, audioSender.sent.single().packet.payloadType)
        assertEquals(0x01020304u, videoSender.sent.single().packet.ssrc)
        assertEquals(0x05060708u, audioSender.sent.single().packet.ssrc)
    }

    @Test
    fun muteAndPauseGateAudioAndVideoPacketsWithoutStoppingSession() = runBlocking {
        val videoSender = RecordingRtpSender()
        val audioSender = RecordingRtpSender()
        val coordinator = SessionStreamCoordinator(
            videoSender,
            audioSender,
            RecordingVideoCaptureController(),
            RecordingMicrophoneCaptureController(),
        )

        coordinator.start(response(), StreamControls(microphoneMuted = true, videoPaused = true))
        coordinator.sendVideoNal(byteArrayOf(0x65, 1), frameIndex = 0)
        coordinator.sendAudioFrame(byteArrayOf(0x11), packetIndex = 0)
        assertTrue(videoSender.sent.isEmpty())
        assertTrue(audioSender.sent.isEmpty())

        coordinator.setVideoPaused(false)
        coordinator.setMicrophoneMuted(false)
        coordinator.sendVideoNal(byteArrayOf(0x65, 1), frameIndex = 1)
        coordinator.sendAudioFrame(byteArrayOf(0x11), packetIndex = 1)

        assertEquals(1, videoSender.sent.size)
        assertEquals(1, audioSender.sent.size)
    }

    @Test
    fun presetChangesApplyToCaptureControllers() = runBlocking {
        val videoCapture = RecordingVideoCaptureController()
        val microphone = RecordingMicrophoneCaptureController()
        val coordinator = SessionStreamCoordinator(
            RecordingRtpSender(),
            RecordingRtpSender(),
            videoCapture,
            microphone,
        )

        coordinator.start(response(), StreamControls())
        coordinator.setPreset(QualityPreset.HIGH)

        assertEquals(1080, videoCapture.startedSettings?.height)
        assertEquals(96, microphone.startedSettings?.bitrateKbps)
    }

    @Test
    fun stopStopsCaptureControllersAndClosesSenders() = runBlocking {
        val videoSender = RecordingRtpSender()
        val audioSender = RecordingRtpSender()
        val videoCapture = RecordingVideoCaptureController()
        val microphone = RecordingMicrophoneCaptureController()
        val coordinator = SessionStreamCoordinator(videoSender, audioSender, videoCapture, microphone)

        coordinator.start(response(), StreamControls())
        coordinator.stop()

        assertTrue(videoCapture.stopped)
        assertTrue(microphone.stopped)
        assertTrue(videoSender.closed)
        assertTrue(audioSender.closed)
        assertFalse(coordinator.active)
    }

    private fun response() = SessionStartResponse(
        sessionId = "sess-1",
        receiverHost = "10.0.0.2",
        videoRtpPort = 50_004,
        audioRtpPort = 50_006,
        videoPayloadType = 110,
        audioPayloadType = 111,
        ssrcVideo = 0x01020304,
        ssrcAudio = 0x05060708,
        qualityPreset = QualityPreset.BALANCED,
        video = VideoConfig(width = 640, height = 360, fps = 24),
        audio = AudioConfig(sampleRateHz = 48_000, channels = 1),
    )

    private class RecordingRtpSender : RtpSender {
        val sent = mutableListOf<SentRtpPacket>()
        var closed = false

        override suspend fun send(target: RtpTarget, packet: RtpPacket) {
            sent += SentRtpPacket(target, packet)
        }

        override suspend fun close() {
            closed = true
        }
    }

    private data class SentRtpPacket(
        val target: RtpTarget,
        val packet: RtpPacket,
    )

    private class RecordingVideoCaptureController : VideoCaptureController {
        var startedFacing: CameraFacing? = null
        var startedSettings: VideoEncoderSettings? = null
        var stopped = false

        override suspend fun start(facing: CameraFacing, settings: VideoEncoderSettings) {
            startedFacing = facing
            startedSettings = settings
            stopped = false
        }

        override suspend fun flip(to: CameraFacing) {
            startedFacing = to
        }

        override suspend fun pause(paused: Boolean) = Unit

        override suspend fun stop() {
            stopped = true
        }
    }

    private class RecordingMicrophoneCaptureController : MicrophoneCaptureController {
        var startedSettings: AudioEncoderSettings? = null
        var stopped = false

        override suspend fun start(settings: AudioEncoderSettings) {
            startedSettings = settings
            stopped = false
        }

        override suspend fun mute(muted: Boolean) = Unit

        override suspend fun stop() {
            stopped = true
        }
    }
}
