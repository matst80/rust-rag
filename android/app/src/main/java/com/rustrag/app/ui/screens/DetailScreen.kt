package com.rustrag.app.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Save
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import com.rustrag.app.RagApplication
import com.rustrag.app.data.AdminItemPayload
import com.rustrag.app.data.RagApiService
import com.rustrag.app.data.StoreAnalysis
import com.rustrag.app.data.StoreAnalysisVerdict
import com.rustrag.app.data.EntryNeighbor
import com.rustrag.app.data.GemmaManager
import com.rustrag.app.ui.theme.RustRagTheme
import com.rustrag.app.ui.components.ConnectedNeighborRow
import com.rustrag.app.ui.components.MarkdownText
import com.rustrag.app.ui.components.GemmaRefineButton
import com.rustrag.app.ui.components.ModernTextField
import java.text.SimpleDateFormat
import java.util.*
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.ui.tooling.preview.Preview

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DetailScreen(
    itemId: String,
    apiService: RagApiService,
    onBack: () -> Unit,
    modifier: Modifier = Modifier
) {
    var currentId by remember(itemId) { mutableStateOf(itemId) }
    val backStack = remember { mutableStateListOf<String>() }

    var isLoading by remember { mutableStateOf(true) }
    var itemDetail by remember { mutableStateOf<AdminItemPayload?>(null) }
    var errorText by remember { mutableStateOf<String?>(null) }

    var isEditing by remember { mutableStateOf(false) }
    var editContent by remember { mutableStateOf("") }
    var editSourceId by remember { mutableStateOf("") }
    var editPath by remember { mutableStateOf("") }
    var isSavingEdits by remember { mutableStateOf(false) }

    val context = LocalContext.current
    val app = context.applicationContext as RagApplication
    val gemmaManager = app.gemmaManager
    val scope = rememberCoroutineScope()

    LaunchedEffect(itemDetail) {
        itemDetail?.let {
            editContent = it.text
            editSourceId = it.sourceId
            editPath = it.path ?: ""
        }
    }

    val handleBack: () -> Unit = {
        if (isEditing) {
            isEditing = false
            itemDetail?.let {
                editContent = it.text
                editSourceId = it.sourceId
                editPath = it.path ?: ""
            }
        } else if (backStack.isNotEmpty()) {
            currentId = backStack.removeLast()
        } else {
            onBack()
        }
        Unit
    }

    // Intercept back button/gesture to navigate back within the app rather than exiting
    BackHandler(enabled = true) {
        handleBack()
    }

    LaunchedEffect(currentId) {
        isLoading = true
        errorText = null
        try {
            val detail = apiService.getEntryDetail(currentId)
            itemDetail = detail
        } catch (e: Exception) {
            errorText = e.message
        } finally {
            isLoading = false
        }
    }

    val copyPathToClipboard = {
        val detail = itemDetail
        if (detail != null) {
            val pathVal = detail.path ?: detail.id
            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            val clip = ClipData.newPlainText("RAG Path", pathVal)
            clipboard.setPrimaryClip(clip)
            Toast.makeText(context, "Copied: $pathVal", Toast.LENGTH_SHORT).show()
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    val detail = itemDetail
                    if (detail != null) {
                        Text(
                            text = detail.analysis?.title ?: detail.path ?: detail.id.take(8),
                            style = MaterialTheme.typography.titleMedium.copy(
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace
                            ),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    } else {
                        Text("Loading Entry...")
                    }
                },
                navigationIcon = {
                    IconButton(onClick = handleBack) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = "Back"
                        )
                    }
                },
                actions = {
                    val detail = itemDetail
                    if (detail != null) {
                        if (isEditing) {
                            IconButton(
                                onClick = {
                                    if (editContent.isBlank()) {
                                        Toast.makeText(context, "Content cannot be empty", Toast.LENGTH_SHORT).show()
                                    } else {
                                        isSavingEdits = true
                                        scope.launch {
                                            try {
                                                val updated = apiService.updateEntry(
                                                    id = detail.id,
                                                    text = editContent,
                                                    sourceId = editSourceId,
                                                    path = editPath,
                                                    metadata = detail.metadata,
                                                    typeName = detail.typeName,
                                                    data = detail.data
                                                )
                                                itemDetail = updated
                                                isEditing = false
                                                Toast.makeText(context, "Saved successfully", Toast.LENGTH_SHORT).show()
                                            } catch (e: Exception) {
                                                Toast.makeText(context, "Failed to save: ${e.message}", Toast.LENGTH_LONG).show()
                                            } finally {
                                                isSavingEdits = false
                                            }
                                        }
                                    }
                                },
                                enabled = !isSavingEdits
                            ) {
                                if (isSavingEdits) {
                                    CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                                } else {
                                    Icon(
                                        imageVector = Icons.Default.Save,
                                        contentDescription = "Save Changes"
                                    )
                                }
                            }
                            IconButton(
                                onClick = {
                                    isEditing = false
                                    editContent = detail.text
                                    editSourceId = detail.sourceId
                                    editPath = detail.path ?: ""
                                },
                                enabled = !isSavingEdits
                            ) {
                                Icon(
                                    imageVector = Icons.Default.Close,
                                    contentDescription = "Cancel Edit"
                                )
                            }
                        } else {
                            IconButton(onClick = { isEditing = true }) {
                                Icon(
                                    imageVector = Icons.Default.Edit,
                                    contentDescription = "Edit Entry"
                                )
                            }
                            IconButton(onClick = copyPathToClipboard) {
                                Icon(
                                    imageVector = Icons.Default.ContentCopy,
                                    contentDescription = "Copy Path"
                                )
                            }
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                    titleContentColor = MaterialTheme.colorScheme.onBackground,
                    navigationIconContentColor = MaterialTheme.colorScheme.onBackground,
                    actionIconContentColor = MaterialTheme.colorScheme.onBackground
                )
            )
        },
        modifier = modifier
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(MaterialTheme.colorScheme.background)
                .padding(innerPadding)
        ) {
            AnimatedContent(
                targetState = Triple(isLoading, errorText, itemDetail),
                transitionSpec = {
                    fadeIn() togetherWith fadeOut()
                },
                label = "detail_content_transition",
                modifier = Modifier.fillMaxSize()
            ) { (loading, error, detail) ->
                if (loading) {
                    Box(
                        modifier = Modifier.fillMaxSize(),
                        contentAlignment = Alignment.Center
                    ) {
                        CircularProgressIndicator(
                            color = MaterialTheme.colorScheme.primary
                        )
                    }
                } else if (error != null) {
                    Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(24.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center
                    ) {
                        Text(
                            text = "Error loading entry details",
                            style = MaterialTheme.typography.titleMedium.copy(
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.error
                            )
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = error,
                            textAlign = TextAlign.Center,
                            style = MaterialTheme.typography.bodyMedium
                        )
                        Spacer(modifier = Modifier.height(20.dp))
                        Button(onClick = handleBack) { Text("Go Back") }
                    }
                } else if (detail != null) {
                    val dateStr = remember(detail.createdAt) {
                        val sdf = SimpleDateFormat("MMMM d, yyyy 'at' HH:mm:ss", Locale.getDefault())
                        sdf.format(Date(detail.createdAt))
                    }

                    LazyColumn(
                        modifier = Modifier.fillMaxSize(),
                        contentPadding = PaddingValues(16.dp),
                        verticalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        if (isEditing) {
                            item {
                                EditViewBody(
                                    editSourceId = editSourceId,
                                    onSourceIdChange = { editSourceId = it },
                                    editPath = editPath,
                                    onPathChange = { editPath = it },
                                    editContent = editContent,
                                    onContentChange = { editContent = it },
                                    gemmaManager = gemmaManager
                                )
                            }
                        } else {
                            // 1. Header Section
                            item {
                                DetailHeaderSection(detail = detail, dateStr = dateStr)
                            }

                            // 2. AI Executive Summary (Top Placement)
                            detail.analysis?.let { analysis ->
                                if (analysis.summary != null) {
                                    item {
                                        AiSummarySection(analysis = analysis)
                                    }
                                }
                            }

                            // 3. Document Content Block
                            item {
                                Column(modifier = Modifier.fillMaxWidth()) {
                                    Text(
                                        text = "SOURCE CONTENT",
                                        style = MaterialTheme.typography.labelSmall.copy(
                                            fontWeight = FontWeight.Bold,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                                            letterSpacing = 0.5.sp
                                        ),
                                        modifier = Modifier.padding(bottom = 6.dp)
                                    )
                                    HorizontalDivider(
                                        color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f),
                                        modifier = Modifier.padding(bottom = 8.dp)
                                    )
                                    MarkdownText(
                                        markdown = detail.text,
                                        modifier = Modifier.fillMaxWidth()
                                    )
                                }
                            }

                            // 4. Related & Similar Entries
                            if (!detail.neighbors.isNullOrEmpty()) {
                                item {
                                    Column(modifier = Modifier.fillMaxWidth()) {
                                        Spacer(modifier = Modifier.height(8.dp))
                                        Row(
                                            verticalAlignment = Alignment.CenterVertically,
                                            modifier = Modifier.padding(bottom = 6.dp)
                                        ) {
                                            Text(text = "🌱", fontSize = 14.sp)
                                            Spacer(modifier = Modifier.width(8.dp))
                                            Text(
                                                text = "RELATED & SIMILAR ENTRIES",
                                                style = MaterialTheme.typography.labelSmall.copy(
                                                    fontWeight = FontWeight.Bold,
                                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                                    letterSpacing = 0.5.sp
                                                )
                                            )
                                        }
                                        HorizontalDivider(
                                            color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f)
                                        )
                                    }
                                }

                                itemsIndexed(detail.neighbors) { index, neighbor ->
                                    val verdict = detail.analysis?.verdicts?.firstOrNull { it.targetId == neighbor.id }
                                    ConnectedNeighborRow(
                                        neighbor = neighbor,
                                        verdict = verdict,
                                        onClick = {
                                            backStack.add(currentId)
                                            currentId = neighbor.id
                                        }
                                    )
                                    if (index < detail.neighbors.lastIndex) {
                                        HorizontalDivider(
                                            color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
                                        )
                                    }
                                }
                            }

                            item {
                                Spacer(modifier = Modifier.height(24.dp))
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun DetailHeaderSection(
    detail: AdminItemPayload,
    dateStr: String,
    modifier: Modifier = Modifier
) {
    val titleText = detail.analysis?.title ?: detail.path ?: detail.id.take(8)
    val hasAiTitle = detail.analysis?.title != null
    val pathText = detail.path

    Column(modifier = modifier.fillMaxWidth()) {
        // Badges Row
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            // Namespace/Source ID Badge
            AssistChip(
                onClick = {},
                label = {
                    Text(
                        text = detail.sourceId.uppercase(),
                        style = MaterialTheme.typography.labelSmall.copy(
                            fontFamily = FontFamily.Monospace,
                            fontWeight = FontWeight.Bold
                        )
                    )
                },
                colors = AssistChipDefaults.assistChipColors(
                    labelColor = MaterialTheme.colorScheme.primary
                )
            )

            // Document Type Badge
            val docType = detail.analysis?.docType ?: detail.typeName
            if (!docType.isNullOrBlank()) {
                AssistChip(
                    onClick = {},
                    label = {
                        Text(
                            text = docType.uppercase(),
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontFamily = FontFamily.Monospace
                            )
                        )
                    }
                )
            }
        }

        Spacer(modifier = Modifier.height(8.dp))

        // Title
        Text(
            text = titleText,
            style = MaterialTheme.typography.headlineMedium.copy(
                fontWeight = FontWeight.Bold,
                fontFamily = if (hasAiTitle) FontFamily.Default else FontFamily.Monospace
            ),
            color = MaterialTheme.colorScheme.onSurface
        )

        // Path (if distinct from title)
        if (!pathText.isNullOrBlank() && pathText != titleText) {
            Spacer(modifier = Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(text = "📁 ", fontSize = 14.sp)
                Text(
                    text = pathText,
                    style = MaterialTheme.typography.bodyMedium.copy(
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                )
            }
        }

        Spacer(modifier = Modifier.height(6.dp))

        // Date
        Text(
            text = dateStr,
            style = MaterialTheme.typography.labelSmall.copy(
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f)
            )
        )
    }
}

@Composable
fun AiSummarySection(
    analysis: StoreAnalysis,
    modifier: Modifier = Modifier
) {
    val summary = analysis.summary ?: return

    ElevatedCard(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.15f)
        ),
        shape = RoundedCornerShape(16.dp)
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp)
        ) {
            // Header: Sparkles + Title
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Icon(
                    imageVector = Icons.Default.AutoAwesome,
                    contentDescription = "AI Analysis",
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(18.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = "AI Executive Summary",
                    style = MaterialTheme.typography.titleSmall.copy(
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.primary
                    )
                )
            }

            Spacer(modifier = Modifier.height(8.dp))

            // Summary text
            Text(
                text = summary,
                style = MaterialTheme.typography.bodyMedium.copy(
                    lineHeight = 20.sp
                ),
                color = MaterialTheme.colorScheme.onSurface
            )

            // Tags (if available)
            if (!analysis.tags.isNullOrEmpty()) {
                Spacer(modifier = Modifier.height(12.dp))
                LazyRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    items(analysis.tags) { tag ->
                        Surface(
                            shape = RoundedCornerShape(6.dp),
                            color = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f),
                            border = BorderStroke(
                                0.5.dp,
                                MaterialTheme.colorScheme.primary.copy(alpha = 0.2f)
                            )
                        ) {
                            Text(
                                text = "#$tag",
                                style = MaterialTheme.typography.labelSmall.copy(
                                    fontWeight = FontWeight.SemiBold,
                                    color = MaterialTheme.colorScheme.primary
                                ),
                                modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp)
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun EditViewBody(
    editSourceId: String,
    onSourceIdChange: (String) -> Unit,
    editPath: String,
    onPathChange: (String) -> Unit,
    editContent: String,
    onContentChange: (String) -> Unit,
    gemmaManager: GemmaManager,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(14.dp)
    ) {
        Text(
            text = "EDIT ENTRY",
            style = MaterialTheme.typography.labelSmall.copy(
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.primary,
                letterSpacing = 0.5.sp
            ),
            modifier = Modifier.padding(bottom = 4.dp)
        )

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            ModernTextField(
                value = editSourceId,
                onValueChange = onSourceIdChange,
                label = { Text("Namespace") },
                singleLine = true,
                modifier = Modifier.weight(1f)
            )

            ModernTextField(
                value = editPath,
                onValueChange = onPathChange,
                label = { Text("Path (optional)") },
                singleLine = true,
                modifier = Modifier.weight(1f)
            )
        }

        ModernTextField(
            value = editContent,
            onValueChange = onContentChange,
            label = { Text("Content (Markdown)") },
            singleLine = false,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 200.dp, max = 400.dp)
        )

        GemmaRefineButton(
            content = editContent,
            gemmaManager = gemmaManager,
            onAccept = onContentChange,
            modifier = Modifier.align(Alignment.End)
        )
    }
}

@Preview(showBackground = true)
@Composable
fun DetailScreenPreview() {
    val mockDetail = AdminItemPayload(
        id = "12345678",
        text = "# Rust RAG Architecture\nThis is the main documentation body of the entry.\n\nIt contains structured info about vector db storage.",
        metadata = buildJsonObject {},
        sourceId = "rust-rag",
        createdAt = System.currentTimeMillis() - 3600000,
        updatedAt = System.currentTimeMillis(),
        path = "docs/architecture.md",
        typeName = "markdown",
        data = null,
        neighbors = listOf(
            EntryNeighbor(
                id = "87654321",
                title = "Vector DB Comparison",
                relationship = "compares",
                sourceType = "similarity"
            )
        ),
        analysis = StoreAnalysis(
            verdicts = listOf(
                StoreAnalysisVerdict(
                    targetId = "87654321",
                    relation = "agrees",
                    confidence = 0.85f,
                    reason = "Both documents discuss vectors and storage optimizations."
                )
            ),
            tags = listOf("docs", "architecture", "rag"),
            title = "Rust RAG Architecture Guide",
            summary = "This guide details the core vector architecture, using SQLite for indexing and dense/sparse retrievers.",
            docType = "documentation"
        )
    )

    RustRagTheme {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(MaterialTheme.colorScheme.background)
        ) {
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp)
            ) {
                item {
                    DetailHeaderSection(detail = mockDetail, dateStr = "May 22, 2026 at 19:00:00")
                }
                item {
                    mockDetail.analysis?.let {
                        AiSummarySection(analysis = it)
                    }
                }
                item {
                    Column(modifier = Modifier.fillMaxWidth()) {
                        Text(
                            text = "SOURCE CONTENT",
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                                letterSpacing = 0.5.sp
                            ),
                            modifier = Modifier.padding(bottom = 6.dp)
                        )
                        HorizontalDivider(
                            color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f),
                            modifier = Modifier.padding(bottom = 8.dp)
                        )
                        MarkdownText(
                            markdown = mockDetail.text,
                            modifier = Modifier.fillMaxWidth()
                        )
                    }
                }
            }
        }
    }
}
