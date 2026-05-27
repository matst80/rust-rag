package com.rustrag.app.ui.components

import android.widget.Toast
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.rustrag.app.data.RagApiService
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Composable
fun CreateTodoDialog(
    apiService: RagApiService,
    onDismiss: () -> Unit,
    onSuccess: () -> Unit
) {
    var title by remember { mutableStateOf("") }
    var notes by remember { mutableStateOf("") }
    var priority by remember { mutableStateOf("medium") }
    var due by remember { mutableStateOf("") }
    var sourceId by remember { mutableStateOf("todos") }
    var isSaving by remember { mutableStateOf(false) }

    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    AlertDialog(
        onDismissRequest = { if (!isSaving) onDismiss() },
        title = { Text("Create New Task") },
        text = {
            Column(
                verticalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.fillMaxWidth()
            ) {
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    label = { Text("Task Title") },
                    placeholder = { Text("What needs to be done?") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )

                // Priority Selection
                Text("Priority", style = MaterialTheme.typography.labelMedium)
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    listOf("low", "medium", "high").forEach { p ->
                        val selected = priority == p
                        val color = when (p) {
                            "high" -> Color(0xFFFF4D4D)
                            "medium" -> Color(0xFFFFB347)
                            "low" -> Color(0xFF77DD77)
                            else -> Color.Gray
                        }
                        FilterChip(
                            selected = selected,
                            onClick = { priority = p },
                            label = { Text(p.uppercase()) },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = color.copy(alpha = 0.2f),
                                selectedLabelColor = color
                            ),
                            border = FilterChipDefaults.filterChipBorder(
                                selected = selected,
                                enabled = true,
                                selectedBorderColor = color,
                                selectedBorderWidth = 1.dp
                            )
                        )
                    }
                }

                OutlinedTextField(
                    value = due,
                    onValueChange = { due = it },
                    label = { Text("Due Date (optional)") },
                    placeholder = { Text("YYYY-MM-DD") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )

                OutlinedTextField(
                    value = notes,
                    onValueChange = { notes = it },
                    label = { Text("Notes (optional)") },
                    placeholder = { Text("Add descriptive notes here...") },
                    minLines = 2,
                    maxLines = 4,
                    modifier = Modifier.fillMaxWidth()
                )

                OutlinedTextField(
                    value = sourceId,
                    onValueChange = { sourceId = it },
                    label = { Text("Namespace") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    if (title.isBlank()) {
                        Toast.makeText(context, "Title cannot be empty", Toast.LENGTH_SHORT).show()
                        return@Button
                    }
                    isSaving = true
                    scope.launch {
                        try {
                            val todoData = buildJsonObject {
                                put("title", title.trim())
                                put("status", "open")
                                put("priority", priority)
                                if (due.isNotBlank()) put("due", due.trim())
                                if (notes.isNotBlank()) put("notes", notes.trim())
                            }
                            apiService.storeEntry(
                                text = title.trim(),
                                sourceId = sourceId,
                                typeName = "todo",
                                data = todoData
                            )
                            Toast.makeText(context, "Task created!", Toast.LENGTH_SHORT).show()
                            onSuccess()
                        } catch (e: Exception) {
                            Toast.makeText(context, "Failed to create: ${e.message}", Toast.LENGTH_LONG).show()
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
                    Text("Create")
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
