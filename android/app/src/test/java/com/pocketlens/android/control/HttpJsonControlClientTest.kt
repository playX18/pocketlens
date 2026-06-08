package com.pocketlens.android.control

import com.pocketlens.android.protocol.PairRequest
import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.protocol.ReceiverStatus
import com.pocketlens.android.protocol.SessionStartRequest
import com.pocketlens.android.protocol.SessionStopRequest
import com.sun.net.httpserver.HttpExchange
import com.sun.net.httpserver.HttpServer
import java.net.InetSocketAddress
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.runBlocking

class HttpJsonControlClientTest {
    private var server: HttpServer? = null

    @AfterTest
    fun tearDown() {
        server?.stop(0)
    }

    @Test
    fun callsStatusPairStartAndStopRoutesWithJson() = runBlocking {
        val seenRequests = mutableListOf<String>()
        val baseUrl = startServer { exchange ->
            seenRequests += "${exchange.requestMethod} ${exchange.requestURI.path} ${exchange.requestBody.bufferedReader().readText()}"
            when (exchange.requestURI.path) {
                ControlRoutes.STATUS -> exchange.respond(fixture("receiver_status.ready.json"))
                ControlRoutes.PAIR -> exchange.respond(fixture("pair.success.json"))
                ControlRoutes.SESSION_START -> exchange.respond(fixture("session_start.success.json"))
                ControlRoutes.SESSION_STOP -> exchange.respond(fixture("session_stop.success.json"))
                else -> exchange.respond("{}", 404)
            }
        }
        val client = HttpJsonControlClient()

        val status: ReceiverStatus = client.status(baseUrl)
        val pair = client.pair(baseUrl, PairRequest(pin = "123456", deviceName = "Pixel"))
        val session = client.startSession(baseUrl, SessionStartRequest(pair.sessionToken, QualityPreset.BALANCED))
        client.stopSession(baseUrl, SessionStopRequest(pair.sessionToken, session.sessionId))

        assertEquals("PocketLens Linux", status.receiverName)
        assertEquals("session_0123456789abcdef", pair.sessionToken)
        assertEquals("sess_0123456789abcdef", session.sessionId)
        assertEquals("GET /status ", seenRequests[0])
        assertEquals(true, seenRequests[1].contains("\"pin\":\"123456\""))
        assertEquals(true, seenRequests[2].contains("\"session_token\":\"session_0123456789abcdef\""))
        assertEquals(true, seenRequests[3].contains("\"session_id\":\"sess_0123456789abcdef\""))
    }

    @Test
    fun throwsContractErrorEnvelopeForNonSuccessResponses() = runBlocking {
        val baseUrl = startServer { exchange ->
            exchange.respond(fixture("pair.invalid_pin.json"), 403)
        }

        val error = assertFailsWith<ControlClientException> {
            HttpJsonControlClient().pair(baseUrl, PairRequest(pin = "0000", deviceName = "Pixel"))
        }

        assertEquals("invalid_pin", error.code)
        assertEquals(403, error.httpStatus)
    }

    @Test
    fun buildsWebSocketEventsUrlWithToken() {
        val url = HttpJsonControlClient().eventsUrl("http://192.168.1.2:3769", "token abc", "sess 1")

        assertEquals("ws://192.168.1.2:3769/session/events?session_token=token+abc&session_id=sess+1", url)
    }

    private fun startServer(handler: (HttpExchange) -> Unit): String {
        val httpServer = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        httpServer.createContext("/") { exchange -> handler(exchange) }
        httpServer.start()
        server = httpServer
        return "http://127.0.0.1:${httpServer.address.port}"
    }

    private fun HttpExchange.respond(body: String, status: Int = 200) {
        val bytes = body.toByteArray()
        sendResponseHeaders(status, bytes.size.toLong())
        responseBody.use { it.write(bytes) }
    }

    private fun fixture(name: String): String =
        javaClass.classLoader!!.getResource("fixtures/$name")!!.readText()
}
