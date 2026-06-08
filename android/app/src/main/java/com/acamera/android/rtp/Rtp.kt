package com.acamera.android.rtp

private const val RTP_VERSION = 2
private const val RTP_HEADER_SIZE = 12

data class RtpPacket(
    val payloadType: Int,
    val sequenceNumber: Int,
    val timestamp: UInt,
    val ssrc: UInt,
    val marker: Boolean,
    val payload: ByteArray,
) {
    fun toByteArray(): ByteArray {
        require(payloadType in 0..127) { "payloadType must fit 7 bits" }
        require(sequenceNumber in 0..0xFFFF) { "sequenceNumber must fit 16 bits" }

        val out = ByteArray(RTP_HEADER_SIZE + payload.size)
        out[0] = (RTP_VERSION shl 6).toByte()
        out[1] = ((if (marker) 0x80 else 0) or payloadType).toByte()
        out[2] = (sequenceNumber ushr 8).toByte()
        out[3] = sequenceNumber.toByte()
        writeUInt(out, 4, timestamp)
        writeUInt(out, 8, ssrc)
        payload.copyInto(out, RTP_HEADER_SIZE)
        return out
    }

    override fun equals(other: Any?): Boolean =
        other is RtpPacket &&
            payloadType == other.payloadType &&
            sequenceNumber == other.sequenceNumber &&
            timestamp == other.timestamp &&
            ssrc == other.ssrc &&
            marker == other.marker &&
            payload.contentEquals(other.payload)

    override fun hashCode(): Int {
        var result = payloadType
        result = 31 * result + sequenceNumber
        result = 31 * result + timestamp.hashCode()
        result = 31 * result + ssrc.hashCode()
        result = 31 * result + marker.hashCode()
        result = 31 * result + payload.contentHashCode()
        return result
    }
}

class RtpSequence(start: Int = 0) {
    private var next = start and 0xFFFF

    fun next(): Int {
        val current = next
        next = (next + 1) and 0xFFFF
        return current
    }
}

object H264RtpPacketizer {
    private const val DEFAULT_MAX_PAYLOAD_SIZE = 1_200
    private const val CLOCK_RATE = 90_000
    private const val FU_A_TYPE = 28

    fun timestampForFrame(frameIndex: Long, fps: Int): UInt =
        ((frameIndex * CLOCK_RATE) / fps).toUInt()

    fun packetizeNal(
        nalUnit: ByteArray,
        payloadType: Int,
        timestamp: UInt,
        ssrc: UInt,
        sequence: RtpSequence,
        maxPayloadSize: Int = DEFAULT_MAX_PAYLOAD_SIZE,
    ): List<RtpPacket> {
        require(nalUnit.isNotEmpty()) { "nalUnit must not be empty" }
        require(maxPayloadSize >= 3) { "maxPayloadSize must allow FU-A header" }
        if (nalUnit.size <= maxPayloadSize) {
            return listOf(
                RtpPacket(
                    payloadType = payloadType,
                    sequenceNumber = sequence.next(),
                    timestamp = timestamp,
                    ssrc = ssrc,
                    marker = true,
                    payload = nalUnit,
                ),
            )
        }

        val nalHeader = nalUnit[0].toInt() and 0xFF
        val nalType = nalHeader and 0x1F
        val nri = nalHeader and 0x60
        val fuIndicator = (nri or FU_A_TYPE).toByte()
        val fragmentCapacity = maxPayloadSize - 2
        val packets = mutableListOf<RtpPacket>()
        var offset = 1
        var first = true

        while (offset < nalUnit.size) {
            val remaining = nalUnit.size - offset
            val fragmentSize = minOf(fragmentCapacity, remaining)
            val last = offset + fragmentSize == nalUnit.size
            val fuHeader = ((if (first) 0x80 else 0) or (if (last) 0x40 else 0) or nalType).toByte()
            val payload = ByteArray(2 + fragmentSize)
            payload[0] = fuIndicator
            payload[1] = fuHeader
            nalUnit.copyInto(payload, destinationOffset = 2, startIndex = offset, endIndex = offset + fragmentSize)

            packets += RtpPacket(
                payloadType = payloadType,
                sequenceNumber = sequence.next(),
                timestamp = timestamp,
                ssrc = ssrc,
                marker = last,
                payload = payload,
            )
            first = false
            offset += fragmentSize
        }

        return packets
    }
}

object OpusRtpPacketizer {
    private const val SAMPLE_RATE = 48_000

    fun timestampForPacket(packetIndex: Long, frameDurationMs: Int = 20): UInt =
        (packetIndex * SAMPLE_RATE * frameDurationMs / 1_000).toUInt()

    fun packetizeFrame(
        opusFrame: ByteArray,
        payloadType: Int,
        timestamp: UInt,
        ssrc: UInt,
        sequence: RtpSequence,
    ): RtpPacket {
        require(opusFrame.isNotEmpty()) { "opusFrame must not be empty" }
        return RtpPacket(
            payloadType = payloadType,
            sequenceNumber = sequence.next(),
            timestamp = timestamp,
            ssrc = ssrc,
            marker = true,
            payload = opusFrame,
        )
    }
}

private fun writeUInt(target: ByteArray, offset: Int, value: UInt) {
    val long = value.toLong()
    target[offset] = (long ushr 24).toByte()
    target[offset + 1] = (long ushr 16).toByte()
    target[offset + 2] = (long ushr 8).toByte()
    target[offset + 3] = long.toByte()
}
