package com.pocketlens.android.rtp

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RtpPacketizerTest {
    @Test
    fun rtpHeaderContainsVersionMarkerPayloadTypeSequenceTimestampAndSsrc() {
        val packet = RtpPacket(
            payloadType = 96,
            sequenceNumber = 0x1234,
            timestamp = 0x01020304u,
            ssrc = 0xA0B0C0D0u,
            marker = true,
            payload = byteArrayOf(1, 2, 3),
        ).toByteArray()

        assertEquals(0x80.toByte(), packet[0])
        assertEquals(0xE0.toByte(), packet[1])
        assertEquals(0x12.toByte(), packet[2])
        assertEquals(0x34.toByte(), packet[3])
        assertContentEquals(byteArrayOf(1, 2, 3), packet.copyOfRange(12, 15))
    }

    @Test
    fun h264SmallNalUsesSinglePacketWithMarker() {
        val sequence = RtpSequence(start = 10)
        val packets = H264RtpPacketizer.packetizeNal(
            nalUnit = byteArrayOf(0x65, 1, 2, 3),
            payloadType = 96,
            timestamp = 3_000u,
            ssrc = 7u,
            sequence = sequence,
        )

        assertEquals(1, packets.size)
        assertEquals(10, packets.single().sequenceNumber)
        assertTrue(packets.single().marker)
        assertContentEquals(byteArrayOf(0x65, 1, 2, 3), packets.single().payload)
    }

    @Test
    fun h264LargeNalUsesFuAFragmentsWithMarkerOnLastPacket() {
        val sequence = RtpSequence(start = 20)
        val nal = byteArrayOf(0x65, 1, 2, 3, 4, 5, 6)
        val packets = H264RtpPacketizer.packetizeNal(
            nalUnit = nal,
            payloadType = 96,
            timestamp = 9_000u,
            ssrc = 9u,
            sequence = sequence,
            maxPayloadSize = 4,
        )

        assertEquals(3, packets.size)
        assertEquals(20, packets[0].sequenceNumber)
        assertEquals(21, packets[1].sequenceNumber)
        assertEquals(22, packets[2].sequenceNumber)
        assertFalse(packets[0].marker)
        assertFalse(packets[1].marker)
        assertTrue(packets[2].marker)
        assertEquals(28, packets[0].payload[0].toInt() and 0x1F)
        assertTrue((packets[0].payload[1].toInt() and 0x80) != 0)
        assertTrue((packets[2].payload[1].toInt() and 0x40) != 0)
    }

    @Test
    fun opusPacketsAdvanceSequenceAndUseTwentyMillisecondTimestamp() {
        val sequence = RtpSequence(start = 30)
        val firstTimestamp = OpusRtpPacketizer.timestampForPacket(packetIndex = 0)
        val secondTimestamp = OpusRtpPacketizer.timestampForPacket(packetIndex = 1)
        val packet = OpusRtpPacketizer.packetizeFrame(
            opusFrame = byteArrayOf(0x11, 0x22),
            payloadType = 97,
            timestamp = secondTimestamp,
            ssrc = 12u,
            sequence = sequence,
        )

        assertEquals(0u, firstTimestamp)
        assertEquals(960u, secondTimestamp)
        assertEquals(30, packet.sequenceNumber)
        assertEquals(97, packet.payloadType)
        assertTrue(packet.marker)
    }
}
