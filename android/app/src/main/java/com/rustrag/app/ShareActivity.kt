package com.rustrag.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.rustrag.app.data.RagApiService
import com.rustrag.app.data.TokenManager
import com.rustrag.app.ui.screens.ShareScreen
import com.rustrag.app.ui.theme.RustRagTheme

class ShareActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val sharedText = if (intent.action == Intent.ACTION_SEND && intent.type == "text/plain") {
            intent.getStringExtra(Intent.EXTRA_TEXT) ?: ""
        } else {
            ""
        }

        val app = application as RagApplication
        val tokenManager = app.tokenManager
        val apiService = app.apiService

        setContent {
            RustRagTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color.Transparent
                ) {
                    ShareScreen(
                        sharedText = sharedText,
                        apiService = apiService,
                        tokenManager = tokenManager,
                        onFinish = { _ ->
                            finish()
                        }
                    )
                }
            }
        }
    }
}
