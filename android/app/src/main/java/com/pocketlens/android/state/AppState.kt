package com.pocketlens.android.state

import com.pocketlens.android.discovery.ReceiverAdvertisement
import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.protocol.ReceiverEvent
import com.pocketlens.android.protocol.SessionStartResponse

enum class UiStep {
    FindPc,
    Pairing,
    Stream,
}

enum class AppDestination {
    DISCOVERY,
    LIVE,
}

object AppStartReducer {
    fun initialDestination(storedToken: String?): AppDestination =
        if (storedToken.isNullOrBlank()) AppDestination.DISCOVERY else AppDestination.LIVE
}

enum class PermissionDecision {
    GRANTED,
    DENIED,
    PERMANENTLY_DENIED,
}

enum class PermissionState {
    READY,
    NEEDS_PERMISSION,
    BLOCKED,
}

data class PermissionSnapshot(
    val camera: PermissionDecision,
    val microphone: PermissionDecision,
)

object PermissionReducer {
    fun reduce(snapshot: PermissionSnapshot): PermissionState =
        when {
            snapshot.camera == PermissionDecision.PERMANENTLY_DENIED ||
                snapshot.microphone == PermissionDecision.PERMANENTLY_DENIED -> PermissionState.BLOCKED
            snapshot.camera == PermissionDecision.GRANTED &&
                snapshot.microphone == PermissionDecision.GRANTED -> PermissionState.READY
            else -> PermissionState.NEEDS_PERMISSION
        }
}

data class PairingState(
    val selectedReceiver: ReceiverAdvertisement? = null,
    val inFlight: Boolean = false,
    val token: String? = null,
    val error: String? = null,
)

sealed interface PairingAction {
    data class SelectReceiver(val receiver: ReceiverAdvertisement) : PairingAction
    data object SubmitPin : PairingAction
    data class PairingSucceeded(val token: String) : PairingAction
    data class PairingFailed(val message: String) : PairingAction
    data object Reset : PairingAction
}

object PairingReducer {
    fun reduce(state: PairingState, action: PairingAction): PairingState =
        when (action) {
            is PairingAction.SelectReceiver -> PairingState(selectedReceiver = action.receiver)
            PairingAction.SubmitPin -> state.copy(inFlight = true, error = null)
            is PairingAction.PairingSucceeded -> state.copy(inFlight = false, token = action.token, error = null)
            is PairingAction.PairingFailed -> state.copy(inFlight = false, error = action.message)
            PairingAction.Reset -> PairingState()
        }
}

data class SessionState(
    val status: SessionStatus = SessionStatus.IDLE,
    val sessionId: String? = null,
    val preset: QualityPreset = QualityPreset.BALANCED,
    val stats: StreamStats? = null,
    val warning: String? = null,
    val error: String? = null,
)

enum class SessionStatus {
    IDLE,
    STARTING,
    ACTIVE,
    RECONNECTING,
    STOPPING,
    ERROR,
}

data class StreamStats(
    val videoBitrateKbps: Int,
    val audioBitrateKbps: Int,
    val packetLossPercent: Double,
)

sealed interface SessionAction {
    data class StartRequested(val preset: QualityPreset) : SessionAction
    data class StartSucceeded(val response: SessionStartResponse) : SessionAction
    data class StartFailed(val message: String) : SessionAction
    data object StopRequested : SessionAction
    data object StopSucceeded : SessionAction
    data object HeartbeatTimedOut : SessionAction
    data class EventReceived(val event: ReceiverEvent) : SessionAction
}

object SessionReducer {
    fun reduce(state: SessionState, action: SessionAction): SessionState =
        when (action) {
            is SessionAction.StartRequested -> state.copy(
                status = SessionStatus.STARTING,
                preset = action.preset,
                warning = null,
                error = null,
            )
            is SessionAction.StartSucceeded -> state.copy(
                status = SessionStatus.ACTIVE,
                sessionId = action.response.sessionId,
                preset = action.response.qualityPreset,
                warning = null,
                error = null,
            )
            is SessionAction.StartFailed -> state.copy(status = SessionStatus.ERROR, error = action.message)
            SessionAction.StopRequested -> state.copy(status = SessionStatus.STOPPING)
            SessionAction.StopSucceeded -> SessionState()
            SessionAction.HeartbeatTimedOut -> state.copy(status = SessionStatus.RECONNECTING, warning = "Receiver heartbeat timed out")
            is SessionAction.EventReceived -> reduceEvent(state, action.event)
        }

    private fun reduceEvent(state: SessionState, event: ReceiverEvent): SessionState =
        when (event) {
            is ReceiverEvent.Stats -> state.copy(
                stats = StreamStats(
                    videoBitrateKbps = event.estimatedBitrateKbps,
                    audioBitrateKbps = 0,
                    packetLossPercent = packetLossPercent(event),
                ),
                preset = event.qualityPreset,
            )
            is ReceiverEvent.Warning -> state.copy(warning = event.message)
            is ReceiverEvent.Error -> state.copy(
                status = SessionStatus.ERROR,
                error = event.message,
            )
        }

    private fun packetLossPercent(event: ReceiverEvent.Stats): Double {
        val totalPackets = event.videoPackets + event.audioPackets
        if (totalPackets == 0L) return 0.0
        val lostPackets = event.videoPacketsLost + event.audioPacketsLost
        return lostPackets.toDouble() / totalPackets.toDouble() * 100.0
    }
}

data class ControlsState(
    val cameraFacing: CameraFacing = CameraFacing.BACK,
    val microphoneMuted: Boolean = false,
    val videoPaused: Boolean = false,
    val preset: QualityPreset = QualityPreset.BALANCED,
)

enum class CameraFacing {
    FRONT,
    BACK,
}

sealed interface ControlsAction {
    data object FlipCamera : ControlsAction
    data object ToggleMute : ControlsAction
    data object ToggleVideoPaused : ControlsAction
    data class SelectPreset(val preset: QualityPreset) : ControlsAction
}

object ControlsReducer {
    fun reduce(state: ControlsState, action: ControlsAction): ControlsState =
        when (action) {
            ControlsAction.FlipCamera -> state.copy(
                cameraFacing = if (state.cameraFacing == CameraFacing.BACK) CameraFacing.FRONT else CameraFacing.BACK,
            )
            ControlsAction.ToggleMute -> state.copy(microphoneMuted = !state.microphoneMuted)
            ControlsAction.ToggleVideoPaused -> state.copy(videoPaused = !state.videoPaused)
            is ControlsAction.SelectPreset -> state.copy(preset = action.preset)
        }
}
