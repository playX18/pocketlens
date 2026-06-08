package com.acamera.android.crypto

import kotlin.test.Test
import kotlin.test.assertContentEquals

class ACameraCryptoTest {
    @Test
    fun aes256GcmEmptyPlaintextVectorMatchesStandard() {
        val key = ByteArray(32)
        val envelope = "000000000000000000000000530f8afbc74536b9a963b4f1c4cb738b"

        assertContentEquals(ByteArray(0), ACameraCrypto.decryptFromHex(key, ByteArray(0), envelope))
    }

    @Test
    fun encryptedEnvelopeRoundTripsAndAuthenticatesAad() {
        val key = ACameraCrypto.deriveKey("pairing".encodeToByteArray())
        val encrypted = ACameraCrypto.encryptToHex(key, "aad".encodeToByteArray(), "token".encodeToByteArray())

        assertContentEquals(
            "token".encodeToByteArray(),
            ACameraCrypto.decryptFromHex(key, "aad".encodeToByteArray(), encrypted),
        )
    }
}
