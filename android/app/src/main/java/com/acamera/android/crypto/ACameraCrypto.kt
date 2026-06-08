package com.acamera.android.crypto

import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

private const val TAG_LEN = 16
private const val NONCE_LEN = 12

object ACameraCrypto {
    private val random = SecureRandom()

    fun randomHex(bytes: Int): String {
        val data = ByteArray(bytes)
        random.nextBytes(data)
        return data.toHex()
    }

    fun securePairingKey(
        pin: String,
        pairingId: String,
        phoneNonce: String,
        receiverNonce: String,
        phonePublicKey: String,
        receiverPublicKey: String,
    ): ByteArray = deriveKey(
        "acamera-secure-pairing-v1".encodeToByteArray(),
        pin.encodeToByteArray(),
        pairingId.encodeToByteArray(),
        phoneNonce.encodeToByteArray(),
        receiverNonce.encodeToByteArray(),
        phonePublicKey.encodeToByteArray(),
        receiverPublicKey.encodeToByteArray(),
    )

    fun decryptFromHex(key: ByteArray, aad: ByteArray, envelopeHex: String): ByteArray {
        val envelope = envelopeHex.hexToBytes()
        require(envelope.size >= NONCE_LEN + TAG_LEN) { "encrypted envelope is too short" }
        val nonce = envelope.copyOfRange(0, NONCE_LEN)
        val ciphertext = envelope.copyOfRange(NONCE_LEN, envelope.size)
        return aesGcmDecrypt(key, nonce, aad, ciphertext)
    }

    fun encryptToHex(key: ByteArray, aad: ByteArray, plaintext: ByteArray): String {
        val nonce = randomHex(NONCE_LEN).hexToBytes()
        val ciphertext = aesGcmEncrypt(key, nonce, aad, plaintext)
        return (nonce + ciphertext).toHex()
    }

    fun encryptPacket(key: ByteArray, counter: Long, aad: ByteArray, plaintext: ByteArray): ByteArray {
        val nonce = packetNonce(counter)
        val ciphertext = aesGcmEncrypt(key, nonce, aad, plaintext)
        return nonce + ciphertext
    }

    fun deriveKey(vararg parts: ByteArray): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256")
        for (part in parts) {
            digest.update(part.size.toLong().toBigEndianBytes())
            digest.update(part)
        }
        return digest.digest()
    }

    private fun aesGcmEncrypt(key: ByteArray, nonce: ByteArray, aad: ByteArray, plaintext: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(TAG_LEN * 8, nonce))
        cipher.updateAAD(aad)
        return cipher.doFinal(plaintext)
    }

    private fun aesGcmDecrypt(key: ByteArray, nonce: ByteArray, aad: ByteArray, ciphertext: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(TAG_LEN * 8, nonce))
        cipher.updateAAD(aad)
        return cipher.doFinal(ciphertext)
    }

    private fun packetNonce(counter: Long): ByteArray =
        ByteArray(4) + counter.toBigEndianBytes()
}

fun ByteArray.toHex(): String = joinToString(separator = "") { "%02x".format(it.toInt() and 0xFF) }

fun String.hexToBytes(): ByteArray {
    val clean = trim()
    require(clean.length % 2 == 0) { "hex length must be even" }
    return ByteArray(clean.length / 2) { index ->
        clean.substring(index * 2, index * 2 + 2).toInt(16).toByte()
    }
}

private fun Long.toBigEndianBytes(): ByteArray =
    ByteArray(8) { shift -> ((this ushr ((7 - shift) * 8)) and 0xFF).toByte() }
