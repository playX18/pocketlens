package com.pocketlens.android.state

import com.pocketlens.android.app.PocketLensUiState
import com.pocketlens.android.discovery.ReceiverAdvertisement
import com.pocketlens.android.protocol.AudioConfig
import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.protocol.ReceiverEvent
import com.pocketlens.android.protocol.SessionStartResponse
import com.pocketlens.android.protocol.VideoConfig
import kotlin.test.Test
import kotlin.test.assertEquals

class ReducersTest {
    @Test
    fun appStartsInDiscoveryWhenNoTokenExists() {
        assertEquals(AppDestination.DISCOVERY, AppStartReducer.initialDestination(null))
        assertEquals(AppDestination.DISCOVERY, AppStartReducer.initialDestination(""))
        assertEquals(AppDestination.LIVE, AppStartReducer.initialDestination("token"))
    }

    @Test
    fun permissionReducerMapsGrantDenialAndBlockedStates() {
        assertEquals(
            PermissionState.READY,
            PermissionReducer.reduce(
                PermissionSnapshot(PermissionDecision.GRANTED, PermissionDecision.GRANTED),
            ),
        )
        assertEquals(
            PermissionState.NEEDS_PERMISSION,
            PermissionReducer.reduce(
                PermissionSnapshot(PermissionDecision.GRANTED, PermissionDecision.DENIED),
            ),
        )
        assertEquals(
            PermissionState.BLOCKED,
            PermissionReducer.reduce(
                PermissionSnapshot(PermissionDecision.PERMANENTLY_DENIED, PermissionDecision.GRANTED),
            ),
        )
    }

    @Test
    fun pairingReducerHandlesSuccessAndFailure() {
        val receiver = ReceiverAdvertisement("Receiver", 1, 3769, setOf("h264"))
        val selected = PairingReducer.reduce(PairingState(), PairingAction.SelectReceiver(receiver))
        val submitting = PairingReducer.reduce(selected, PairingAction.SubmitPin)
        val failed = PairingReducer.reduce(submitting, PairingAction.PairingFailed("wrong PIN"))
        val succeeded = PairingReducer.reduce(submitting, PairingAction.PairingSucceeded("token-123"))

        assertEquals(receiver, selected.selectedReceiver)
        assertEquals(true, submitting.inFlight)
        assertEquals("wrong PIN", failed.error)
        assertEquals("token-123", succeeded.token)
        assertEquals(false, succeeded.inFlight)
    }

    @Test
    fun sessionReducerMovesThroughStartStatsWarningAndFatalError() {
        val starting = SessionReducer.reduce(
            SessionState(),
            SessionAction.StartRequested(QualityPreset.HIGH),
        )
        val active = SessionReducer.reduce(
            starting,
            SessionAction.StartSucceeded(
                SessionStartResponse(
                    sessionId = "session-123",
                    receiverHost = "192.168.1.25",
                    videoRtpPort = 5004,
                    audioRtpPort = 5006,
                    videoPayloadType = 96,
                    audioPayloadType = 97,
                    ssrcVideo = 11L,
                    ssrcAudio = 12L,
                    qualityPreset = QualityPreset.BALANCED,
                    video = VideoConfig(),
                    audio = AudioConfig(),
                ),
            ),
        )
        val withStats = SessionReducer.reduce(
            active,
            SessionAction.EventReceived(
                ReceiverEvent.Stats(
                    sessionId = "session-123",
                    videoPackets = 1_000,
                    audioPackets = 1_000,
                    videoPacketsLost = 4,
                    audioPacketsLost = 1,
                    estimatedBitrateKbps = 2_500,
                    qualityPreset = QualityPreset.BALANCED,
                ),
            ),
        )
        val warning = SessionReducer.reduce(
            withStats,
            SessionAction.HeartbeatTimedOut,
        )
        val failed = SessionReducer.reduce(
            warning,
            SessionAction.EventReceived(
                ReceiverEvent.Error("session-123", "media_pipeline_failed", "stopped"),
            ),
        )

        assertEquals(SessionStatus.STARTING, starting.status)
        assertEquals(SessionStatus.ACTIVE, active.status)
        assertEquals("session-123", active.sessionId)
        assertEquals(2500, withStats.stats?.videoBitrateKbps)
        assertEquals(SessionStatus.RECONNECTING, warning.status)
        assertEquals(SessionStatus.ERROR, failed.status)
        assertEquals("stopped", failed.error)
    }

    @Test
    fun controlsReducerHandlesCameraMutePauseAndPreset() {
        val flipped = ControlsReducer.reduce(ControlsState(), ControlsAction.FlipCamera)
        val muted = ControlsReducer.reduce(flipped, ControlsAction.ToggleMute)
        val paused = ControlsReducer.reduce(muted, ControlsAction.ToggleVideoPaused)
        val high = ControlsReducer.reduce(paused, ControlsAction.SelectPreset(QualityPreset.HIGH))

        assertEquals(CameraFacing.FRONT, flipped.cameraFacing)
        assertEquals(true, muted.microphoneMuted)
        assertEquals(true, paused.videoPaused)
        assertEquals(QualityPreset.HIGH, high.preset)
    }

    @Test
    fun uiStepFindPcWhenNotPairingAndNoToken() {
        assertEquals(UiStep.FindPc, PocketLensUiState().currentStep())
    }

    @Test
    fun uiStepPairingWhenPairingInFlight() {
        val state = PocketLensUiState(
            pairing = PairingState(inFlight = true),
        )
        assertEquals(UiStep.Pairing, state.currentStep())
    }

    @Test
    fun uiStepStreamWhenTokenExists() {
        val state = PocketLensUiState(
            pairing = PairingState(token = "token-123"),
        )
        assertEquals(UiStep.Stream, state.currentStep())
    }

    @Test
    fun uiStepPairingTakesPrecedenceOverToken() {
        val state = PocketLensUiState(
            pairing = PairingState(inFlight = true, token = "stale-token"),
        )
        assertEquals(UiStep.Pairing, state.currentStep())
    }
}
