package com.acamera.android.discovery

import android.content.Context
import android.net.wifi.WifiManager
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.Inet4Address
import java.net.NetworkInterface
import java.net.SocketTimeoutException

private val REQUIRED_CAPABILITIES = setOf("secure_pairing", "encrypted_rtp")

data class ReceiverAdvertisement(
    val receiverName: String,
    val protocolVersion: Int,
    val controlPort: Int,
    val capabilities: Set<String>,
    val host: String? = null,
)

sealed interface DiscoveryParseResult {
    data class Valid(val advertisement: ReceiverAdvertisement) : DiscoveryParseResult
    data class Invalid(val reason: String) : DiscoveryParseResult
}

object MdnsTxtParser {
    private const val REQUIRED_PROTOCOL = 1

    fun parseAttributes(attributes: Map<String, ByteArray>, host: String? = null): DiscoveryParseResult =
        parse(
            txt = attributes.mapValues { (_, value) -> value.toString(Charsets.UTF_8) },
            host = host,
        )

    fun parse(txt: Map<String, String>, host: String? = null): DiscoveryParseResult {
        val name = txt["name"]?.takeIf { it.isNotBlank() }
            ?: return DiscoveryParseResult.Invalid("missing receiver name")
        val version = txt["version"]?.toIntOrNull()
            ?: return DiscoveryParseResult.Invalid("missing protocol version")
        if (version != REQUIRED_PROTOCOL) {
            return DiscoveryParseResult.Invalid("unsupported protocol version")
        }
        val controlPort = txt["control_port"]?.toIntOrNull()
            ?: return DiscoveryParseResult.Invalid("missing control port")
        if (controlPort !in 1..65535) {
            return DiscoveryParseResult.Invalid("invalid control port")
        }
        val capabilities = txt["capabilities"]
            ?.split(',')
            ?.map { it.trim() }
            ?.filter { it.isNotEmpty() }
            ?.toSet()
            ?: emptySet()
        if (!capabilities.containsAll(REQUIRED_CAPABILITIES)) {
            return DiscoveryParseResult.Invalid("receiver does not support secure encrypted pairing")
        }

        return DiscoveryParseResult.Valid(
            ReceiverAdvertisement(
                receiverName = name,
                protocolVersion = version,
                controlPort = controlPort,
                capabilities = capabilities,
                host = host,
            ),
        )
    }
}

interface ReceiverDiscovery {
    val advertisements: StateFlow<List<ReceiverAdvertisement>>
    fun start()
    fun stop()
}

class AndroidNsdReceiverDiscovery(
    context: Context,
) : ReceiverDiscovery {
    private val nsdManager = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
    private val _advertisements = MutableStateFlow<List<ReceiverAdvertisement>>(emptyList())
    private var discoveryListener: NsdManager.DiscoveryListener? = null
    private var multicastLock: WifiManager.MulticastLock? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var broadcastJob: Job? = null
    private var broadcastSocket: DatagramSocket? = null
    private val knownReceivers = linkedMapOf<String, ReceiverAdvertisement>()

    override val advertisements: StateFlow<List<ReceiverAdvertisement>> = _advertisements

    override fun start() {
        if (discoveryListener != null) return
        multicastLock = wifiManager.createMulticastLock("acamera-mdns").apply {
            setReferenceCounted(false)
            acquire()
        }
        val listener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) = Unit

            override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                if (serviceInfo.serviceType.startsWith(SERVICE_TYPE_PREFIX)) {
                    nsdManager.resolveService(serviceInfo, resolveListener())
                }
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) {
                knownReceivers.remove(serviceInfo.serviceName)
                publish()
            }

            override fun onDiscoveryStopped(serviceType: String) = Unit

            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                stop()
            }

            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
                stop()
            }
        }
        discoveryListener = listener
        nsdManager.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
        startBroadcastDiscovery()
    }

    override fun stop() {
        discoveryListener?.let { listener ->
            runCatching { nsdManager.stopServiceDiscovery(listener) }
        }
        discoveryListener = null
        broadcastJob?.cancel()
        broadcastJob = null
        broadcastSocket?.close()
        broadcastSocket = null
        multicastLock?.let { lock ->
            if (lock.isHeld) {
                lock.release()
            }
        }
        multicastLock = null
    }

    private fun resolveListener(): NsdManager.ResolveListener =
        object : NsdManager.ResolveListener {
            override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit

            override fun onServiceResolved(serviceInfo: NsdServiceInfo) {
                val host = serviceInfo.hostAddress()
                val result = MdnsTxtParser.parseAttributes(serviceInfo.attributes, host = host)
                if (result is DiscoveryParseResult.Valid) {
                    knownReceivers[serviceInfo.serviceName] = result.advertisement
                    publish()
                }
            }
        }

    private fun publish() {
        _advertisements.value = knownReceivers.values.sortedBy { it.receiverName }
    }

    private fun startBroadcastDiscovery() {
        broadcastJob?.cancel()
        broadcastJob = scope.launch {
            val socket = DatagramSocket().apply {
                broadcast = true
                soTimeout = 300
            }
            broadcastSocket = socket
            val probe = DISCOVERY_PROBE.toByteArray(Charsets.UTF_8)
            val receiveBuffer = ByteArray(1_500)
            while (isActive && !socket.isClosed) {
                val broadcastAddresses = localBroadcastAddresses()
                for (port in DISCOVERY_PORTS) {
                    for (address in broadcastAddresses) {
                        runCatching {
                            socket.send(DatagramPacket(probe, probe.size, address, port))
                        }
                    }
                }
                val deadline = System.currentTimeMillis() + 1_000
                while (isActive && System.currentTimeMillis() < deadline && !socket.isClosed) {
                    try {
                        val packet = DatagramPacket(receiveBuffer, receiveBuffer.size)
                        socket.receive(packet)
                        parseBroadcastResponse(
                            text = String(packet.data, packet.offset, packet.length, Charsets.UTF_8),
                            host = packet.address.hostAddress,
                        )?.let { receiver ->
                            knownReceivers["udp:${receiver.host}:${receiver.controlPort}"] = receiver
                            publish()
                        }
                    } catch (_: SocketTimeoutException) {
                        break
                    }
                }
                delay(2_000)
            }
        }
    }

    private fun localBroadcastAddresses(): Set<InetAddress> {
        val addresses = linkedSetOf(InetAddress.getByName("255.255.255.255"))
        val interfaces = runCatching { NetworkInterface.getNetworkInterfaces().toList() }.getOrDefault(emptyList())
        for (networkInterface in interfaces) {
            if (!networkInterface.isUp || networkInterface.isLoopback) continue
            for (interfaceAddress in networkInterface.interfaceAddresses) {
                val address = interfaceAddress.address
                val broadcast = interfaceAddress.broadcast
                if (address is Inet4Address && broadcast != null) {
                    addresses += broadcast
                }
            }
        }
        return addresses
    }

    private fun parseBroadcastResponse(text: String, host: String?): ReceiverAdvertisement? {
        val json = runCatching { JSONObject(text) }.getOrNull() ?: return null
        if (json.optString("service_type") != "_acamera._udp.local") return null
        val version = json.optInt("protocol_version", -1)
        if (version != 1) return null
        val port = json.optInt("control_port", -1).takeIf { it in 1..65535 } ?: return null
        val capabilities = buildSet {
            val array = json.optJSONArray("capabilities")
            if (array != null) {
                for (index in 0 until array.length()) {
                    array.optString(index).takeIf { it.isNotBlank() }?.let(::add)
                }
            }
        }
        if (!capabilities.containsAll(REQUIRED_CAPABILITIES)) return null
        return ReceiverAdvertisement(
            receiverName = json.optString("receiver_name", "ACamera Linux"),
            protocolVersion = version,
            controlPort = port,
            capabilities = capabilities,
            host = host,
        )
    }

    private fun NsdServiceInfo.hostAddress(): String? {
        val host: InetAddress? = host
        return host?.hostAddress
    }

    companion object {
        const val SERVICE_TYPE = "_acamera._udp."
        private const val SERVICE_TYPE_PREFIX = "_acamera._udp"
        private const val DISCOVERY_PROBE = "ACAMERA_DISCOVER_V1"
        private val DISCOVERY_PORTS = intArrayOf(3769, 47650)
    }
}
