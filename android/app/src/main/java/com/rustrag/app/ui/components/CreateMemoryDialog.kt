package com.rustrag.app.ui.components

import android.widget.Toast
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.rustrag.app.data.RagApiService
import kotlinx.coroutines.launch

@Composable
fun CreateMemoryDialog(
    apiService: RagApiService,
    onDismiss: () -> Unit,
    onSuccess: () -> Unit
) {
    var textContent by remember { mutableStateOf("") }
    var sourceId by remember { mutableStateOf("inbox") }
    var path by remember { mutableStateOf("") }
    var isSaving by remember { mutableStateOf(false) }

    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    AlertDialog(
        onDismissRequest = { if (!isSaving) onDismiss() },
        title = { Text("Create New Memory") },
        text = {
            Column(
                verticalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.fillMaxWidth()
            ) {
                OutlinedTextField(
                    value = textContent,
                    onValueChange = { textContent = it },
                    label = { Text("Content") },
                    placeholder = { Text("Write your note here...") },
                    minLines = 3,
                    maxLines = 6,
                    modifier = Modifier.fillMaxWidth()
                )

                OutlinedTextField(
                    value = sourceId,
                    onValueChange = { sourceId = it },
                    label = { Text("Namespace") },
                    placeholder = { Text("e.g. inbox, wiki") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )

                OutlinedTextField(
                    value = path,
                    onValueChange = { path = it },
                    label = { Text("Path (optional)") },
                    placeholder = { Text("e.g. personal/ideas") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    if (textContent.isBlank()) {
                        Toast.makeText(context, "Content cannot be empty", Toast.LENGTH_SHORT).show()
                        return@Button
                    }
                    isSaving = true
                    scope.launch {
                        try {
                            apiService.storeEntry(
                                text = textContent,
                                sourceId = sourceId,
                                path = path.takeIf { it.isNotBlank() }
                            )
                            Toast.makeText(context, "Memory saved!", Toast.LENGTH_SHORT).show()
                            onSuccess()
                        } catch (e: Exception) {
                            Toast.makeText(context, "Failed to save: ${e.message}", Toast.LENGTH_LONG).show()
                        } finally {
                            isSaving = false
                        }
                    }
                },
                enabled = !isSaving,
                shape = RoundedCornerShape(24.dp)
            ) {
                if (isSaving) {
                    CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary, strokeWidth = 2.dp)
                } else {
                    Text("Save")
                }
            }
        },
        dismissButton = {
            TextButton(
                onClick = onDismiss,
                enabled = !isSaving
            ) {
                Text("Cancel")
            }
        }
    )
}
