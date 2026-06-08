package com.pocketlens.android.storage

import android.content.Context

interface TokenStorage {
    fun readToken(receiverName: String): String?
    fun writeToken(receiverName: String, token: String)
    fun clearToken(receiverName: String)
}

class SharedPreferencesTokenStorage(context: Context) : TokenStorage {
    private val preferences = context.getSharedPreferences("pocketlens_tokens", Context.MODE_PRIVATE)

    override fun readToken(receiverName: String): String? =
        preferences.getString(key(receiverName), null)

    override fun writeToken(receiverName: String, token: String) {
        require(receiverName.isNotBlank()) { "receiverName must not be blank" }
        require(token.isNotBlank()) { "token must not be blank" }
        preferences.edit().putString(key(receiverName), token).apply()
    }

    override fun clearToken(receiverName: String) {
        preferences.edit().remove(key(receiverName)).apply()
    }

    private fun key(receiverName: String): String =
        "token:${receiverName.trim().lowercase()}"
}

class InMemoryTokenStorage : TokenStorage {
    private val tokens = linkedMapOf<String, String>()

    override fun readToken(receiverName: String): String? = tokens[receiverName]

    override fun writeToken(receiverName: String, token: String) {
        require(receiverName.isNotBlank()) { "receiverName must not be blank" }
        require(token.isNotBlank()) { "token must not be blank" }
        tokens[receiverName] = token
    }

    override fun clearToken(receiverName: String) {
        tokens.remove(receiverName)
    }
}
