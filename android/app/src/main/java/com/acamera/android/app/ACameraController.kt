package com.acamera.android.app

import com.acamera.android.control.ControlClient
import com.acamera.android.control.ReceiverEventStreamFactory
import com.acamera.android.control.WebSocketReceiverEventStream
import com.acamera.android.crypto.ACameraCrypto
import com.acamera.android.discovery.ReceiverAdvertisement
import com.acamera.android.discovery.ReceiverDiscovery
import com.acamera.android.media.StreamControls
import com.acamera.android.protocol.PairResponse
import com.acamera.android.protocol.QualityPreset
import com.acamera.android.protocol.ReceiverEvent
import com.acamera.android.protocol.SecurePairRequest
import com.acamera.android.protocol.SecurePairingStatus
import com.acamera.android.protocol.SessionStartRequest
import com.acamera.android.protocol.SessionStartResponse
import com.acamera.android.protocol.SessionStopRequest
import com.acamera.android.state.ControlsAction
import com.acamera.android.state.ControlsReducer
import com.acamera.android.state.ControlsState
import com.acamera.android.state.PairingAction
import com.acamera.android.state.PairingReducer
import com.acamera.android.state.PairingState
import com.acamera.android.state.SessionAction
import com.acamera.android.state.SessionReducer
import com.acamera.android.state.SessionState
import com.acamera.android.state.SessionStatus
import com.acamera.android.state.UiStep
import com.acamera.android.storage.TokenStorage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.delay
import kotlinx.serialization.decodeFromString

data class ACameraUiState(
    val discoveredReceivers: List<ReceiverAdvertisement> = emptyList(),
    val manualHost: String = "192.168.100.128",
    val manualPort: String = "3769",
    val manualConnectVisible: Boolean = false,
    val manualConnectSuggested: Boolean = false,
    val discoveryRefreshing: Boolean = true,
    val pin: String = "",
    val pairing: PairingState = PairingState(),
    val session: SessionState = SessionState(),
    val controls: ControlsState = ControlsState(),
    val statusMessage: String = "Searching…",
    val errorMessage: String? = null,
) {
    val selectedReceiver: ReceiverAdvertisement?
        get() = pairing.selectedReceiver ?: if (manualConnectVisible) manualReceiverOrNull() else null

    val canPair: Boolean
        get() = selectedReceiver != null && !pairing.inFlight

    val canStart: Boolean
        get() = pairing.token != null && session.status in setOf(SessionStatus.IDLE, SessionStatus.ERROR)

    val canStop: Boolean
        get() = session.status in setOf(SessionStatus.ACTIVE, SessionStatus.RECONNECTING)

    fun currentStep(): UiStep = when {
        pairing.inFlight -> UiStep.Pairing
        pairing.token != null -> UiStep.Stream
        else -> UiStep.FindPc
    }

    fun baseUrl(): String? {
        val receiver = selectedReceiver ?: return null
        val host = receiver.host ?: return null
        return "http://$host:${receiver.controlPort}"
    }

    private fun manualReceiverOrNull(): ReceiverAdvertisement? {
        val port = manualPort.toIntOrNull()?.takeIf { it in 1..65535 } ?: return null
        val host = manualHost.trim().takeIf { it.isNotBlank() } ?: return null
        return ReceiverAdvertisement(
            receiverName = host,
            protocolVersion = 1,
            controlPort = port,
            capabilities = setOf("h264", "opus", "rtp"),
            host = host,
        )
    }
}

interface MediaSessionController {
    suspend fun start(response: SessionStartResponse, controls: StreamControls)
    suspend fun stop()
    suspend fun setMicrophoneMuted(muted: Boolean)
    suspend fun setVideoPaused(paused: Boolean)
    suspend fun flipCamera()
    suspend fun setPreset(preset: QualityPreset)
}

fun interface MediaSessionControllerFactory {
    fun create(): MediaSessionController
}

interface StreamingForegroundController {
    fun start()
    fun stop()
}

object NoopStreamingForegroundController : StreamingForegroundController {
    override fun start() = Unit
    override fun stop() = Unit
}

class ACameraController(
    private val scope: CoroutineScope,
    private val controlClient: ControlClient,
    private val tokenStorage: TokenStorage,
    private val mediaSessionFactory: MediaSessionControllerFactory,
    private val foregroundController: StreamingForegroundController = NoopStreamingForegroundController,
    private val receiverDiscovery: ReceiverDiscovery? = null,
    private val eventStreamFactory: ReceiverEventStreamFactory = ReceiverEventStreamFactory { WebSocketReceiverEventStream(it) },
    private val deviceNameProvider: () -> String = { "Android" },
    private val pinProvider: () -> String = { (0..999_999).random().toString().padStart(6, '0') },
) {
    private val _uiState = MutableStateFlow(ACameraUiState())
    private var mediaSession: MediaSessionController? = null
    private var discoveryJob: Job? = null
    private var discoveryTimerJob: Job? = null
    private var eventsJob: Job? = null
    private var pairingJob: Job? = null

    val uiState: StateFlow<ACameraUiState> = _uiState.asStateFlow()

    fun startDiscovery() {
        beginDiscoveryWindow()
        receiverDiscovery?.start()
        if (receiverDiscovery != null && discoveryJob == null) {
            discoveryJob = scope.launch {
                receiverDiscovery.advertisements.collectLatest { receivers ->
                    _uiState.update {
                        it.copy(
                            discoveredReceivers = receivers,
                            discoveryRefreshing = receivers.isEmpty() && it.discoveryRefreshing,
                            statusMessage = if (receivers.isNotEmpty() && it.pairing.token == null) {
                                "Select a PC"
                            } else {
                                it.statusMessage
                            },
                        )
                    }
                }
            }
        }
    }

    fun stopDiscovery() {
        receiverDiscovery?.stop()
        discoveryJob?.cancel()
        discoveryJob = null
        discoveryTimerJob?.cancel()
        discoveryTimerJob = null
    }

    fun refreshDiscovery() {
        receiverDiscovery?.stop()
        receiverDiscovery?.start()
        beginDiscoveryWindow()
    }

    fun showManualConnect() {
        _uiState.update {
            it.copy(
                manualConnectVisible = true,
                manualConnectSuggested = false,
                pairing = PairingReducer.reduce(it.pairing, PairingAction.Reset),
                statusMessage = "Enter PC address",
                errorMessage = null,
            )
        }
    }

    fun setManualHost(host: String) {
        _uiState.update {
            it.copy(
                manualHost = host.trim(),
                pairing = if (host.isNotBlank()) {
                    PairingReducer.reduce(it.pairing, PairingAction.Reset)
                } else {
                    it.pairing
                },
                errorMessage = null,
            )
        }
    }

    fun setManualPort(port: String) {
        _uiState.update { it.copy(manualPort = port.filter(Char::isDigit).take(5), errorMessage = null) }
    }

    fun setPin(pin: String) {
        _uiState.update { it.copy(pin = pin.filter(Char::isDigit).take(8), errorMessage = null) }
    }

    fun selectReceiver(receiver: ReceiverAdvertisement) {
        _uiState.update {
            it.copy(
                pairing = PairingReducer.reduce(it.pairing, PairingAction.SelectReceiver(receiver)),
                manualConnectVisible = false,
                manualHost = "",
                errorMessage = null,
                statusMessage = "Selected ${receiver.receiverName}",
            )
        }
    }

    fun connectReceiver(receiver: ReceiverAdvertisement) {
        selectReceiver(receiver)
        pair()
    }

    fun cancelPairing() {
        pairingJob?.cancel()
        pairingJob = null
        _uiState.update {
            it.copy(
                pairing = PairingReducer.reduce(it.pairing, PairingAction.Reset),
                pin = "",
                errorMessage = null,
                statusMessage = "Searching…",
            )
        }
        beginDiscoveryWindow()
    }

    fun forgetReceiver() {
        scope.launch {
            forgetReceiverInternal()
        }
    }

    private suspend fun forgetReceiverInternal() {
        val snapshot = uiState.value
        val receiverName = snapshot.pairing.selectedReceiver?.receiverName
        if (snapshot.session.status in setOf(SessionStatus.ACTIVE, SessionStatus.RECONNECTING, SessionStatus.STARTING)) {
            stopSessionInternal()
        }
        pairingJob?.cancel()
        pairingJob = null
        receiverName?.let { tokenStorage.clearToken(it) }
        _uiState.update {
            it.copy(
                pairing = PairingReducer.reduce(it.pairing, PairingAction.Reset),
                session = SessionState(),
                pin = "",
                manualConnectVisible = false,
                errorMessage = null,
                statusMessage = "Searching…",
            )
        }
        beginDiscoveryWindow()
    }

    fun pair() {
        val snapshot = uiState.value
        val receiver = snapshot.selectedReceiver
        val baseUrl = snapshot.baseUrl()
        if (receiver == null || baseUrl == null) {
            _uiState.update { it.copy(errorMessage = "Enter a receiver host or select a discovered receiver.") }
            return
        }
        pairingJob?.cancel()
        pairingJob = scope.launch {
            val pairingWithReceiver = if (snapshot.pairing.selectedReceiver == null) {
                PairingReducer.reduce(snapshot.pairing, PairingAction.SelectReceiver(receiver))
            } else {
                snapshot.pairing
            }
            _uiState.update {
                it.copy(
                    pairing = PairingReducer.reduce(pairingWithReceiver, PairingAction.SubmitPin),
                    pin = "",
                    errorMessage = null,
                    statusMessage = "Connecting to ${receiver.receiverName}…",
                )
            }
            val pairingId = ACameraCrypto.randomHex(16)
            val phoneNonce = ACameraCrypto.randomHex(16)
            val phonePublicKey = ACameraCrypto.randomHex(32)
            val phonePin = pinProvider()
            runCatching {
                val requested = controlClient.requestSecurePairing(
                    baseUrl,
                    SecurePairRequest(
                        pairingId = pairingId,
                        deviceName = deviceNameProvider(),
                        phoneNonce = phoneNonce,
                        phonePublicKey = phonePublicKey,
                    ),
                )
                _uiState.update {
                    it.copy(pin = phonePin)
                }
                val key = ACameraCrypto.securePairingKey(
                    pin = phonePin,
                    pairingId = pairingId,
                    phoneNonce = phoneNonce,
                    receiverNonce = requested.receiverNonce,
                    phonePublicKey = phonePublicKey,
                    receiverPublicKey = requested.receiverPublicKey,
                )
                var response: PairResponse? = null
                var attempts = 0
                while (response == null && attempts < 300) {
                    val result = controlClient.securePairingResult(baseUrl, pairingId)
                    when (result.status) {
                        SecurePairingStatus.APPROVED -> {
                            val encrypted = result.encryptedResult ?: error("approved pairing result was empty")
                            val plaintext = ACameraCrypto.decryptFromHex(key, pairingId.encodeToByteArray(), encrypted)
                            response = com.acamera.android.protocol.ACameraJson.instance.decodeFromString<PairResponse>(
                                plaintext.decodeToString(),
                            )
                        }
                        SecurePairingStatus.EXPIRED -> error("Pairing PIN expired.")
                        SecurePairingStatus.REJECTED -> error("Pairing was rejected on the PC.")
                        SecurePairingStatus.PENDING -> delay(1_000)
                    }
                    attempts += 1
                }
                response ?: error("Timed out waiting for PC approval.")
            }.onSuccess { response ->
                tokenStorage.writeToken(response.receiverName, response.sessionToken)
                _uiState.update {
                    it.copy(
                        pairing = PairingReducer.reduce(it.pairing, PairingAction.PairingSucceeded(response.sessionToken)),
                        statusMessage = "Connected to ${response.receiverName}",
                        errorMessage = null,
                    )
                }
            }.onFailure { error ->
                if (error is kotlinx.coroutines.CancellationException) {
                    throw error
                }
                val message = error.message ?: "Pairing failed"
                _uiState.update {
                    it.copy(
                        pairing = PairingReducer.reduce(it.pairing, PairingAction.PairingFailed(message)),
                        errorMessage = message,
                        statusMessage = "Pairing failed",
                    )
                }
            }
        }
    }

    fun startSession() {
        scope.launch {
            startSessionInternal(uiState.value.controls.preset, "Starting…")
        }
    }

    fun stopSession() {
        scope.launch {
            stopSessionInternal()
        }
    }

    private suspend fun stopSessionInternal() {
        val snapshot = uiState.value
        val baseUrl = snapshot.baseUrl()
        val token = snapshot.pairing.token
        val sessionId = snapshot.session.sessionId
        _uiState.update {
            it.copy(
                session = SessionReducer.reduce(it.session, SessionAction.StopRequested),
                statusMessage = "Stopping…",
                errorMessage = null,
            )
        }
        runCatching {
            stopReceiverEvents()
            if (baseUrl != null && token != null && sessionId != null) {
                runCatching {
                    controlClient.stopSession(baseUrl, SessionStopRequest(token, sessionId))
                }
            }
            runCatching {
                mediaSession?.stop()
            }
            foregroundController.stop()
        }.onSuccess {
            mediaSession = null
            _uiState.update {
                it.copy(
                    session = SessionReducer.reduce(it.session, SessionAction.StopSucceeded),
                    statusMessage = "Stopped",
                )
            }
        }.onFailure { error ->
            val message = error.message ?: "Session stop failed"
            _uiState.update { it.copy(errorMessage = message, statusMessage = "Stop failed") }
        }
    }

    fun toggleMute() {
        _uiState.update { it.copy(controls = ControlsReducer.reduce(it.controls, ControlsAction.ToggleMute)) }
        scope.launch {
            mediaSession?.setMicrophoneMuted(uiState.value.controls.microphoneMuted)
        }
    }

    fun toggleVideoPaused() {
        _uiState.update { it.copy(controls = ControlsReducer.reduce(it.controls, ControlsAction.ToggleVideoPaused)) }
        scope.launch {
            mediaSession?.setVideoPaused(uiState.value.controls.videoPaused)
        }
    }

    fun flipCamera() {
        _uiState.update { it.copy(controls = ControlsReducer.reduce(it.controls, ControlsAction.FlipCamera)) }
        scope.launch {
            mediaSession?.flipCamera()
        }
    }

    fun selectPreset(preset: QualityPreset) {
        val wasActive = uiState.value.session.status == SessionStatus.ACTIVE
        _uiState.update {
            it.copy(controls = ControlsReducer.reduce(it.controls, ControlsAction.SelectPreset(preset)))
        }
        scope.launch {
            if (wasActive) {
                restartSessionWithPreset(preset)
            } else {
                mediaSession?.setPreset(preset)
            }
        }
    }

    fun applyReceiverEvent(event: ReceiverEvent) {
        _uiState.update { it.copy(session = SessionReducer.reduce(it.session, SessionAction.EventReceived(event))) }
    }

    private fun startReceiverEvents(baseUrl: String, token: String, sessionId: String) {
        stopReceiverEvents()
        val streamUrl = controlClient.eventsUrl(baseUrl, token, sessionId)
        eventsJob = scope.launch {
            runCatching {
                eventStreamFactory.create(streamUrl).collect { event ->
                    val currentSessionId = uiState.value.session.sessionId ?: sessionId
                    if (event.belongsToSession(currentSessionId)) {
                        applyReceiverEvent(event)
                    }
                }
            }.onFailure { error ->
                if (error !is kotlinx.coroutines.CancellationException) {
                    _uiState.update {
                        it.copy(
                            session = SessionReducer.reduce(it.session, SessionAction.HeartbeatTimedOut),
                            errorMessage = error.message ?: "Receiver event stream disconnected",
                        )
                    }
                }
            }
        }
    }

    private fun stopReceiverEvents() {
        eventsJob?.cancel()
        eventsJob = null
    }

    private fun ReceiverEvent.belongsToSession(sessionId: String): Boolean =
        when (this) {
            is ReceiverEvent.Stats -> this.sessionId == sessionId
            is ReceiverEvent.Warning -> this.sessionId == null || this.sessionId == sessionId
            is ReceiverEvent.Error -> this.sessionId == null || this.sessionId == sessionId
        }

    private fun ControlsState.toStreamControls(): StreamControls =
        StreamControls(
            cameraFacing = cameraFacing,
            microphoneMuted = microphoneMuted,
            videoPaused = videoPaused,
            preset = preset,
        )

    private fun beginDiscoveryWindow() {
        discoveryTimerJob?.cancel()
        _uiState.update {
            it.copy(
                discoveryRefreshing = true,
                manualConnectSuggested = false,
                errorMessage = null,
                statusMessage = if (it.pairing.token == null) "Searching…" else it.statusMessage,
            )
        }
        discoveryTimerJob = scope.launch {
            delay(10_000)
            _uiState.update {
                if (it.discoveredReceivers.isEmpty() && it.pairing.token == null && !it.manualConnectVisible) {
                    it.copy(
                        discoveryRefreshing = false,
                        manualConnectSuggested = true,
                    )
                } else {
                    it.copy(discoveryRefreshing = false)
                }
            }
        }
    }

    private suspend fun restartSessionWithPreset(preset: QualityPreset) {
        val snapshot = uiState.value
        val baseUrl = snapshot.baseUrl()
        val token = snapshot.pairing.token
        val sessionId = snapshot.session.sessionId
        if (baseUrl == null || token == null || sessionId == null) {
            _uiState.update { it.copy(errorMessage = "Pair before changing quality.") }
            return
        }

        _uiState.update {
            it.copy(
                session = SessionReducer.reduce(it.session, SessionAction.StartRequested(preset)),
                statusMessage = "Restarting…",
                errorMessage = null,
            )
        }
        runCatching {
            stopReceiverEvents()
            runCatching {
                controlClient.stopSession(baseUrl, SessionStopRequest(token, sessionId))
            }
            runCatching {
                mediaSession?.stop()
            }
            mediaSession = null
            foregroundController.stop()
        }.onFailure { error ->
            val message = error.message ?: "Quality change failed"
            _uiState.update {
                it.copy(
                    session = SessionReducer.reduce(it.session, SessionAction.StartFailed(message)),
                    errorMessage = message,
                    statusMessage = "Quality change failed",
                )
            }
            return
        }

        startSessionInternal(preset, "Restarting stream...")
    }

    private suspend fun startSessionInternal(preset: QualityPreset, statusMessage: String) {
        val snapshot = uiState.value
        val baseUrl = snapshot.baseUrl()
        val token = snapshot.pairing.token
        if (baseUrl == null || token == null) {
            _uiState.update { it.copy(errorMessage = "Pair before starting a session.") }
            return
        }

        var startedResponse: SessionStartResponse? = null
        _uiState.update {
            it.copy(
                session = SessionReducer.reduce(it.session, SessionAction.StartRequested(preset)),
                errorMessage = null,
                statusMessage = statusMessage,
            )
        }
        runCatching {
            val response = controlClient.startSession(
                baseUrl,
                SessionStartRequest(sessionToken = token, qualityPreset = preset),
            )
            startedResponse = response
            val coordinator = mediaSessionFactory.create()
            foregroundController.start()
            coordinator.start(response, snapshot.controls.copy(preset = preset).toStreamControls())
            mediaSession = coordinator
            startReceiverEvents(baseUrl, token, response.sessionId)
            response
        }.onSuccess { response ->
            _uiState.update {
                it.copy(
                    session = SessionReducer.reduce(it.session, SessionAction.StartSucceeded(response)),
                    statusMessage = "Live",
                    errorMessage = null,
                )
            }
        }.onFailure { error ->
            val message = error.message ?: "Session start failed"
            stopReceiverEvents()
            startedResponse?.let { response ->
                runCatching {
                    controlClient.stopSession(baseUrl, SessionStopRequest(token, response.sessionId))
                }
            }
            runCatching {
                mediaSession?.stop()
            }
            foregroundController.stop()
            mediaSession = null
            _uiState.update {
                it.copy(
                    session = SessionReducer.reduce(it.session, SessionAction.StartFailed(message)),
                    errorMessage = message,
                    statusMessage = "Start failed",
                )
            }
        }
    }
}
