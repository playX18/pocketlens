package com.pocketlens.android.media

import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.protocol.SessionStartResponse
import com.pocketlens.android.rtp.H264RtpPacketizer
import com.pocketlens.android.rtp.EncryptingRtpSender
import com.pocketlens.android.rtp.OpusRtpPacketizer
import com.pocketlens.android.rtp.RtpSequence
import com.pocketlens.android.rtp.RtpSender
import com.pocketlens.android.rtp.RtpTarget
import com.pocketlens.android.state.CameraFacing

data class StreamControls(
    val cameraFacing: CameraFacing = CameraFacing.BACK,
    val microphoneMuted: Boolean = false,
    val videoPaused: Boolean = false,
    val preset: QualityPreset = QualityPreset.BALANCED,
)

object EncoderSettingsMapper {
    fun fromSessionStart(response: SessionStartResponse): EncoderSettings {
        val preset = PresetMapper.settingsFor(response.qualityPreset)
        return EncoderSettings(
            video = preset.video.copy(
                width = response.video.width,
                height = response.video.height,
                fps = response.video.fps,
            ),
            audio = preset.audio.copy(
                sampleRateHz = response.audio.sampleRateHz,
                channelCount = response.audio.channels,
            ),
        )
    }

    fun fromPreset(preset: QualityPreset): EncoderSettings =
        PresetMapper.settingsFor(preset)
}

class SessionStreamCoordinator(
    private val videoSender: RtpSender,
    private val audioSender: RtpSender,
    private val videoCapture: VideoCaptureController,
    private val microphoneCapture: MicrophoneCaptureController,
) {
    private var session: ActiveMediaSession? = null
    private var controls: StreamControls = StreamControls()
    private val videoSequence = RtpSequence()
    private val audioSequence = RtpSequence()

    val active: Boolean
        get() = session != null

    suspend fun start(response: SessionStartResponse, controls: StreamControls = StreamControls()) {
        stop()

        val settings = EncoderSettingsMapper.fromSessionStart(response)
        val started = ActiveMediaSession(
            sessionId = response.sessionId,
            videoTarget = RtpTarget(response.receiverHost, response.videoRtpPort),
            audioTarget = RtpTarget(response.receiverHost, response.audioRtpPort),
            videoPayloadType = response.videoPayloadType,
            audioPayloadType = response.audioPayloadType,
            videoSsrc = response.ssrcVideo.toUInt(),
            audioSsrc = response.ssrcAudio.toUInt(),
            videoFps = response.video.fps,
            videoSender = response.mediaEncryption?.let { EncryptingRtpSender(keyHex = it.videoKey, streamLabel = "video") }
                ?: videoSender,
            audioSender = response.mediaEncryption?.let { EncryptingRtpSender(keyHex = it.audioKey, streamLabel = "audio") }
                ?: audioSender,
        )

        this.controls = controls.copy(preset = response.qualityPreset)
        session = started
        videoCapture.start(controls.cameraFacing, settings.video)
        videoCapture.pause(controls.videoPaused)
        microphoneCapture.start(settings.audio)
        microphoneCapture.mute(controls.microphoneMuted)
    }

    suspend fun stop() {
        val current = session
        session = null
        if (current != null) {
            videoCapture.stop()
            microphoneCapture.stop()
            current.videoSender.close()
            current.audioSender.close()
        }
    }

    suspend fun setMicrophoneMuted(muted: Boolean) {
        controls = controls.copy(microphoneMuted = muted)
        microphoneCapture.mute(muted)
    }

    suspend fun setVideoPaused(paused: Boolean) {
        controls = controls.copy(videoPaused = paused)
        videoCapture.pause(paused)
    }

    suspend fun flipCamera() {
        val next = if (controls.cameraFacing == CameraFacing.BACK) CameraFacing.FRONT else CameraFacing.BACK
        controls = controls.copy(cameraFacing = next)
        videoCapture.flip(next)
    }

    suspend fun setPreset(preset: QualityPreset) {
        controls = controls.copy(preset = preset)
        val settings = EncoderSettingsMapper.fromPreset(preset)
        videoCapture.start(controls.cameraFacing, settings.video)
        videoCapture.pause(controls.videoPaused)
        microphoneCapture.start(settings.audio)
        microphoneCapture.mute(controls.microphoneMuted)
    }

    suspend fun sendVideoNal(nalUnit: ByteArray, frameIndex: Long) {
        val current = session ?: return
        if (controls.videoPaused) return

        val timestamp = H264RtpPacketizer.timestampForFrame(frameIndex, current.videoFps)
        val packets = H264RtpPacketizer.packetizeNal(
            nalUnit = nalUnit,
            payloadType = current.videoPayloadType,
            timestamp = timestamp,
            ssrc = current.videoSsrc,
            sequence = videoSequence,
        )
        for (packet in packets) {
            current.videoSender.send(current.videoTarget, packet)
        }
    }

    suspend fun sendAudioFrame(opusFrame: ByteArray, packetIndex: Long) {
        val current = session ?: return
        if (controls.microphoneMuted) return

        val packet = OpusRtpPacketizer.packetizeFrame(
            opusFrame = opusFrame,
            payloadType = current.audioPayloadType,
            timestamp = OpusRtpPacketizer.timestampForPacket(packetIndex),
            ssrc = current.audioSsrc,
            sequence = audioSequence,
        )
        current.audioSender.send(current.audioTarget, packet)
    }

    private data class ActiveMediaSession(
        val sessionId: String,
        val videoTarget: RtpTarget,
        val audioTarget: RtpTarget,
        val videoPayloadType: Int,
        val audioPayloadType: Int,
        val videoSsrc: UInt,
        val audioSsrc: UInt,
        val videoFps: Int,
        val videoSender: RtpSender,
        val audioSender: RtpSender,
    )
}
