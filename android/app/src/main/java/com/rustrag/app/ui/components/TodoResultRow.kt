package com.rustrag.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustrag.app.data.SearchResultPayload
import java.text.SimpleDateFormat
import java.util.*
import androidx.compose.ui.tooling.preview.Preview
import com.rustrag.app.ui.theme.RustRagTheme
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

@Composable
fun TodoResultRow(
    item: SearchResultPayload,
    onStatusToggle: () -> Unit,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val dateStr = remember(item.createdAt) {
        val sdf = SimpleDateFormat("MMM d, yyyy HH:mm", Locale.getDefault())
        sdf.format(Date(item.createdAt))
    }

    val todoData = remember(item.data) { item.data?.jsonObject }
    val titleText = todoData?.get("title")?.jsonPrimitive?.contentOrNull ?: item.text
    val status = todoData?.get("status")?.jsonPrimitive?.contentOrNull ?: "open"
    val priority = todoData?.get("priority")?.jsonPrimitive?.contentOrNull
    val due = todoData?.get("due")?.jsonPrimitive?.contentOrNull
    val notes = todoData?.get("notes")?.jsonPrimitive?.contentOrNull

    val isDone = status == "done"

    val priorityColor = when (priority?.lowercase()) {
        "high" -> Color(0xFFFF4D4D)
        "medium" -> Color(0xFFFFB347)
        "low" -> Color(0xFF77DD77)
        else -> Color.Gray
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 12.dp),
        verticalAlignment = Alignment.Top
    ) {
        // Status Checkbox
        Box(
            modifier = Modifier
                .padding(top = 2.dp, end = 12.dp)
                .size(22.dp)
                .clip(CircleShape)
                .background(
                    if (isDone) MaterialTheme.colorScheme.primary.copy(alpha = 0.2f)
                    else Color.Transparent
                )
                .border(
                    width = 1.5.dp,
                    color = if (isDone) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
                    shape = CircleShape
                )
                .clickable(onClick = onStatusToggle),
            contentAlignment = Alignment.Center
        ) {
            if (isDone) {
                Icon(
                    imageVector = Icons.Default.Check,
                    contentDescription = "Completed",
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(14.dp)
                )
            }
        }

        // Todo details Column
        Column(
            modifier = Modifier.weight(1f)
        ) {
            // Title text (Strikethrough if done)
            Text(
                text = titleText,
                style = MaterialTheme.typography.titleMedium.copy(
                    fontWeight = FontWeight.Bold,
                    textDecoration = if (isDone) TextDecoration.LineThrough else TextDecoration.None
                ),
                color = if (isDone) MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f)
                else MaterialTheme.colorScheme.onSurface,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis
            )

            // Optional notes/description
            if (!notes.isNullOrBlank() && notes != titleText) {
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = notes,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis
                )
            }

            Spacer(modifier = Modifier.height(8.dp))

            // Metadata Chips Row (Priority, Due Date, Namespace)
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                // Namespace Badge
                Surface(
                    shape = RoundedCornerShape(4.dp),
                    color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f),
                    border = BorderStroke(0.5.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
                ) {
                    Text(
                        text = item.sourceId.uppercase(),
                        style = MaterialTheme.typography.labelSmall.copy(
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace,
                            fontSize = 8.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        ),
                        modifier = Modifier.padding(horizontal = 4.dp, vertical = 1.dp)
                    )
                }

                // Priority Badge
                if (!priority.isNullOrBlank()) {
                    Surface(
                        shape = RoundedCornerShape(4.dp),
                        color = priorityColor.copy(alpha = 0.1f),
                        border = BorderStroke(0.5.dp, priorityColor.copy(alpha = 0.5f))
                    ) {
                        Text(
                            text = priority.uppercase(),
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                                fontSize = 8.sp,
                                color = priorityColor
                            ),
                            modifier = Modifier.padding(horizontal = 4.dp, vertical = 1.dp)
                        )
                    }
                }

                // Due Date Badge
                if (!due.isNullOrBlank()) {
                    Surface(
                        shape = RoundedCornerShape(4.dp),
                        color = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.4f),
                        border = BorderStroke(0.5.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.3f))
                    ) {
                        Text(
                            text = "📅 $due",
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                                fontSize = 8.sp,
                                color = MaterialTheme.colorScheme.primary
                            ),
                            modifier = Modifier.padding(horizontal = 4.dp, vertical = 1.dp)
                        )
                    }
                }

                Spacer(modifier = Modifier.weight(1f))

                // Date Created
                Text(
                    text = dateStr,
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                        fontSize = 9.sp
                    )
                )
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
fun TodoResultRowPreview() {
    val mockTodo = SearchResultPayload(
        id = "todo-1",
        text = "Finish writing the project report for RAG",
        metadata = buildJsonObject {},
        sourceId = "todos",
        createdAt = System.currentTimeMillis(),
        updatedAt = System.currentTimeMillis(),
        distance = -1f,
        path = "projects/rust-rag",
        typeName = "todo",
        data = buildJsonObject {
            put("title", "Finish writing the project report for RAG")
            put("status", "open")
            put("priority", "high")
            put("due", "2226-05-24")
            put("notes", "Make sure to cover ONNX embedder performance details and WorkManager background worker setup.")
        }
    )
    RustRagTheme {
        Box(modifier = Modifier.padding(16.dp)) {
            TodoResultRow(
                item = mockTodo,
                onStatusToggle = {},
                onClick = {}
            )
        }
    }
}
