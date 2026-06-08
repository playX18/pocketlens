package com.acamera.android.app

import android.app.Application
import android.os.Build
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.acamera.android.control.HttpJsonControlClient
import com.acamera.android.discovery.AndroidNsdReceiverDiscovery
import com.acamera.android.foreground.StreamingForegroundService
import com.acamera.android.media.AndroidAudioRecordMicrophoneCaptureController
import com.acamera.android.media.AndroidCamera2VideoCaptureController
import com.acamera.android.media.EncodedAudioSink
import com.acamera.android.media.EncodedVideoSink
import com.acamera.android.media.SessionStreamCoordinator
import com.acamera.android.rtp.UdpRtpSender
import com.acamera.android.storage.SharedPreferencesTokenStorage

class ACameraViewModel(application: Application) : AndroidViewModel(application) {
    private val controller = ACameraController(
        scope = viewModelScope,
        controlClient = HttpJsonControlClient(),
        tokenStorage = SharedPreferencesTokenStorage(application),
        mediaSessionFactory = MediaSessionControllerFactory {
            createProductionMediaSession(application)
        },
        foregroundController = AndroidStreamingForegroundController(application),
        receiverDiscovery = AndroidNsdReceiverDiscovery(application),
        deviceNameProvider = { Build.MODEL ?: "Android" },
    )

    val uiState = controller.uiState

    init {
        controller.startDiscovery()
    }

    fun setManualHost(host: String) = controller.setManualHost(host)
    fun setManualPort(port: String) = controller.setManualPort(port)
    fun setPin(pin: String) = controller.setPin(pin)
    fun refreshDiscovery() = controller.refreshDiscovery()
    fun showManualConnect() = controller.showManualConnect()
    fun selectReceiver(index: Int) {
        uiState.value.discoveredReceivers.getOrNull(index)?.let(controller::selectReceiver)
    }
    fun connectReceiver(index: Int) {
        uiState.value.discoveredReceivers.getOrNull(index)?.let(controller::connectReceiver)
    }
    fun pair() = controller.pair()
    fun cancelPairing() = controller.cancelPairing()
    fun forgetReceiver() = controller.forgetReceiver()
    fun startSession() = controller.startSession()
    fun stopSession() = controller.stopSession()
    fun toggleMute() = controller.toggleMute()
    fun toggleVideoPaused() = controller.toggleVideoPaused()
    fun flipCamera() = controller.flipCamera()
    fun selectPreset(preset: com.acamera.android.protocol.QualityPreset) = controller.selectPreset(preset)

    override fun onCleared() {
        controller.stopDiscovery()
        super.onCleared()
    }
}

private class AndroidStreamingForegroundController(
    private val application: Application,
) : StreamingForegroundController {
    override fun start() = StreamingForegroundService.start(application)
    override fun stop() = StreamingForegroundService.stop(application)
}

private fun createProductionMediaSession(application: Application): MediaSessionController {
    lateinit var coordinator: SessionStreamCoordinator
    val videoSink = object : EncodedVideoSink {
        override suspend fun onH264NalUnit(nalUnit: ByteArray, presentationTimeUs: Long) {
            coordinator.sendVideoNal(nalUnit, frameIndex = presentationTimeUs / 33_333L)
        }
    }
    val audioSink = object : EncodedAudioSink {
        override suspend fun onOpusFrame(opusFrame: ByteArray, presentationTimeUs: Long) {
            coordinator.sendAudioFrame(opusFrame, packetIndex = presentationTimeUs / 20_000L)
        }
    }
    coordinator = SessionStreamCoordinator(
        videoSender = UdpRtpSender(),
        audioSender = UdpRtpSender(),
        videoCapture = AndroidCamera2VideoCaptureController(application, videoSink),
        microphoneCapture = AndroidAudioRecordMicrophoneCaptureController(application, audioSink),
    )
    return object : MediaSessionController {
        override suspend fun start(
            response: com.acamera.android.protocol.SessionStartResponse,
            controls: com.acamera.android.media.StreamControls,
        ) = coordinator.start(response, controls)

        override suspend fun stop() = coordinator.stop()
        override suspend fun setMicrophoneMuted(muted: Boolean) = coordinator.setMicrophoneMuted(muted)
        override suspend fun setVideoPaused(paused: Boolean) = coordinator.setVideoPaused(paused)
        override suspend fun flipCamera() = coordinator.flipCamera()
        override suspend fun setPreset(preset: com.acamera.android.protocol.QualityPreset) = coordinator.setPreset(preset)
    }
}
