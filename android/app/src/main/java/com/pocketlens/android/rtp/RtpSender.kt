package com.pocketlens.android.rtp

import com.pocketlens.android.crypto.PocketLensCrypto
import com.pocketlens.android.crypto.hexToBytes
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress

data class RtpTarget(
    val host: String,
    val port: Int,
) {
    init {
        require(host.isNotBlank()) { "host must not be blank" }
        require(port in 1..65535) { "port must be a valid UDP port" }
    }
}

interface RtpSender {
    suspend fun send(target: RtpTarget, packet: RtpPacket)
    suspend fun close()
}

interface DatagramTransport {
    fun send(host: String, port: Int, payload: ByteArray)
    fun close()
}

class UdpDatagramTransport(
    private val socket: DatagramSocket = DatagramSocket(),
) : DatagramTransport {
    override fun send(host: String, port: Int, payload: ByteArray) {
        val address = InetAddress.getByName(host)
        socket.send(DatagramPacket(payload, payload.size, address, port))
    }

    override fun close() {
        socket.close()
    }
}

class UdpRtpSender(
    private val transport: DatagramTransport = UdpDatagramTransport(),
) : RtpSender {
    override suspend fun send(target: RtpTarget, packet: RtpPacket) {
        val bytes = packet.toByteArray()
        withContext(Dispatchers.IO) {
            transport.send(target.host, target.port, bytes)
        }
    }

    override suspend fun close() {
        withContext(Dispatchers.IO) {
            transport.close()
        }
    }

    fun sendBlocking(target: RtpTarget, packet: RtpPacket) {
        runBlocking {
            send(target, packet)
        }
    }

    fun closeBlocking() {
        runBlocking {
            close()
        }
    }
}

class EncryptingRtpSender(
    private val transport: DatagramTransport = UdpDatagramTransport(),
    keyHex: String,
    private val streamLabel: String,
) : RtpSender {
    private val key = keyHex.hexToBytes()
    private var counter = 0L

    override suspend fun send(target: RtpTarget, packet: RtpPacket) {
        val encrypted = PocketLensCrypto.encryptPacket(
            key = key,
            counter = counter++,
            aad = streamLabel.encodeToByteArray(),
            plaintext = packet.toByteArray(),
        )
        withContext(Dispatchers.IO) {
            transport.send(target.host, target.port, encrypted)
        }
    }

    override suspend fun close() {
        withContext(Dispatchers.IO) {
            transport.close()
        }
    }
}
