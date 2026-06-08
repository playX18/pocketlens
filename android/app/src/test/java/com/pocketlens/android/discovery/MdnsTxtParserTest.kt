package com.pocketlens.android.discovery

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class MdnsTxtParserTest {
    @Test
    fun parsesValidTxtAdvertisement() {
        val result = MdnsTxtParser.parse(
            txt = mapOf(
                "name" to "Living Room Receiver",
                "version" to "1",
                "control_port" to "3769",
                "capabilities" to "h264, opus, rtp, secure_pairing, encrypted_rtp",
            ),
            host = "192.168.1.8",
        )

        assertTrue(result is DiscoveryParseResult.Valid)
        assertEquals("Living Room Receiver", result.advertisement.receiverName)
        assertEquals(1, result.advertisement.protocolVersion)
        assertEquals(3769, result.advertisement.controlPort)
        assertEquals(
            setOf("h264", "opus", "rtp", "secure_pairing", "encrypted_rtp"),
            result.advertisement.capabilities,
        )
        assertEquals("192.168.1.8", result.advertisement.host)
    }

    @Test
    fun rejectsUnsupportedProtocolVersion() {
        val result = MdnsTxtParser.parse(
            mapOf(
                "name" to "Receiver",
                "version" to "2",
                "control_port" to "3769",
            ),
        )

        assertEquals(DiscoveryParseResult.Invalid("unsupported protocol version"), result)
    }

    @Test
    fun rejectsInvalidControlPort() {
        val result = MdnsTxtParser.parse(
            mapOf(
                "name" to "Receiver",
                "version" to "1",
                "control_port" to "70000",
            ),
        )

        assertEquals(DiscoveryParseResult.Invalid("invalid control port"), result)
    }

    @Test
    fun parsesAndroidNsdTxtAttributeBytes() {
        val result = MdnsTxtParser.parseAttributes(
            mapOf(
                "name" to "Desk".toByteArray(),
                "version" to "1".toByteArray(),
                "control_port" to "3769".toByteArray(),
                "capabilities" to "h264,opus,rtp,secure_pairing,encrypted_rtp".toByteArray(),
            ),
            host = "10.0.0.4",
        )

        assertTrue(result is DiscoveryParseResult.Valid)
        assertEquals("Desk", result.advertisement.receiverName)
        assertEquals("10.0.0.4", result.advertisement.host)
    }

    @Test
    fun rejectsPlainReceiverWithoutEncryptionCapabilities() {
        val result = MdnsTxtParser.parse(
            mapOf(
                "name" to "Old Receiver",
                "version" to "1",
                "control_port" to "3769",
                "capabilities" to "h264,opus,rtp",
            ),
        )

        assertEquals(
            DiscoveryParseResult.Invalid("receiver does not support secure encrypted pairing"),
            result,
        )
    }
}
