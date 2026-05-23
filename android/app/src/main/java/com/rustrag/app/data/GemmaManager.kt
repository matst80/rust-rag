package com.rustrag.app.data

import android.content.Context
import android.net.Uri
import com.google.ai.edge.litertlm.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL

enum class GemmaStatus {
    NOT_IMPORTED,
    IDLE,
    INITIALIZING,
    READY,
    GENERATING,
    ERROR
}

class GemmaManager(private val context: Context) {
    private val modelFileName = "gemma-local.litertlm"
    private val modelFile = File(context.filesDir, modelFileName)

    private val _status = MutableStateFlow(if (modelFile.exists()) GemmaStatus.IDLE else GemmaStatus.NOT_IMPORTED)
    val status: StateFlow<GemmaStatus> = _status.asStateFlow()

    private val _importProgress = MutableStateFlow(0f)
    val importProgress: StateFlow<Float> = _importProgress.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private var engine: Engine? = null

    fun checkModelStatus() {
        if (modelFile.exists()) {
            if (_status.value == GemmaStatus.NOT_IMPORTED) {
                _status.value = GemmaStatus.IDLE
            }
        } else {
            release()
            _status.value = GemmaStatus.NOT_IMPORTED
        }
    }

    suspend fun importModel(uri: Uri): Boolean = withContext(Dispatchers.IO) {
        _status.value = GemmaStatus.INITIALIZING
        _error.value = null
        _importProgress.value = 0f
        try {
            val contentResolver = context.contentResolver
            val totalBytes = contentResolver.openAssetFileDescriptor(uri, "r")?.use { it.length } ?: -1L
            contentResolver.openInputStream(uri)?.use { input ->
                val tempFile = File(context.filesDir, "${modelFileName}.tmp")
                FileOutputStream(tempFile).use { output ->
                    val buffer = ByteArray(128 * 1024) // 128KB buffer
                    var bytesCopied = 0L
                    var bytesRead: Int
                    var lastReportedTime = System.currentTimeMillis()

                    while (input.read(buffer).also { bytesRead = it } != -1) {
                        output.write(buffer, 0, bytesRead)
                        bytesCopied += bytesRead
                        if (totalBytes > 0) {
                            val currentTime = System.currentTimeMillis()
                            if (currentTime - lastReportedTime > 100) {
                                val progress = bytesCopied.toFloat() / totalBytes.toFloat()
                                _importProgress.value = progress
                                lastReportedTime = currentTime
                            }
                        }
                    }
                }
                
                if (tempFile.renameTo(modelFile)) {
                    _status.value = GemmaStatus.IDLE
                    return@withContext true
                } else {
                    throw IllegalStateException("Failed to rename temp model file")
                }
            } ?: throw IllegalArgumentException("Could not open input stream for Uri")
        } catch (e: Exception) {
            val tempFile = File(context.filesDir, "${modelFileName}.tmp")
            if (tempFile.exists()) tempFile.delete()
            _error.value = "Import failed: ${e.message}"
            _status.value = GemmaStatus.NOT_IMPORTED
            false
        }
    }

    suspend fun downloadModel(urlStr: String): Boolean = withContext(Dispatchers.IO) {
        _status.value = GemmaStatus.INITIALIZING
        _error.value = null
        _importProgress.value = 0f
        var connection: HttpURLConnection? = null
        val tempFile = File(context.filesDir, "${modelFileName}.tmp")
        try {
            var currentUrl = urlStr
            var redirectCount = 0
            val maxRedirects = 5
            var responseCode = -1

            while (redirectCount < maxRedirects) {
                val url = URL(currentUrl)
                connection = url.openConnection() as HttpURLConnection
                connection.connectTimeout = 15000
                connection.readTimeout = 15000
                connection.requestMethod = "GET"
                connection.instanceFollowRedirects = true

                responseCode = connection.responseCode
                if (responseCode == HttpURLConnection.HTTP_MOVED_PERM ||
                    responseCode == HttpURLConnection.HTTP_MOVED_TEMP ||
                    responseCode == 307 ||
                    responseCode == 308
                ) {
                    val newUrl = connection.getHeaderField("Location")
                    if (newUrl.isNullOrEmpty()) {
                        break
                    }
                    currentUrl = newUrl
                    redirectCount++
                    connection.disconnect()
                } else {
                    break
                }
            }

            val conn = connection ?: throw java.io.IOException("Connection failed")
            if (responseCode != HttpURLConnection.HTTP_OK) {
                throw java.io.IOException("Server returned HTTP $responseCode")
            }

            val totalBytes = conn.contentLengthLong
            conn.inputStream.use { input ->
                FileOutputStream(tempFile).use { output ->
                    val buffer = ByteArray(128 * 1024) // 128KB buffer
                    var bytesCopied = 0L
                    var bytesRead: Int
                    var lastReportedTime = System.currentTimeMillis()

                    while (input.read(buffer).also { bytesRead = it } != -1) {
                        output.write(buffer, 0, bytesRead)
                        bytesCopied += bytesRead
                        val currentTime = System.currentTimeMillis()
                        if (currentTime - lastReportedTime > 100) {
                            if (totalBytes > 0) {
                                val progress = bytesCopied.toFloat() / totalBytes.toFloat()
                                _importProgress.value = progress
                            } else {
                                _importProgress.value = -1f // Indeterminate
                            }
                            lastReportedTime = currentTime
                        }
                    }
                }

                if (tempFile.renameTo(modelFile)) {
                    _status.value = GemmaStatus.IDLE
                    return@withContext true
                } else {
                    throw IllegalStateException("Failed to rename temp model file")
                }
            }
        } catch (e: Exception) {
            if (tempFile.exists()) tempFile.delete()
            _error.value = "Download failed: ${e.message}"
            _status.value = GemmaStatus.NOT_IMPORTED
            false
        } finally {
            connection?.disconnect()
        }
    }

    suspend fun initialize(): Boolean = withContext(Dispatchers.IO) {
        if (engine != null) {
            _status.value = GemmaStatus.READY
            return@withContext true
        }

        if (!modelFile.exists()) {
            _status.value = GemmaStatus.NOT_IMPORTED
            _error.value = "Model file not found"
            return@withContext false
        }

        _status.value = GemmaStatus.INITIALIZING
        _error.value = null
        try {
            val engineConfig = EngineConfig(
                modelPath = modelFile.absolutePath,
                backend = Backend.CPU()
            )

            engine = Engine(engineConfig).apply {
                initialize()
            }
            _status.value = GemmaStatus.READY
            true
        } catch (e: Exception) {
            _error.value = "Initialization failed: ${e.message}"
            _status.value = GemmaStatus.ERROR
            false
        }
    }

    suspend fun generateStreaming(prompt: String, onUpdate: (String, Boolean) -> Unit) {
        val currentEngine = engine ?: run {
            val success = initialize()
            if (!success) {
                onUpdate("Error: Local model is not ready.", true)
                return
            }
            engine
        }

        if (currentEngine == null) {
            onUpdate("Error: Local engine is null.", true)
            return
        }

        _status.value = GemmaStatus.GENERATING
        try {
            withContext(Dispatchers.IO) {
                val conversationConfig = ConversationConfig(
                    systemInstruction = Contents.of("You are a helpful writing assistant.")
                )
                currentEngine.createConversation(conversationConfig).use { conversation ->
                    var accumulatedResponse = ""
                    conversation.sendMessageAsync(prompt).collect { token ->
                        accumulatedResponse += token
                        onUpdate(accumulatedResponse, false)
                    }
                    onUpdate(accumulatedResponse, true)
                }
                _status.value = GemmaStatus.READY
            }
        } catch (e: Exception) {
            _error.value = "Generation failed: ${e.message}"
            _status.value = GemmaStatus.ERROR
            onUpdate("Error: ${e.message}", true)
        }
    }

    fun release() {
        engine?.close()
        engine = null
        if (modelFile.exists()) {
            _status.value = GemmaStatus.IDLE
        } else {
            _status.value = GemmaStatus.NOT_IMPORTED
        }
    }

    fun deleteModelFile() {
        release()
        if (modelFile.exists()) {
            modelFile.delete()
        }
        _status.value = GemmaStatus.NOT_IMPORTED
    }
}
