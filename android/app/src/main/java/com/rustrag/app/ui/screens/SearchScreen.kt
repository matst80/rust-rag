package com.rustrag.app.ui.screens

import android.widget.Toast
import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustrag.app.data.RagApiService
import com.rustrag.app.data.SearchResultPayload
import com.rustrag.app.ui.components.SearchResultRow
import com.rustrag.app.ui.components.ModernTextField
import com.rustrag.app.ui.components.SearchModeButton
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.foundation.BorderStroke
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import androidx.compose.ui.graphics.lerp as lerpColor
import androidx.compose.ui.unit.lerp as lerpDp

import androidx.compose.ui.tooling.preview.Preview
import com.rustrag.app.ui.theme.RustRagTheme
import kotlinx.serialization.json.buildJsonObject

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchScreen(
    apiService: RagApiService,
    queryText: String,
    onQueryTextChange: (String) -> Unit,
    sourceIdFilter: String,
    onSourceIdFilterChange: (String) -> Unit,
    searchModeAssisted: Boolean,
    onSearchModeAssistedChange: (Boolean) -> Unit,
    searchModeHybrid: Boolean,
    onSearchModeHybridChange: (Boolean) -> Unit,
    searchModeRerank: Boolean,
    onSearchModeRerankChange: (Boolean) -> Unit,
    results: List<SearchResultPayload>,
    onResultsChange: (List<SearchResultPayload>) -> Unit,
    isSearching: Boolean,
    onIsSearchingChange: (Boolean) -> Unit,
    shouldFocusSearch: Boolean,
    onFocusConsumed: () -> Unit,
    onNavigateToDetail: (String) -> Unit,
    onLogout: () -> Unit,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    val searchAction = {
        onIsSearchingChange(true)
        scope.launch {
            try {
                if (queryText.isNotBlank()) {
                    val resp = apiService.search(
                        queryText = queryText,
                        sourceId = sourceIdFilter.takeIf { it.isNotBlank() },
                        hybrid = searchModeHybrid,
                        rerank = if (searchModeRerank) true else null
                    )
                    onResultsChange(resp.results)
                    if (resp.results.isEmpty()) {
                        Toast.makeText(context, "No results found", Toast.LENGTH_SHORT).show()
                    }
                } else {
                    val recent = apiService.getRecentEntries(limit = 15, sourceId = sourceIdFilter.takeIf { it.isNotBlank() })
                    onResultsChange(recent)
                }
            } catch (e: Exception) {
                Toast.makeText(context, "Error: ${e.message}", Toast.LENGTH_LONG).show()
            } finally {
                onIsSearchingChange(false)
            }
        }
    }

    SearchScreenContent(
        queryText = queryText,
        onQueryTextChange = onQueryTextChange,
        sourceIdFilter = sourceIdFilter,
        onSourceIdFilterChange = onSourceIdFilterChange,
        searchModeAssisted = searchModeAssisted,
        onSearchModeAssistedChange = onSearchModeAssistedChange,
        searchModeHybrid = searchModeHybrid,
        onSearchModeHybridChange = onSearchModeHybridChange,
        searchModeRerank = searchModeRerank,
        onSearchModeRerankChange = onSearchModeRerankChange,
        results = results,
        isSearching = isSearching,
        shouldFocusSearch = shouldFocusSearch,
        onFocusConsumed = onFocusConsumed,
        onNavigateToDetail = onNavigateToDetail,
        onLogout = onLogout,
        onSearch = { searchAction() },
        modifier = modifier
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchScreenContent(
    queryText: String,
    onQueryTextChange: (String) -> Unit,
    sourceIdFilter: String,
    onSourceIdFilterChange: (String) -> Unit,
    searchModeAssisted: Boolean,
    onSearchModeAssistedChange: (Boolean) -> Unit,
    searchModeHybrid: Boolean,
    onSearchModeHybridChange: (Boolean) -> Unit,
    searchModeRerank: Boolean,
    onSearchModeRerankChange: (Boolean) -> Unit,
    results: List<SearchResultPayload>,
    isSearching: Boolean,
    shouldFocusSearch: Boolean,
    onFocusConsumed: () -> Unit,
    onNavigateToDetail: (String) -> Unit,
    onLogout: () -> Unit,
    onSearch: () -> Unit,
    modifier: Modifier = Modifier
) {
    val scope = rememberCoroutineScope()

    val focusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current

    val primaryColor = MaterialTheme.colorScheme.primary
    val surfaceColor = MaterialTheme.colorScheme.surface

    val transitionProgress = remember { Animatable(if (shouldFocusSearch) 0f else 1f) }
    var isMorphing by remember { mutableStateOf(shouldFocusSearch) }
    var isSearchFocused by remember { mutableStateOf(false) }

    LaunchedEffect(shouldFocusSearch) {
        if (shouldFocusSearch) {
            isMorphing = true
            transitionProgress.snapTo(0f)
            focusRequester.requestFocus()
            delay(100)
            keyboardController?.show()
            
            transitionProgress.animateTo(
                targetValue = 1f,
                animationSpec = tween(durationMillis = 500, easing = FastOutSlowInEasing)
            )
            isMorphing = false
            onFocusConsumed()
        }
    }

    val searchPadding = lerpDp(48.dp, 0.dp, transitionProgress.value)
    val searchBorderColor = lerpColor(Color(0x3300F2FF), primaryColor, transitionProgress.value)
    val searchBgColor = lerpColor(Color(0xFF121212), surfaceColor, transitionProgress.value)
    
    val targetAlpha = if (isSearchFocused && queryText.isEmpty()) 0.4f else 1.0f
    val contentAlpha by animateFloatAsState(
        targetValue = targetAlpha,
        animationSpec = tween(durationMillis = 300),
        label = "content_alpha"
    )

    var showCustomSourceDialog by remember { mutableStateOf(false) }
    var customSourceInput by remember { mutableStateOf("") }
    var customSources by remember { mutableStateOf(setOf<String>()) }
    var isNamespacesExpanded by remember { mutableStateOf(false) }

    val allSources = remember(customSources) {
        listOf(
            "All" to "",
            "Inbox" to "inbox",
            "Wiki" to "wiki",
            "Knowledge" to "knowledge",
            "Todos" to "todos"
        ) + customSources.map { it.replaceFirstChar { c -> c.uppercase() } to it }
    }

    if (showCustomSourceDialog) {
        AlertDialog(
            onDismissRequest = { showCustomSourceDialog = false },
            title = { Text("Add Custom Namespace") },
            text = {
                ModernTextField(
                    value = customSourceInput,
                    onValueChange = { customSourceInput = it },
                    label = { Text("Namespace (source_id)") },
                    placeholder = { Text("e.g. journal, books") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )
            },
                            // Confirm Button
                            confirmButton = {
                                Button(
                                    onClick = {
                                        val cleaned = customSourceInput.trim().lowercase()
                                        if (cleaned.isNotEmpty()) {
                                            customSources = customSources + cleaned
                                            onSourceIdFilterChange(cleaned)
                                            customSourceInput = ""
                                            showCustomSourceDialog = false
                                            onSearch()
                                        }
                                    },
                                    shape = RoundedCornerShape(24.dp)
                                ) {
                                    Text("Add")
                                }
                            },
            dismissButton = {
                TextButton(onClick = { showCustomSourceDialog = false }) {
                    Text("Cancel")
                }
            }
        )
    }

    Scaffold(
//        topBar = {
//            TopAppBar(
//                title = {
//                    Row(
//                        verticalAlignment = Alignment.CenterVertically
//                    ) {
//                        Image(
//                            painter = painterResource(id = com.rustrag.app.R.drawable.ic_brain),
//                            contentDescription = "Second Brain Logo",
//                            modifier = Modifier.size(28.dp)
//                        )
//                        Spacer(modifier = Modifier.width(10.dp))
//                        Text(
//                            text = "rust-rag",
//                            style = MaterialTheme.typography.titleMedium.copy(
//                                fontWeight = FontWeight.Bold,
//                                fontFamily = FontFamily.Monospace,
//                                letterSpacing = 1.sp
//                            )
//                        )
//                    }
//                },
//                actions = {
//                    IconButton(onClick = onLogout) {
//                        Icon(
//                            imageVector = Icons.AutoMirrored.Filled.Logout,
//                            contentDescription = "Log Out"
//                        )
//                    }
//                },
//                colors = TopAppBarDefaults.topAppBarColors(
//                    containerColor = MaterialTheme.colorScheme.background,
//                    titleContentColor = MaterialTheme.colorScheme.onBackground,
//                    actionIconContentColor = MaterialTheme.colorScheme.onBackground
//                ),
//                modifier = Modifier.alpha(contentAlpha)
//            )
//        },
        modifier = modifier
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .background(MaterialTheme.colorScheme.background)
                .padding(innerPadding)
                .padding(horizontal = 16.dp)
        ) {
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(14.dp),
                contentPadding = PaddingValues(bottom = 24.dp)
            ) {
                // Top Search Section + Mode Toggles Card
                item {
                    Spacer(modifier = Modifier.height(16.dp))
                    
                    Card(
                        shape = RoundedCornerShape(24.dp),
                        //border = BorderStroke(1.dp, searchBorderColor),
                        colors = CardDefaults.cardColors(
                            containerColor = searchBgColor
                        ),
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = searchPadding)
                    ) {
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 4.dp, vertical = 4.dp)
                        ) {
                            // Text Input Field (integrated seamlessly)
                            TextField(
                                value = queryText,
                                onValueChange = { onQueryTextChange(it) },
                                placeholder = {
                                    Text(
                                        text = "Search your knowledge base...",
                                        style = MaterialTheme.typography.bodyMedium.copy(
                                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f)
                                        )
                                    )
                                },
                                leadingIcon = {
                                    AnimatedContent(
                                        targetState = isMorphing,
                                        transitionSpec = {
                                            fadeIn(animationSpec = tween(300)) togetherWith fadeOut(animationSpec = tween(300))
                                        },
                                        label = "leading_icon_transition"
                                    ) { morphing ->
                                        if (morphing) {
                                            Image(
                                                painter = painterResource(id = com.rustrag.app.R.drawable.ic_brain),
                                                contentDescription = "Brain Icon",
                                                modifier = Modifier.size(24.dp)
                                            )
                                        } else {
                                            Icon(
                                                imageVector = Icons.Default.Search,
                                                contentDescription = "Search Icon",
                                                tint = MaterialTheme.colorScheme.primary
                                            )
                                        }
                                    }
                                },
                                trailingIcon = {
                                    if (queryText.isNotEmpty()) {
                                        IconButton(
                                            onClick = {
                                                onQueryTextChange("")
                                                scope.launch {
                                                    delay(50)
                                                    onSearch()
                                                }
                                            }
                                        ) {
                                            Icon(
                                                imageVector = Icons.Default.Clear,
                                                contentDescription = "Clear text",
                                                tint = MaterialTheme.colorScheme.onSurfaceVariant
                                            )
                                        }
                                    }
                                },
                                singleLine = true,
                                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                                keyboardActions = KeyboardActions(onSearch = { onSearch() }),
                                colors = TextFieldDefaults.colors(
                                    focusedContainerColor = Color.Transparent,
                                    unfocusedContainerColor = Color.Transparent,
                                    focusedIndicatorColor = Color.Transparent,
                                    unfocusedIndicatorColor = Color.Transparent,
                                    disabledIndicatorColor = Color.Transparent,
                                    focusedTextColor = Color.White,
                                    unfocusedTextColor = Color.White
                                ),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .focusRequester(focusRequester)
                                    .onFocusChanged { isSearchFocused = it.isFocused }
                            )

//                            HorizontalDivider(
//                                color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.2f),
//                                modifier = Modifier.padding(horizontal = 12.dp)
//                            )
                            
                            Spacer(modifier = Modifier.height(4.dp))
                            
                            // Connected Toggle Buttons inside the search card (below the divider)
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(start = 12.dp, end = 12.dp, bottom = 8.dp),
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                SearchModeButton(
                                    text = "ASSISTED",
                                    selected = searchModeAssisted,
                                    onClick = {
                                        onSearchModeAssistedChange(!searchModeAssisted)
                                        scope.launch { delay(50); onSearch() }
                                    }
                                )
                                SearchModeButton(
                                    text = "HYBRID",
                                    selected = searchModeHybrid,
                                    onClick = {
                                        onSearchModeHybridChange(!searchModeHybrid)
                                        scope.launch { delay(50); onSearch() }
                                    }
                                )
                                SearchModeButton(
                                    text = "RERANK",
                                    selected = searchModeRerank,
                                    onClick = {
                                        onSearchModeRerankChange(!searchModeRerank)
                                        scope.launch { delay(50); onSearch() }
                                    }
                                )
                            }
                        }
                    }
                }

                // Namespace Filter Row with expandable content (hidden by default)
                item {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .alpha(contentAlpha)
                    ) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { isNamespacesExpanded = !isNamespacesExpanded }
                                .padding(vertical = 4.dp),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(
                                text = "FILTER",
                                style = MaterialTheme.typography.labelSmall.copy(
                                    fontWeight = FontWeight.Bold,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                                    letterSpacing = 0.5.sp
                                )
                            )
                            
                            Icon(
                                imageVector = if (isNamespacesExpanded) Icons.Default.ExpandLess else Icons.Default.ExpandMore,
                                contentDescription = if (isNamespacesExpanded) "Collapse" else "Expand",
                                tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                                modifier = Modifier.size(18.dp)
                            )
                        }
                        
                        AnimatedVisibility(
                            visible = isNamespacesExpanded,
                            enter = expandVertically() + fadeIn(),
                            exit = shrinkVertically() + fadeOut()
                        ) {
                            LazyRow(
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(top = 4.dp)
                            ) {
                                items(allSources) { (label, value) ->
                                    val selected = sourceIdFilter == value
                                    FilterChip(
                                        selected = selected,
                                        onClick = {
                                            onSourceIdFilterChange(value)
                                            scope.launch { delay(50); onSearch() }
                                        },
                                        label = {
                                            Text(
                                                text = label,
                                                style = MaterialTheme.typography.labelSmall,
                                                maxLines = 1,
                                                overflow = TextOverflow.Ellipsis
                                            )
                                        }
                                    )
                                }

                                item {
                                    InputChip(
                                        selected = false,
                                        onClick = { showCustomSourceDialog = true },
                                        label = {
                                            Text(
                                                text = "+ CUSTOM",
                                                style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold),
                                                maxLines = 1,
                                                overflow = TextOverflow.Ellipsis
                                            )
                                        },
                                        colors = InputChipDefaults.inputChipColors(
                                            labelColor = MaterialTheme.colorScheme.primary
                                        )
                                    )
                                }
                            }
                        }
                    }
                }

                // Header title before memories list
                item {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 4.dp)
                            .alpha(contentAlpha),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            text = if (queryText.isNotBlank()) "SEARCH RESULTS" else "RECENT MEMORIES",
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary,
                                letterSpacing = 1.sp
                            )
                        )

                        if (queryText.isNotBlank()) {
                            Text(
                                text = "CLEAR QUERY",
                                style = MaterialTheme.typography.labelSmall.copy(
                                    fontWeight = FontWeight.Bold,
                                    color = MaterialTheme.colorScheme.error
                                ),
                                modifier = Modifier
                                    .clickable {
                                        onQueryTextChange("")
                                        scope.launch { delay(50); onSearch() }
                                    }
                                    .padding(4.dp)
                            )
                        }
                    }
                }

                // Search Results / Memories List
                item {
                    Box(modifier = Modifier.alpha(contentAlpha)) {
                        AnimatedContent(
                            targetState = Triple(isSearching, results.isEmpty(), results),
                            transitionSpec = {
                                fadeIn() togetherWith fadeOut()
                            },
                            label = "search_results_transition"
                        ) { (searching, empty, items) ->
                            if (searching) {
                                Box(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .height(200.dp),
                                    contentAlignment = Alignment.Center
                                ) {
                                    CircularProgressIndicator(
                                        color = MaterialTheme.colorScheme.primary
                                    )
                                }
                            } else if (empty) {
                                Box(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .height(200.dp),
                                    contentAlignment = Alignment.Center
                                ) {
                                    Text(
                                        text = "No memories loaded.",
                                        style = MaterialTheme.typography.bodyMedium.copy(
                                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                                            fontFamily = FontFamily.Monospace
                                        )
                                    )
                                }
                            } else {
                                Column(modifier = Modifier.fillMaxWidth()) {
                                    items.forEachIndexed { index, item ->
                                        SearchResultRow(
                                            item = item,
                                            onClick = { onNavigateToDetail(item.id) }
                                        )
                                        if (index < items.lastIndex) {
                                            HorizontalDivider(
                                                color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
fun SearchScreenPreview() {
    val results = listOf(
        SearchResultPayload(
            id = "1",
            text = "This is a sample search result about rust-rag features. It explains how the RAG system integrates with Android.",
            metadata = buildJsonObject { },
            sourceId = "wiki",
            createdAt = System.currentTimeMillis(),
            updatedAt = System.currentTimeMillis(),
            distance = 0.2f,
            path = "wiki/intro.md"
        ),
        SearchResultPayload(
            id = "2",
            text = "A recent memory captured from a conversation about Jetpack Compose previews.",
            metadata = buildJsonObject { },
            sourceId = "inbox",
            createdAt = System.currentTimeMillis() - 3600000,
            updatedAt = System.currentTimeMillis() - 3600000,
            distance = 0.5f,
            path = null
        ),
        SearchResultPayload(
            id = "3",
            text = "Technical documentation for the OkHttpClient integration in the Rust-RAG project.",
            metadata = buildJsonObject { },
            sourceId = "knowledge",
            createdAt = System.currentTimeMillis() - 86400000,
            updatedAt = System.currentTimeMillis() - 86400000,
            distance = -1f,
            path = "docs/network.kt"
        )
    )

    RustRagTheme {
        SearchScreenContent(
            queryText = "rust-rag",
            onQueryTextChange = {},
            sourceIdFilter = "",
            onSourceIdFilterChange = {},
            searchModeAssisted = false,
            onSearchModeAssistedChange = {},
            searchModeHybrid = true,
            onSearchModeHybridChange = {},
            searchModeRerank = true,
            onSearchModeRerankChange = {},
            results = results,
            isSearching = false,
            shouldFocusSearch = false,
            onFocusConsumed = {},
            onNavigateToDetail = {},
            onLogout = {},
            onSearch = {}
        )
    }
}
