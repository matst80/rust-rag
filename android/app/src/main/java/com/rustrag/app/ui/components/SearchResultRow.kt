package com.rustrag.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustrag.app.data.SearchResultPayload
import com.rustrag.app.data.StoreAnalysis
import java.text.SimpleDateFormat
import java.util.*
import androidx.compose.ui.tooling.preview.Preview
import com.rustrag.app.ui.theme.RustRagTheme
import kotlinx.serialization.json.buildJsonObject

@Composable
fun SearchResultRow(
    item: SearchResultPayload,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val dateStr = remember(item.createdAt) {
        val sdf = SimpleDateFormat("MMM d, yyyy HH:mm", Locale.getDefault())
        sdf.format(Date(item.createdAt))
    }

    val similarityPct = remember(item.distance) {
        if (item.distance == -1f) {
            "RECENT"
        } else {
            val sim = (1.0f - item.distance).coerceIn(0.0f, 1.0f) * 100
            String.format(Locale.US, "%.0f%% MATCH", sim)
        }
    }

    val isStrongMatch = item.distance != -1f && item.distance < 0.35f
    val badgeContainerColor = when {
        item.distance == -1f -> MaterialTheme.colorScheme.surfaceVariant
        item.distance < 0.35f -> MaterialTheme.colorScheme.primaryContainer
        item.distance < 0.6f -> MaterialTheme.colorScheme.secondaryContainer
        else -> MaterialTheme.colorScheme.surfaceVariant
    }
    
    val badgeContentColor = when {
        item.distance == -1f -> MaterialTheme.colorScheme.onSurfaceVariant
        item.distance < 0.35f -> MaterialTheme.colorScheme.onPrimaryContainer
        item.distance < 0.6f -> MaterialTheme.colorScheme.onSecondaryContainer
        else -> MaterialTheme.colorScheme.onSurfaceVariant
    }

    val titleText = item.analysis?.title ?: item.path ?: item.id.take(8)
    val hasAiTitle = item.analysis?.title != null
    val displayBody = item.analysis?.summary ?: item.text
    val isSummary = item.analysis?.summary != null


        Column(
            modifier = modifier
                .fillMaxWidth()
                .padding(vertical = 12.dp)
                .clickable(onClick = onClick)
        ) {
            // Header Row
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                // Source ID / Namespace badge
                Row(
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Box(
                        modifier = Modifier
                            .size(6.dp)
                            .clip(CircleShape)
                            .background(
                                if (isStrongMatch) MaterialTheme.colorScheme.primary 
                                else MaterialTheme.colorScheme.outline
                            )
                    )
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(
                        text = item.sourceId.uppercase(),
                        style = MaterialTheme.typography.labelSmall.copy(
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace,
                            color = if (isStrongMatch) MaterialTheme.colorScheme.primary 
                                    else MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    )
                }

                // Match percentage / Score indicator badge
                Surface(
                    shape = RoundedCornerShape(6.dp),
                    color = badgeContainerColor,
                    modifier = Modifier.height(18.dp)
                ) {
                    Box(
                        modifier = Modifier.padding(horizontal = 6.dp),
                        contentAlignment = Alignment.Center
                    ) {
                        Text(
                            text = similarityPct,
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                                fontSize = 8.sp,
                                color = badgeContentColor
                            )
                        )
                    }
                }
            }

            Spacer(modifier = Modifier.height(8.dp))

            // Title
            Text(
                text = titleText,
                style = MaterialTheme.typography.titleSmall.copy(
                    fontWeight = FontWeight.Bold,
                    fontFamily = if (hasAiTitle) FontFamily.Default else FontFamily.Monospace
                ),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )

            // Tags row (if available)
            val tags = item.analysis?.tags
            if (!tags.isNullOrEmpty()) {
                Spacer(modifier = Modifier.height(6.dp))
                LazyRow(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    items(tags) { tag ->
                        Surface(
                            shape = RoundedCornerShape(4.dp),
                            color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f),
                            border = BorderStroke(0.5.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
                        ) {
                            Text(
                                text = "#$tag",
                                style = MaterialTheme.typography.labelSmall.copy(
                                    fontSize = 8.sp,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                ),
                                modifier = Modifier.padding(horizontal = 4.dp, vertical = 1.dp)
                            )
                        }
                    }
                }
            }

            Spacer(modifier = Modifier.height(8.dp))

            // Text Snippet or AI Summary
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.Top
            ) {
                if (isSummary) {
                    Text(
                        text = "✨ ",
                        fontSize = 12.sp,
                        modifier = Modifier.padding(top = 1.dp)
                    )
                }
                Text(
                    text = displayBody,
                    style = MaterialTheme.typography.bodyMedium.copy(
                        lineHeight = 18.sp
                    ),
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                    color = if (isSummary) MaterialTheme.colorScheme.onSurface 
                            else MaterialTheme.colorScheme.onSurfaceVariant
                )
            }

            Spacer(modifier = Modifier.height(10.dp))

            // Footer Row
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                if (!item.path.isNullOrBlank() && item.path != titleText) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.weight(1f, fill = false)
                    ) {
                        Text(
                            text = "📁",
                            fontSize = 10.sp
                        )
                        Spacer(modifier = Modifier.width(4.dp))
                        Text(
                            text = item.path,
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontFamily = FontFamily.Monospace,
                                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.8f),
                                fontSize = 9.sp
                            ),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                } else {
                    Spacer(modifier = Modifier.weight(1f))
                }

                Spacer(modifier = Modifier.width(8.dp))

                Text(
                    text = dateStr,
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                        fontSize = 9.sp
                    )
                )
            }
        }
    }


@Preview(showBackground = true)
@Composable
fun SearchResultRowPreview() {
    val mockItem = SearchResultPayload(
        id = "12345678",
        text = "This is some markdown content about Rust RAG which acts as a search result snippet.",
        metadata = buildJsonObject {},
        sourceId = "rust-rag",
        createdAt = System.currentTimeMillis(),
        updatedAt = System.currentTimeMillis(),
        distance = 0.25f,
        path = "src/main.rs",
        analysis = StoreAnalysis(
            tags = listOf("rust", "rag", "database"),
            title = "Rust RAG Architecture Overview",
            summary = "This entry outlines the core Rust RAG architecture, highlighting the vector indexing process and retrieval mechanisms using SQLite.",
            docType = "source_code"
        )
    )
    RustRagTheme {
        Box(modifier = Modifier.padding(16.dp)) {
            SearchResultRow(
                item = mockItem,
                onClick = {}
            )
        }
    }
}
