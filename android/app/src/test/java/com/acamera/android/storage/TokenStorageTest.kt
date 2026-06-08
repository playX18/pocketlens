package com.acamera.android.storage

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class TokenStorageTest {
    @Test
    fun storesReadsAndClearsReceiverToken() {
        val storage = InMemoryTokenStorage()

        storage.writeToken("Receiver", "token-123")
        assertEquals("token-123", storage.readToken("Receiver"))

        storage.clearToken("Receiver")
        assertNull(storage.readToken("Receiver"))
    }
}
