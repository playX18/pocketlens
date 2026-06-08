package com.pocketlens.android.app

import com.pocketlens.android.control.ControlClient
import com.pocketlens.android.control.ReceiverEventStream
import com.pocketlens.android.control.ReceiverEventStreamFactory
import com.pocketlens.android.crypto.PocketLensCrypto
import com.pocketlens.android.discovery.ReceiverAdvertisement
import com.pocketlens.android.discovery.ReceiverDiscovery
import com.pocketlens.android.media.StreamControls
import com.pocketlens.android.protocol.AudioConfig
import com.pocketlens.android.protocol.PairRequest
import com.pocketlens.android.protocol.PairResponse
import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.protocol.ReceiverCapabilities
import com.pocketlens.android.protocol.ReceiverEvent
import com.pocketlens.android.protocol.ReceiverStatus
import com.pocketlens.android.protocol.SecurePairRequest
import com.pocketlens.android.protocol.SecurePairRequestResponse
import com.pocketlens.android.protocol.SecurePairResultResponse
import com.pocketlens.android.protocol.SecurePairingStatus
import com.pocketlens.android.protocol.SessionStartRequest
import com.pocketlens.android.protocol.SessionStartResponse
import com.pocketlens.android.protocol.SessionStopRequest
import com.pocketlens.android.protocol.VideoConfig
import com.pocketlens.android.protocol.VirtualDevice
import com.pocketlens.android.protocol.VirtualDevices
import com.pocketlens.android.state.CameraFacing
import com.pocketlens.android.state.SessionStatus
import com.pocketlens.android.storage.InMemoryTokenStorage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.encodeToString
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class PocketLensControllerTest {
    @Test
    fun discoverySelectionPairAndSessionStartWireControlClientToMediaSession() = runBlocking {
        val receiver = ReceiverAdvertisement("Desk", 1, 3769, setOf("h264", "opus", "rtp"), "127.0.0.1")
        val discovery = FakeDiscovery(listOf(receiver))
        val client = FakeControlClient()
        val mediaSession = RecordingMediaSession()
        val controller = PocketLensController(
            scope = CoroutineScope(Dispatchers.Unconfined),
            controlClient = client,
            tokenStorage = InMemoryTokenStorage(),
            mediaSessionFactory = MediaSessionControllerFactory { mediaSession },
            receiverDiscovery = discovery,
            deviceNameProvider = { "Pixel" },
            pinProvider = { "123456" },
        )

        controller.startDiscovery()
        controller.connectReceiver(receiver)
        controller.startSession()

        val state = controller.uiState.value
        assertEquals("http://127.0.0.1:3769", client.baseUrls.distinct().single())
        assertEquals("Pixel", client.securePairRequest?.deviceName)
        assertEquals("token-abc123", client.startRequest?.sessionToken)
        assertEquals(SessionStatus.ACTIVE, state.session.status)
        assertEquals("session-abc123", state.session.sessionId)
        assertNotNull(mediaSession.startedWith)
        assertEquals(CameraFacing.BACK, mediaSession.controls?.cameraFacing)
    }

    @Test
    fun manualHostCanPairWhenMdnsIsUnavailable() = runBlocking {
        val client = FakeControlClient()
        val controller = PocketLensController(
            scope = CoroutineScope(Dispatchers.Unconfined),
            controlClient = client,
            tokenStorage = InMemoryTokenStorage(),
            mediaSessionFactory = MediaSessionControllerFactory { RecordingMediaSession() },
            deviceNameProvider = { "Pixel" },
            pinProvider = { "123456" },
        )

        assertEquals(false, controller.uiState.value.canPair)
        controller.showManualConnect()
        controller.setManualHost("192.168.1.20")
        controller.setManualPort("4777")
        controller.setPin("9876")
        assertTrue(controller.uiState.value.canPair)

        controller.pair()

        assertEquals("http://192.168.1.20:4777", client.baseUrls.distinct().single())
        assertEquals("token-abc123", controller.uiState.value.pairing.token)
    }

    @Test
    fun controlsUpdateActiveMediaSession() = runBlocking {
        val mediaSession = RecordingMediaSession()
        val client = FakeControlClient()
        val controller = pairedStartedController(mediaSession, client)

        controller.toggleMute()
        controller.toggleVideoPaused()
        controller.flipCamera()
        controller.selectPreset(QualityPreset.HIGH)

        assertEquals(true, mediaSession.muted)
        assertEquals(true, mediaSession.videoPaused)
        assertEquals(1, mediaSession.flipCount)
        assertEquals(null, mediaSession.preset)
        assertEquals(true, mediaSession.stopped)
        assertEquals(SessionStopRequest("token-abc123", "session-abc123"), client.stopRequest)
        assertEquals(QualityPreset.HIGH, client.startRequest?.qualityPreset)
        assertEquals(QualityPreset.HIGH, mediaSession.startedWith?.qualityPreset)
        assertEquals(QualityPreset.HIGH, mediaSession.controls?.preset)
    }

    @Test
    fun stopSessionStopsControlAndMediaSession() = runBlocking {
        val client = FakeControlClient()
        val mediaSession = RecordingMediaSession()
        val controller = pairedStartedController(mediaSession, client)

        controller.stopSession()

        assertEquals(SessionStopRequest("token-abc123", "session-abc123"), client.stopRequest)
        assertEquals(true, mediaSession.stopped)
        assertEquals(SessionStatus.IDLE, controller.uiState.value.session.status)
    }

    @Test
    fun startSessionConsumesReceiverEventsWithSessionIdUrl() = runBlocking {
        val eventStreamFactory = RecordingEventStreamFactory(
            ReceiverEvent.Stats(
                sessionId = "session-abc123",
                videoPackets = 40,
                audioPackets = 60,
                videoPacketsLost = 1,
                audioPacketsLost = 0,
                estimatedBitrateKbps = 2_200,
                qualityPreset = QualityPreset.BALANCED,
            ),
            ReceiverEvent.Warning(
                sessionId = "other-session",
                code = "ignored",
                message = "Ignored warning",
            ),
        )
        val controller = pairedStartedController(
            mediaSession = RecordingMediaSession(),
            eventStreamFactory = eventStreamFactory,
        )

        val state = controller.uiState.value
        assertEquals(
            "http://127.0.0.1:3769/session/events?session_token=token-abc123&session_id=session-abc123",
            eventStreamFactory.urls.single(),
        )
        assertEquals(2_200, state.session.stats?.videoBitrateKbps)
        assertEquals(null, state.session.warning)
    }

    @Test
    fun stopSessionCancelsReceiverEventStream() = runBlocking {
        val stopped = CompletableDeferred<Unit>()
        val eventStreamFactory = ReceiverEventStreamFactory { url ->
            object : ReceiverEventStream {
                override suspend fun collect(onEvent: suspend (ReceiverEvent) -> Unit) {
                    assertEquals(
                        "http://127.0.0.1:3769/session/events?session_token=token-abc123&session_id=session-abc123",
                        url,
                    )
                    try {
                        awaitCancellation()
                    } finally {
                        stopped.complete(Unit)
                    }
                }
            }
        }
        val controller = pairedStartedController(
            mediaSession = RecordingMediaSession(),
            eventStreamFactory = eventStreamFactory,
        )

        controller.stopSession()

        stopped.await()
        assertEquals(SessionStatus.IDLE, controller.uiState.value.session.status)
    }

    private fun pairedStartedController(
        mediaSession: RecordingMediaSession,
        client: FakeControlClient = FakeControlClient(),
        eventStreamFactory: ReceiverEventStreamFactory = ReceiverEventStreamFactory {
            object : ReceiverEventStream {
                override suspend fun collect(onEvent: suspend (ReceiverEvent) -> Unit) = Unit
            }
        },
    ): PocketLensController {
        val controller = PocketLensController(
            scope = CoroutineScope(Dispatchers.Unconfined),
            controlClient = client,
            tokenStorage = InMemoryTokenStorage(),
            mediaSessionFactory = MediaSessionControllerFactory { mediaSession },
            eventStreamFactory = eventStreamFactory,
            deviceNameProvider = { "Pixel" },
            pinProvider = { "123456" },
        )
        controller.showManualConnect()
        controller.setManualHost("127.0.0.1")
        controller.setPin("123456")
        controller.pair()
        controller.startSession()
        return controller
    }

    private class FakeDiscovery(initial: List<ReceiverAdvertisement>) : ReceiverDiscovery {
        override val advertisements = MutableStateFlow(initial)
        override fun start() = Unit
        override fun stop() = Unit
    }

    private class FakeControlClient : ControlClient {
        val baseUrls = mutableListOf<String>()
        var pairRequest: PairRequest? = null
        var securePairRequest: SecurePairRequest? = null
        var startRequest: SessionStartRequest? = null
        var stopRequest: SessionStopRequest? = null

        override suspend fun status(baseUrl: String): ReceiverStatus {
            baseUrls += baseUrl
            return ReceiverStatus(
                receiverName = "Desk",
                protocolVersion = 1,
                serviceType = "_pocketlens._udp.local",
                paired = false,
                activeSession = false,
                capabilities = ReceiverCapabilities(
                    videoCodecs = listOf("h264"),
                    audioCodecs = listOf("opus"),
                    qualityPresets = QualityPreset.entries,
                    adaptiveQuality = true,
                ),
                virtualDevices = VirtualDevices(
                    camera = VirtualDevice("PocketLens", true, "v4l2loopback"),
                    microphone = VirtualDevice("PocketLens Microphone", true, "pipewire"),
                ),
            )
        }

        override suspend fun pair(baseUrl: String, request: PairRequest): PairResponse {
            baseUrls += baseUrl
            pairRequest = request
            return PairResponse("token-abc123", "Desk", 1, 3600)
        }

        override suspend fun requestSecurePairing(
            baseUrl: String,
            request: SecurePairRequest,
        ): SecurePairRequestResponse {
            baseUrls += baseUrl
            securePairRequest = request
            return SecurePairRequestResponse(
                pairingId = request.pairingId,
                receiverNonce = "receiver-nonce",
                receiverPublicKey = "receiver-public-key",
                expiresInSeconds = 300,
            )
        }

        override suspend fun securePairingResult(baseUrl: String, pairingId: String): SecurePairResultResponse {
            baseUrls += baseUrl
            val request = securePairRequest ?: error("secure pair request missing")
            val key = PocketLensCrypto.securePairingKey(
                pin = "123456",
                pairingId = pairingId,
                phoneNonce = request.phoneNonce,
                receiverNonce = "receiver-nonce",
                phonePublicKey = request.phonePublicKey,
                receiverPublicKey = "receiver-public-key",
            )
            val encrypted = PocketLensCrypto.encryptToHex(
                key,
                pairingId.encodeToByteArray(),
                com.pocketlens.android.protocol.PocketLensJson.instance.encodeToString(
                    PairResponse("token-abc123", "Desk", 1, 3600),
                ).encodeToByteArray(),
            )
            return SecurePairResultResponse(pairingId, SecurePairingStatus.APPROVED, encrypted)
        }

        override suspend fun startSession(baseUrl: String, request: SessionStartRequest): SessionStartResponse {
            baseUrls += baseUrl
            startRequest = request
            return SessionStartResponse(
                sessionId = "session-abc123",
                receiverHost = "127.0.0.1",
                videoRtpPort = 5004,
                audioRtpPort = 5006,
                videoPayloadType = 96,
                audioPayloadType = 97,
                ssrcVideo = 11,
                ssrcAudio = 12,
                qualityPreset = request.qualityPreset,
                video = VideoConfig(),
                audio = AudioConfig(),
            )
        }

        override suspend fun stopSession(baseUrl: String, request: SessionStopRequest) {
            baseUrls += baseUrl
            stopRequest = request
        }

        override fun eventsUrl(baseUrl: String, sessionToken: String, sessionId: String?): String =
            buildString {
                append("$baseUrl/session/events?session_token=$sessionToken")
                if (sessionId != null) {
                    append("&session_id=$sessionId")
                }
            }
    }

    private class RecordingEventStreamFactory(
        private vararg val events: ReceiverEvent,
    ) : ReceiverEventStreamFactory {
        val urls = mutableListOf<String>()

        override fun create(url: String): ReceiverEventStream {
            urls += url
            return object : ReceiverEventStream {
                override suspend fun collect(onEvent: suspend (ReceiverEvent) -> Unit) {
                    events.forEach { onEvent(it) }
                }
            }
        }
    }

    private class RecordingMediaSession : MediaSessionController {
        var startedWith: SessionStartResponse? = null
        var controls: StreamControls? = null
        var stopped = false
        var muted = false
        var videoPaused = false
        var flipCount = 0
        var preset: QualityPreset? = null

        override suspend fun start(response: SessionStartResponse, controls: StreamControls) {
            startedWith = response
            this.controls = controls
        }

        override suspend fun stop() {
            stopped = true
        }

        override suspend fun setMicrophoneMuted(muted: Boolean) {
            this.muted = muted
        }

        override suspend fun setVideoPaused(paused: Boolean) {
            videoPaused = paused
        }

        override suspend fun flipCamera() {
            flipCount += 1
        }

        override suspend fun setPreset(preset: QualityPreset) {
            this.preset = preset
        }
    }
}
