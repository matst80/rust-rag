package com.rustrag.app.data

import android.content.Context
import android.content.SharedPreferences

class TokenManager(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("rag_prefs", Context.MODE_PRIVATE)

    companion object {
        private const val KEY_SERVER_URL = "server_url"
        private const val KEY_TOKEN = "auth_token"
        private const val KEY_WIDGET_LATEST_TEXT = "widget_latest_text"
        private const val KEY_WIDGET_LATEST_ID = "widget_latest_id"
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

    var widgetLatestText: String?
        get() = prefs.getString(KEY_WIDGET_LATEST_TEXT, null)
        set(value) {
            prefs.edit().putString(KEY_WIDGET_LATEST_TEXT, value?.trim()).apply()
        }

    var widgetLatestId: String?
        get() = prefs.getString(KEY_WIDGET_LATEST_ID, null)
        set(value) {
            prefs.edit().putString(KEY_WIDGET_LATEST_ID, value?.trim()).apply()
        }

    val isConfigured: Boolean
        get() = !token.isNullOrBlank()

    fun clear() {
        prefs.edit()
            .remove(KEY_TOKEN)
            .remove(KEY_WIDGET_LATEST_TEXT)
            .remove(KEY_WIDGET_LATEST_ID)
            .apply()
    }
}
