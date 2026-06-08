package com.pocketlens.android.rtp

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals

class UdpRtpSenderTest {
    @Test
    fun sendSerializesPacketToConfiguredTarget() {
        val transport = RecordingDatagramTransport()
        val sender = UdpRtpSender(transport)
        val target = RtpTarget(host = "192.168.1.10", port = 50_004)
        val packet = RtpPacket(
            payloadType = 96,
            sequenceNumber = 42,
            timestamp = 1234u,
            ssrc = 99u,
            marker = true,
            payload = byteArrayOf(1, 2, 3),
        )

        sender.sendBlocking(target, packet)

        val sent = transport.sent.single()
        assertEquals("192.168.1.10", sent.host)
        assertEquals(50_004, sent.port)
        assertContentEquals(packet.toByteArray(), sent.payload)
    }

    @Test
    fun closeClosesUnderlyingTransport() {
        val transport = RecordingDatagramTransport()
        val sender = UdpRtpSender(transport)

        sender.closeBlocking()

        assertEquals(1, transport.closeCount)
    }

    private class RecordingDatagramTransport : DatagramTransport {
        val sent = mutableListOf<SentDatagram>()
        var closeCount = 0

        override fun send(host: String, port: Int, payload: ByteArray) {
            sent += SentDatagram(host, port, payload)
        }

        override fun close() {
            closeCount += 1
        }
    }

    private data class SentDatagram(
        val host: String,
        val port: Int,
        val payload: ByteArray,
    )
}
