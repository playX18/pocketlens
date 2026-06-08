package com.acamera.android.protocol

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString

class ProtocolModelsTest {
    private val json = ACameraJson.instance

    @Test
    fun androidFixtureCopiesMatchRootContractFixturesExactly() {
        contractFixtureNames.forEach { name ->
            assertEquals(rootFixture(name), fixture(name), "Android fixture drifted from root contract: $name")
        }
    }

    @Test
    fun qualityPresetSerializesAsPlanFieldValues() {
        assertEquals("\"low\"", json.encodeToString(QualityPreset.LOW))
        assertEquals("\"balanced\"", json.encodeToString(QualityPreset.BALANCED))
        assertEquals("\"high\"", json.encodeToString(QualityPreset.HIGH))
    }

    @Test
    fun receiverStatusFixturesMatchDto() {
        val ready = json.decodeFromString<ReceiverStatus>(fixture("receiver_status.ready.json"))
        val missingDependencies = json.decodeFromString<ReceiverStatus>(
            fixture("receiver_status.missing_dependencies.json"),
        )

        assertEquals("ACamera Linux", ready.receiverName)
        assertEquals(1, ready.protocolVersion)
        assertEquals("_acamera._udp.local", ready.serviceType)
        assertEquals(listOf("h264"), ready.capabilities.videoCodecs)
        assertEquals(listOf("opus"), ready.capabilities.audioCodecs)
        assertEquals(listOf(QualityPreset.LOW, QualityPreset.BALANCED, QualityPreset.HIGH), ready.capabilities.qualityPresets)
        assertEquals(true, ready.virtualDevices.camera.ready)
        assertEquals("v4l2loopback", ready.virtualDevices.camera.backend)
        assertEquals(true, ready.virtualDevices.microphone.ready)
        assertEquals("pipewire", ready.virtualDevices.microphone.backend)
        assertEquals(emptyList(), ready.diagnostics)

        assertEquals(false, missingDependencies.virtualDevices.camera.ready)
        assertEquals(false, missingDependencies.virtualDevices.microphone.ready)
        assertEquals("missing_v4l2loopback", missingDependencies.diagnostics.first().code)
    }

    @Test
    fun pairFixturesMatchDto() {
        val request = json.decodeFromString<PairRequest>(fixture("pair.request.json"))
        val response = json.decodeFromString<PairResponse>(fixture("pair.success.json"))
        val invalidPin = json.decodeFromString<ErrorEnvelope>(fixture("pair.invalid_pin.json"))

        assertEquals(PairRequest(pin = "123456", deviceName = "Pixel 9"), request)
        assertEquals("session_0123456789abcdef", response.sessionToken)
        assertEquals("ACamera Linux", response.receiverName)
        assertEquals(1, response.protocolVersion)
        assertEquals(86_400, response.expiresInSeconds)
        assertEquals("invalid_pin", invalidPin.error.code)
    }

    @Test
    fun sessionStartFixturesMatchDto() {
        val request = json.decodeFromString<SessionStartRequest>(fixture("session_start.request.json"))
        val response = json.decodeFromString<SessionStartResponse>(fixture("session_start.success.json"))

        assertEquals("session_0123456789abcdef", request.sessionToken)
        assertEquals(QualityPreset.BALANCED, request.qualityPreset)
        assertEquals(VideoConfig(codec = "h264", width = 1280, height = 720, fps = 30), request.video)
        assertEquals(AudioConfig(codec = "opus", sampleRateHz = 48_000, channels = 1), request.audio)

        assertEquals("sess_0123456789abcdef", response.sessionId)
        assertEquals("192.168.1.25", response.receiverHost)
        assertEquals(5004, response.videoRtpPort)
        assertEquals(5006, response.audioRtpPort)
        assertEquals(96, response.videoPayloadType)
        assertEquals(97, response.audioPayloadType)
        assertEquals(305_419_896L, response.ssrcVideo)
        assertEquals(2_596_069_104L, response.ssrcAudio)
        assertEquals(QualityPreset.BALANCED, response.qualityPreset)
        assertEquals(request.video, response.video)
        assertEquals(request.audio, response.audio)
    }

    @Test
    fun sessionStopFixturesMatchDto() {
        val request = json.decodeFromString<SessionStopRequest>(fixture("session_stop.request.json"))
        val response = json.decodeFromString<SessionStopResponse>(fixture("session_stop.success.json"))

        assertEquals("session_0123456789abcdef", request.sessionToken)
        assertEquals("sess_0123456789abcdef", request.sessionId)
        assertEquals("sess_0123456789abcdef", response.sessionId)
        assertEquals(true, response.stopped)
    }

    @Test
    fun receiverEventsParseStatsWarningsAndErrors() {
        val stats = json.decodeFromString<ReceiverEvent>(fixture("events.stats.json"))
        val warning = json.decodeFromString<ReceiverEvent>(fixture("events.warning.json"))
        val error = json.decodeFromString<ReceiverEvent>(fixture("events.error.json"))

        assertEquals(
            ReceiverEvent.Stats(
                sessionId = "sess_0123456789abcdef",
                videoPackets = 4_200,
                audioPackets = 8_400,
                videoPacketsLost = 4,
                audioPacketsLost = 2,
                estimatedBitrateKbps = 2_400,
                qualityPreset = QualityPreset.BALANCED,
            ),
            stats,
        )
        assertEquals(
            ReceiverEvent.Warning(
                sessionId = "sess_0123456789abcdef",
                code = "network_degraded",
                message = "Packet loss is above the balanced preset target.",
            ),
            warning,
        )
        assertEquals(
            ReceiverEvent.Error(
                sessionId = "sess_0123456789abcdef",
                code = "media_pipeline_failed",
                message = "The Linux media pipeline stopped unexpectedly.",
            ),
            error,
        )
    }

    @Test
    fun unknownFieldsFailToProtectContractDrift() {
        assertFailsWith<Exception> {
            json.decodeFromString<ReceiverStatus>(
                """
                {
                  "receiver_name": "x",
                  "protocol_version": 1,
                  "service_type": "_acamera._udp.local",
                  "paired": false,
                  "active_session": false,
                  "capabilities": {
                    "video_codecs": ["h264"],
                    "audio_codecs": ["opus"],
                    "quality_presets": ["balanced"],
                    "adaptive_quality": true
                  },
                  "virtual_devices": {
                    "camera": {"name": "ACamera", "ready": true, "backend": "v4l2loopback"},
                    "microphone": {"name": "ACamera Microphone", "ready": true, "backend": "pipewire"}
                  },
                  "diagnostics": [],
                  "unexpected": true
                }
                """.trimIndent(),
            )
        }
    }

    private fun fixture(name: String): String =
        requireNotNull(javaClass.classLoader?.getResource("fixtures/$name")) { "missing fixture $name" }
            .readText()

    private fun rootFixture(name: String): String =
        File(rootDir(), "contracts/fixtures/$name").readText()

    private fun rootDir(): File {
        val userDir = requireNotNull(System.getProperty("user.dir")) { "user.dir is not set" }
        return generateSequence(File(userDir).absoluteFile) { it.parentFile }
            .firstOrNull { File(it, "contracts/fixtures").isDirectory }
            ?: error("could not locate root contracts/fixtures from $userDir")
    }

    private companion object {
        val contractFixtureNames = listOf(
            "events.error.json",
            "events.stats.json",
            "events.warning.json",
            "pair.invalid_pin.json",
            "pair.request.json",
            "pair.success.json",
            "receiver_status.missing_dependencies.json",
            "receiver_status.ready.json",
            "session_start.request.json",
            "session_start.success.json",
            "session_stop.request.json",
            "session_stop.success.json",
        )
    }
}
