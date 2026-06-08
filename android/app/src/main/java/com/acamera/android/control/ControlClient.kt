package com.acamera.android.control

import com.acamera.android.protocol.ACameraJson
import com.acamera.android.protocol.ErrorEnvelope
import com.acamera.android.protocol.PairRequest
import com.acamera.android.protocol.PairResponse
import com.acamera.android.protocol.ReceiverEvent
import com.acamera.android.protocol.ReceiverStatus
import com.acamera.android.protocol.SecurePairRequest
import com.acamera.android.protocol.SecurePairRequestResponse
import com.acamera.android.protocol.SecurePairResultResponse
import com.acamera.android.protocol.SessionStartRequest
import com.acamera.android.protocol.SessionStartResponse
import com.acamera.android.protocol.SessionStopRequest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.net.URLEncoder
import java.nio.charset.StandardCharsets

interface ControlClient {
    suspend fun status(baseUrl: String): ReceiverStatus
    suspend fun pair(baseUrl: String, request: PairRequest): PairResponse
    suspend fun requestSecurePairing(baseUrl: String, request: SecurePairRequest): SecurePairRequestResponse
    suspend fun securePairingResult(baseUrl: String, pairingId: String): SecurePairResultResponse
    suspend fun startSession(baseUrl: String, request: SessionStartRequest): SessionStartResponse
    suspend fun stopSession(baseUrl: String, request: SessionStopRequest)
    fun eventsUrl(baseUrl: String, sessionToken: String, sessionId: String? = null): String
}

fun interface ReceiverEventStreamFactory {
    fun create(url: String): ReceiverEventStream
}

object ControlRoutes {
    const val STATUS = "/status"
    const val PAIR = "/pair"
    const val PAIR_REQUEST = "/pair/request"
    const val PAIR_RESULT = "/pair/result"
    const val SESSION_START = "/session/start"
    const val SESSION_STOP = "/session/stop"
    const val SESSION_EVENTS = "/session/events"
}

interface ReceiverEventStream {
    suspend fun collect(onEvent: suspend (ReceiverEvent) -> Unit)
}

class ControlClientException(
    val code: String,
    override val message: String,
    val httpStatus: Int? = null,
) : RuntimeException(message)

class HttpJsonControlClient(
    private val json: Json = ACameraJson.instance,
    private val connectTimeoutMillis: Int = 3_000,
    private val readTimeoutMillis: Int = 5_000,
) : ControlClient {
    override suspend fun status(baseUrl: String): ReceiverStatus =
        get(baseUrl, ControlRoutes.STATUS)

    override suspend fun pair(baseUrl: String, request: PairRequest): PairResponse =
        post(baseUrl, ControlRoutes.PAIR, request)

    override suspend fun requestSecurePairing(baseUrl: String, request: SecurePairRequest): SecurePairRequestResponse =
        post(baseUrl, ControlRoutes.PAIR_REQUEST, request)

    override suspend fun securePairingResult(baseUrl: String, pairingId: String): SecurePairResultResponse =
        get(baseUrl, "${ControlRoutes.PAIR_RESULT}?pairing_id=${urlEncode(pairingId)}")

    override suspend fun startSession(baseUrl: String, request: SessionStartRequest): SessionStartResponse =
        post(baseUrl, ControlRoutes.SESSION_START, request)

    override suspend fun stopSession(baseUrl: String, request: SessionStopRequest) {
        post<UnitResponse, SessionStopRequest>(baseUrl, ControlRoutes.SESSION_STOP, request)
    }

    override fun eventsUrl(baseUrl: String, sessionToken: String, sessionId: String?): String {
        val uri = URI(baseUrl)
        val scheme = when (uri.scheme) {
            "https" -> "wss"
            else -> "ws"
        }
        val encoded = URLEncoder.encode(sessionToken, StandardCharsets.UTF_8.name())
        val encodedSessionId = sessionId?.let { URLEncoder.encode(it, StandardCharsets.UTF_8.name()) }
        val query = buildString {
            append("session_token=")
            append(encoded)
            if (encodedSessionId != null) {
                append("&session_id=")
                append(encodedSessionId)
            }
        }
        return uri.resolve(ControlRoutes.SESSION_EVENTS).toString()
            .replaceFirst(Regex("^https?"), scheme) + "?$query"
    }

    private suspend inline fun <reified Response> get(baseUrl: String, route: String): Response =
        request(baseUrl, route, method = "GET", body = null)

    private suspend inline fun <reified Response, reified Request> post(
        baseUrl: String,
        route: String,
        request: Request,
    ): Response =
        request(baseUrl, route, method = "POST", body = json.encodeToString(request))

    private suspend inline fun <reified Response> request(
        baseUrl: String,
        route: String,
        method: String,
        body: String?,
    ): Response = withContext(Dispatchers.IO) {
        val connection = URL(normalizeBaseUrl(baseUrl) + route).openConnection() as HttpURLConnection
        try {
            connection.requestMethod = method
            connection.connectTimeout = connectTimeoutMillis
            connection.readTimeout = readTimeoutMillis
            connection.setRequestProperty("Accept", "application/json")
            if (body != null) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json")
                connection.outputStream.use { it.write(body.toByteArray(StandardCharsets.UTF_8)) }
            }

            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val payload = stream?.bufferedReader(StandardCharsets.UTF_8)?.use { it.readText() }.orEmpty()
            if (status !in 200..299) {
                throw errorFromPayload(payload, status)
            }
            if (Response::class == UnitResponse::class) {
                @Suppress("UNCHECKED_CAST")
                UnitResponse as Response
            } else {
                json.decodeFromString(payload)
            }
        } finally {
            connection.disconnect()
        }
    }

    private fun errorFromPayload(payload: String, status: Int): ControlClientException {
        val envelope = runCatching { json.decodeFromString<ErrorEnvelope>(payload) }.getOrNull()
        return ControlClientException(
            code = envelope?.error?.code ?: "http_$status",
            message = envelope?.error?.message ?: payload.ifBlank { "HTTP $status" },
            httpStatus = status,
        )
    }

    private fun normalizeBaseUrl(baseUrl: String): String =
        baseUrl.trim().removeSuffix("/")

    private fun urlEncode(value: String): String =
        URLEncoder.encode(value, StandardCharsets.UTF_8.name())

    private object UnitResponse
}

class JsonLineReceiverEventStream(
    private val url: String,
    private val json: Json = ACameraJson.instance,
    private val connectTimeoutMillis: Int = 3_000,
    private val readTimeoutMillis: Int = 0,
) : ReceiverEventStream {
    override suspend fun collect(onEvent: suspend (ReceiverEvent) -> Unit) {
        withContext(Dispatchers.IO) {
            val connection = URL(url.replaceFirst("ws://", "http://").replaceFirst("wss://", "https://"))
                .openConnection() as HttpURLConnection
            try {
                connection.connectTimeout = connectTimeoutMillis
                connection.readTimeout = readTimeoutMillis
                connection.setRequestProperty("Accept", "application/json")
                BufferedReader(InputStreamReader(connection.inputStream, StandardCharsets.UTF_8)).useLines { lines ->
                    for (line in lines) {
                        val trimmed = line.trim()
                        if (trimmed.isNotEmpty()) {
                            val event = try {
                                json.decodeFromString<ReceiverEvent>(trimmed)
                            } catch (error: SerializationException) {
                                throw ControlClientException("invalid_event", error.message ?: "Invalid receiver event")
                            }
                            kotlinx.coroutines.runBlocking {
                                onEvent(event)
                            }
                        }
                    }
                }
            } finally {
                connection.disconnect()
            }
        }
    }
}

class WebSocketReceiverEventStream(
    private val url: String,
    private val json: Json = ACameraJson.instance,
    private val client: OkHttpClient = OkHttpClient(),
) : ReceiverEventStream {
    override suspend fun collect(onEvent: suspend (ReceiverEvent) -> Unit) = coroutineScope {
        val events = Channel<Result<ReceiverEvent>>(Channel.UNLIMITED)
        val request = Request.Builder().url(url).build()
        val webSocket = client.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onMessage(webSocket: WebSocket, text: String) {
                    events.trySend(parseEvent(text))
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    events.close()
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    events.trySend(Result.failure(t))
                    events.close(t)
                }
            },
        )
        try {
            for (event in events) {
                onEvent(event.getOrThrow())
            }
        } finally {
            webSocket.cancel()
            events.close()
        }
    }

    private fun parseEvent(text: String): Result<ReceiverEvent> =
        runCatching {
            json.decodeFromString<ReceiverEvent>(text)
        }.recoverCatching { error ->
            if (error is SerializationException) {
                throw ControlClientException("invalid_event", error.message ?: "Invalid receiver event")
            }
            throw error
        }
}
