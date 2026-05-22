package com.rustrag.app.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustrag.app.data.*
import com.rustrag.app.ui.components.ModernTextField
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LoginScreen(
    apiService: RagApiService,
    tokenManager: TokenManager,
    onLoginSuccess: () -> Unit,
    modifier: Modifier = Modifier
) {
    var serverUrl by remember { mutableStateOf(tokenManager.serverUrl) }
    var deviceCodeResponse by remember { mutableStateOf<DeviceCodeResponse?>(null) }
    var isPolling by remember { mutableStateOf(false) }
    var statusText by remember { mutableStateOf("") }
    var isLoading by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    // Polling Coroutine
    if (isPolling && deviceCodeResponse != null) {
        LaunchedEffect(deviceCodeResponse) {
            val resp = deviceCodeResponse!!
            val intervalMs = (resp.interval.coerceAtLeast(1) * 1000)
            while (isPolling) {
                delay(intervalMs)
                try {
                    when (val result = apiService.pollDeviceToken(resp.deviceCode)) {
                        is PollResult.Success -> {
                            tokenManager.token = result.tokenResponse.accessToken
                            isPolling = false
                            statusText = "Login successful!"
                            Toast.makeText(context, "Successfully logged in!", Toast.LENGTH_SHORT).show()
                            onLoginSuccess()
                        }
                        is PollResult.Pending -> {
                            statusText = "Waiting for approval on server..."
                        }
                        is PollResult.SlowDown -> {
                            statusText = "Polling too fast. Slowing down..."
                            delay(2000)
                        }
                        is PollResult.AccessDenied -> {
                            isPolling = false
                            statusText = "Access denied by user."
                        }
                        is PollResult.Expired -> {
                            isPolling = false
                            statusText = "Device code expired. Please try again."
                        }
                        is PollResult.Error -> {
                            isPolling = false
                            statusText = "Error: ${result.message}"
                        }
                    }
                } catch (e: Exception) {
                    statusText = "Network error: ${e.localizedMessage ?: e.message}. Retrying..."
                    delay(3000)
                }
            }
        }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    colors = listOf(
                        MaterialTheme.colorScheme.background,
                        MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.2f)
                    )
                )
            )
            .padding(24.dp),
        contentAlignment = Alignment.Center
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center
        ) {
            Text(
                text = "rust-rag client",
                style = MaterialTheme.typography.headlineLarge.copy(
                    fontWeight = FontWeight.Bold
                ),
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(bottom = 8.dp)
            )

            Text(
                text = "Connect your self-hosted RETRIEVAL + AGENT memory",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(bottom = 32.dp)
            )

            if (deviceCodeResponse == null) {
                ModernTextField(
                    value = serverUrl,
                    onValueChange = { serverUrl = it },
                    label = { Text("Server Base URL") },
                    placeholder = { Text("https://rag.example.com") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )

                Spacer(modifier = Modifier.height(20.dp))

                Button(
                    onClick = {
                        isLoading = true
                        tokenManager.serverUrl = serverUrl
                        tokenManager.clear()
                        scope.launch {
                            try {
                                val codeResp = apiService.requestDeviceCode("Android App")
                                deviceCodeResponse = codeResp
                                isPolling = true
                                statusText = "Verification code generated."
                            } catch (e: Exception) {
                                Toast.makeText(context, "Error: ${e.message}", Toast.LENGTH_LONG).show()
                            } finally {
                                isLoading = false
                            }
                        }
                    },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(50.dp),
                    enabled = !isLoading,
                    shape = RoundedCornerShape(24.dp)
                ) {
                    if (isLoading) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(20.dp),
                            color = MaterialTheme.colorScheme.onPrimary,
                            strokeWidth = 2.dp
                        )
                    } else {
                        Text(
                            text = "Connect via Device Flow",
                            style = MaterialTheme.typography.labelLarge.copy(
                                fontWeight = FontWeight.Bold
                            )
                        )
                    }
                }
            } else {
                val resp = deviceCodeResponse!!

                OutlinedCard(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(16.dp),
                    colors = CardDefaults.outlinedCardColors(
                        containerColor = MaterialTheme.colorScheme.surface
                    ),
                    border = androidx.compose.foundation.BorderStroke(
                        1.2.dp,
                        androidx.compose.ui.graphics.Color(0x3300F2FF)
                    )
                ) {
                    Column(
                        modifier = Modifier.padding(24.dp),
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        Text(
                            text = "APPROVE THIS DEVICE",
                            style = MaterialTheme.typography.titleSmall.copy(
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                        )

                        Spacer(modifier = Modifier.height(20.dp))

                        // Large User Verification Code
                        Text(
                            text = resp.userCode,
                            style = MaterialTheme.typography.headlineMedium.copy(
                                fontWeight = FontWeight.ExtraBold,
                                fontFamily = FontFamily.Monospace,
                                color = MaterialTheme.colorScheme.tertiary,
                                letterSpacing = 2.sp
                            ),
                            modifier = Modifier
                                .clip(RoundedCornerShape(8.dp))
                                .background(MaterialTheme.colorScheme.surfaceVariant)
                                .padding(horizontal = 20.dp, vertical = 10.dp)
                        )

                        Spacer(modifier = Modifier.height(20.dp))

                        Text(
                            text = "Please enter this code at the server verification page.",
                            style = MaterialTheme.typography.bodySmall,
                            textAlign = TextAlign.Center,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )

                        Spacer(modifier = Modifier.height(24.dp))

                        Button(
                            onClick = {
                                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                val clip = ClipData.newPlainText("RAG Code", resp.userCode)
                                clipboard.setPrimaryClip(clip)
                                Toast.makeText(context, "Code copied to clipboard!", Toast.LENGTH_SHORT).show()

                                try {
                                    val builder = CustomTabsIntent.Builder()
                                    val customTabsIntent = builder.build()
                                    customTabsIntent.launchUrl(context, Uri.parse(resp.verificationUriComplete))
                                } catch (e: Exception) {
                                    val browserIntent = Intent(Intent.ACTION_VIEW, Uri.parse(resp.verificationUriComplete))
                                    context.startActivity(browserIntent)
                                }
                            },
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(48.dp),
                            shape = RoundedCornerShape(24.dp)
                        ) {
                            Text("Copy Code & Open Browser")
                        }
                    }
                }

                Spacer(modifier = Modifier.height(28.dp))

                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.Center,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.5.dp
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    Text(
                        text = statusText,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }

                Spacer(modifier = Modifier.height(20.dp))

                TextButton(
                    onClick = {
                        isPolling = false
                        deviceCodeResponse = null
                    }
                ) {
                    Text("Cancel and Reset")
                }
            }
        }
    }
}
