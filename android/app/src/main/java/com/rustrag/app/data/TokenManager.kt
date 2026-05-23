package com.rustrag.app.data

import android.content.Context
import android.content.SharedPreferences

class TokenManager(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("rag_prefs", Context.MODE_PRIVATE)

    companion object {
        private const val KEY_SERVER_URL = "server_url"
        private const val KEY_TOKEN = "auth_token"
        private const val DEFAULT_SERVER_URL = "https://rag.k6n.net"
    }

    var serverUrl: String
        get() = prefs.getString(KEY_SERVER_URL, DEFAULT_SERVER_URL) ?: DEFAULT_SERVER_URL
        set(value) {
            val normalized = value.trim().removeSuffix("/")
            prefs.edit().putString(KEY_SERVER_URL, normalized).apply()
        }

    var token: String?
        get() = prefs.getString(KEY_TOKEN, null)
        set(value) {
            prefs.edit().putString(KEY_TOKEN, value?.trim()).apply()
        }

    val isConfigured: Boolean
        get() = !token.isNullOrBlank()

    fun clear() {
        prefs.edit().remove(KEY_TOKEN).apply()
    }
}
