package com.acamera.android.media

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals

class AndroidMediaControllersTest {
    @Test
    fun h264SplitterExtractsAnnexBNalUnits() {
        val units = H264AccessUnitSplitter.nalUnits(
            byteArrayOf(
                0, 0, 0, 1, 0x67, 1, 2,
                0, 0, 1, 0x68, 3,
                0, 0, 0, 1, 0x65, 4, 5,
            ),
        )

        assertEquals(3, units.size)
        assertContentEquals(byteArrayOf(0x67, 1, 2), units[0])
        assertContentEquals(byteArrayOf(0x68, 3), units[1])
        assertContentEquals(byteArrayOf(0x65, 4, 5), units[2])
    }

    @Test
    fun h264SplitterExtractsLengthPrefixedNalUnits() {
        val units = H264AccessUnitSplitter.nalUnits(
            byteArrayOf(
                0, 0, 0, 3, 0x67, 1, 2,
                0, 0, 0, 2, 0x65, 3,
            ),
        )

        assertEquals(2, units.size)
        assertContentEquals(byteArrayOf(0x67, 1, 2), units[0])
        assertContentEquals(byteArrayOf(0x65, 3), units[1])
    }

    @Test
    fun h264SplitterFallsBackToWholeAccessUnitForUnknownShape() {
        val accessUnit = byteArrayOf(0x65, 1, 2, 3)

        val units = H264AccessUnitSplitter.nalUnits(accessUnit)

        assertEquals(1, units.size)
        assertContentEquals(accessUnit, units.single())
    }
}
