package com.rustrag.app.ui.screens

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustrag.app.RagApplication
import com.rustrag.app.data.RagApiService
import com.rustrag.app.data.TokenManager
import com.rustrag.app.ui.components.GemmaRefineButton
import com.rustrag.app.ui.components.ModernTextField
import com.rustrag.app.ui.theme.RustRagTheme
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareScreen(
    sharedText: String,
    apiService: RagApiService,
    tokenManager: TokenManager,
    onFinish: (Boolean) -> Unit,
    modifier: Modifier = Modifier
) {
    var sourceId by remember { mutableStateOf("inbox") }
    var path by remember { mutableStateOf("") }
    var textContent by remember { mutableStateOf(sharedText) }
    var isSaving by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    val saveAction = {
        if (textContent.isBlank()) {
            Toast.makeText(context, "Content cannot be empty", Toast.LENGTH_SHORT).show()
        } else {
            isSaving = true
            scope.launch {
                try {
                    val result = apiService.storeEntry(textContent, sourceId, path)
                    tokenManager.widgetLatestText = textContent
                    tokenManager.widgetLatestId = result.id
                    com.rustrag.app.RagSearchWidgetProvider.triggerUpdate(context)
                    Toast.makeText(context, "Persisted in RAG successfully!", Toast.LENGTH_SHORT).show()
                    onFinish(true)
                } catch (e: Exception) {
                    Toast.makeText(context, "Save failed: ${e.message}", Toast.LENGTH_LONG).show()
                    isSaving = false
                }
            }
        }
    }

    ShareScreenContent(
        textContent = textContent,
        onTextContentChange = { textContent = it },
        sourceId = sourceId,
        onSourceIdChange = { sourceId = it },
        path = path,
        onPathChange = { path = it },
        isSaving = isSaving,
        isConfigured = tokenManager.isConfigured,
        onFinish = onFinish,
        onSave = { saveAction() },
        modifier = modifier
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareScreenContent(
    textContent: String,
    onTextContentChange: (String) -> Unit,
    sourceId: String,
    onSourceIdChange: (String) -> Unit,
    path: String,
    onPathChange: (String) -> Unit,
    isSaving: Boolean,
    isConfigured: Boolean,
    onFinish: (Boolean) -> Unit,
    onSave: () -> Unit,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .navigationBarsPadding()
            .padding(16.dp)
    ) {
        // Premium Header
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(bottom = 16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            IconButton(
                onClick = { onFinish(false) },
                enabled = !isSaving
            ) {
                Icon(
                    imageVector = Icons.Default.Close,
                    contentDescription = "Cancel",
                    tint = MaterialTheme.colorScheme.onBackground
                )
            }

            Text(
                text = "Save to rust-rag",
                style = MaterialTheme.typography.titleLarge.copy(
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.primary
                )
            )

            Button(
                onClick = { onSave() },
                enabled = !isSaving && textContent.isNotBlank(),
                shape = RoundedCornerShape(24.dp)
            ) {
                if (isSaving) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        color = MaterialTheme.colorScheme.onPrimary,
                        strokeWidth = 2.dp
                    )
                } else {
                    Text("Store")
                }
            }
        }

        if (!isConfigured) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                contentAlignment = Alignment.Center
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                    modifier = Modifier.padding(horizontal = 24.dp)
                ) {
                    Text(
                        text = "Authentication Required",
                        style = MaterialTheme.typography.titleMedium.copy(
                            fontWeight = FontWeight.Bold,
                            color = MaterialTheme.colorScheme.error
                        ),
                        textAlign = TextAlign.Center
                    )
                    Text(
                        text = "Please open the main app to configure connection and log in.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.7f),
                        textAlign = TextAlign.Center
                    )
                }
            }
        } else {
            // Text Editor filling the rest of the space
            ModernTextField(
                value = textContent,
                onValueChange = onTextContentChange,
                label = { Text("Shared Content") },
                singleLine = false,
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
            )

            Spacer(modifier = Modifier.height(8.dp))

            val app = context.applicationContext as? RagApplication
            if (app != null) {
                GemmaRefineButton(
                    content = textContent,
                    gemmaManager = app.gemmaManager,
                    onAccept = onTextContentChange,
                    modifier = Modifier.align(Alignment.End)
                )
            }

            Spacer(modifier = Modifier.height(12.dp))

            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                ModernTextField(
                    value = sourceId,
                    onValueChange = onSourceIdChange,
                    label = { Text("Namespace") },
                    singleLine = true,
                    modifier = Modifier.weight(1f)
                )

                ModernTextField(
                    value = path,
                    onValueChange = onPathChange,
                    label = { Text("Path (optional)") },
                    singleLine = true,
                    modifier = Modifier.weight(1f)
                )
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
fun ShareScreenPreview() {
    RustRagTheme {
        ShareScreenContent(
            textContent = "This is a sample text that was shared from another app. It could be a long article, a code snippet, or just a simple note that I want to store in my Rust-RAG knowledge base for later retrieval.",
            onTextContentChange = {},
            sourceId = "inbox",
            onSourceIdChange = {},
            path = "shared/links",
            onPathChange = {},
            isSaving = false,
            isConfigured = true,
            onFinish = {},
            onSave = {}
        )
    }
}

@Preview(showBackground = true)
@Composable
fun ShareScreenNotConfiguredPreview() {
    RustRagTheme {
        ShareScreenContent(
            textContent = "Shared text",
            onTextContentChange = {},
            sourceId = "inbox",
            onSourceIdChange = {},
            path = "",
            onPathChange = {},
            isSaving = false,
            isConfigured = false,
            onFinish = {},
            onSave = {}
        )
    }
}

