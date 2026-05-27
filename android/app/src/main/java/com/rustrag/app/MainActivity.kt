package com.rustrag.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.animation.*
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import com.rustrag.app.data.SearchResultPayload
import com.rustrag.app.data.RagApiService
import com.rustrag.app.data.TokenManager
import com.rustrag.app.ui.screens.LoginScreen
import com.rustrag.app.ui.screens.Screen
import com.rustrag.app.ui.screens.SearchScreen
import com.rustrag.app.ui.screens.SearchTab
import com.rustrag.app.ui.screens.DetailScreen
import com.rustrag.app.data.AppSearchHelper
import com.rustrag.app.ui.theme.RustRagTheme

class MainActivity : ComponentActivity() {

    private lateinit var tokenManager: TokenManager
    private lateinit var apiService: RagApiService
    private lateinit var appSearchHelper: AppSearchHelper

    // Live search query from incoming intents
    private val searchQueryState = mutableStateOf("")
    private val shouldFocusSearchState = mutableStateOf(false)
    private val detailItemIdState = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        
        // Setup dynamic shortcuts
        ShortcutsHelper.setupShortcuts(this)

        val app = application as RagApplication
        tokenManager = app.tokenManager
        apiService = app.apiService
        appSearchHelper = app.appSearchHelper

        intent?.let { handleIntent(it) }

        setContent {
            RustRagTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    var currentScreen by remember {
                        mutableStateOf<Screen>(
                            if (tokenManager.isConfigured) Screen.Search else Screen.Login
                        )
                    }

                    var searchQueryText by remember { mutableStateOf("") }
                    var searchSourceFilter by remember { mutableStateOf("") }
                    var searchResults by remember { mutableStateOf<List<SearchResultPayload>>(emptyList()) }
                    var isSearching by remember { mutableStateOf(false) }

                    var searchModeAssisted by remember { mutableStateOf(false) }
                    var searchModeHybrid by remember { mutableStateOf(true) }
                    var searchModeRerank by remember { mutableStateOf(true) }

                    var activeSearchQuery by searchQueryState
                    var shouldFocusSearch by shouldFocusSearchState
                    var pendingDetailId by detailItemIdState
                    var activeTab by remember { mutableStateOf(SearchTab.MEMORIES) }

                    // If focus is requested, auto navigate to search screen
                    LaunchedEffect(shouldFocusSearch) {
                        if (shouldFocusSearch && currentScreen !is Screen.Search && tokenManager.isConfigured) {
                            currentScreen = Screen.Search
                        }
                    }

                    // If a detail ID is requested, auto navigate to detail screen
                    LaunchedEffect(pendingDetailId) {
                        val id = pendingDetailId
                        if (id != null && tokenManager.isConfigured) {
                            currentScreen = Screen.Detail(id)
                            pendingDetailId = null // Clear it immediately
                        }
                    }

                    // Automatic search/recent-loader effect when search query is blank
                    LaunchedEffect(currentScreen, searchSourceFilter, searchQueryText, activeTab) {
                        if (currentScreen is Screen.Search && tokenManager.isConfigured) {
                            if (searchQueryText.isBlank()) {
                                isSearching = true
                                try {
                                    searchResults = apiService.getRecentEntries(
                                        limit = if (activeTab == SearchTab.TODOS) 50 else 15,
                                        sourceId = searchSourceFilter.takeIf { it.isNotBlank() },
                                        typeName = if (activeTab == SearchTab.TODOS) "todo" else null
                                    )
                                } catch (e: Exception) {
                                    // Log or handle error silently or via Toast
                                } finally {
                                    isSearching = false
                                }
                            }
                        }
                    }

                    // Automatic search update when active tab, source filter, or screen changes with an active query text
                    LaunchedEffect(currentScreen, searchSourceFilter, activeTab) {
                        if (currentScreen is Screen.Search && tokenManager.isConfigured && searchQueryText.isNotBlank()) {
                            isSearching = true
                            try {
                                val resp = apiService.search(
                                    queryText = searchQueryText,
                                    sourceId = searchSourceFilter.takeIf { it.isNotBlank() },
                                    hybrid = searchModeHybrid,
                                    rerank = if (searchModeRerank) true else null,
                                    typeName = if (activeTab == SearchTab.TODOS) "todo" else null
                                )
                                searchResults = resp.results
                            } catch (e: Exception) {
                                // Log or handle error silently
                            } finally {
                                isSearching = false
                            }
                        }
                    }

                    // If a search query is triggered from intent, auto navigate to search screen
                    LaunchedEffect(activeSearchQuery) {
                        if (activeSearchQuery.isNotEmpty()) {
                            searchQueryText = activeSearchQuery
                            currentScreen = Screen.Search
                            activeSearchQuery = "" // Clear it immediately

                            isSearching = true
                            try {
                                val resp = apiService.search(
                                    queryText = searchQueryText,
                                    sourceId = searchSourceFilter.takeIf { it.isNotBlank() },
                                    hybrid = searchModeHybrid,
                                    rerank = if (searchModeRerank) true else null,
                                    typeName = if (activeTab == SearchTab.TODOS) "todo" else null
                                )
                                searchResults = resp.results
                            } catch (e: Exception) {
                                // Handle error
                            } finally {
                                isSearching = false
                            }
                        }
                    }

                    // Index memories to AppSearch whenever searchResults changes
                    LaunchedEffect(searchResults) {
                        if (searchResults.isNotEmpty()) {
                            appSearchHelper.indexMemories(searchResults)

                            // Also update the widget cache if we have a recent list loaded
                            val latest = searchResults.firstOrNull()
                            if (latest != null) {
                                tokenManager.widgetLatestText = latest.text
                                tokenManager.widgetLatestId = latest.id
                                RagSearchWidgetProvider.triggerUpdate(applicationContext)
                            }
                        }
                    }

                    AnimatedContent(
                        targetState = currentScreen,
                        transitionSpec = {
                            val isGoingForward = when {
                                initialState is Screen.Login && targetState is Screen.Search -> true
                                initialState is Screen.Search && targetState is Screen.Detail -> true
                                else -> false
                            }
                            if (isGoingForward) {
                                slideInHorizontally { width -> width } + fadeIn() togetherWith
                                        slideOutHorizontally { width -> -width } + fadeOut()
                            } else {
                                slideInHorizontally { width -> -width } + fadeIn() togetherWith
                                        slideOutHorizontally { width -> width } + fadeOut()
                            }
                        },
                        label = "screen_transition"
                    ) { screen ->
                        when (screen) {
                            is Screen.Login -> {
                                LoginScreen(
                                    apiService = apiService,
                                    tokenManager = tokenManager,
                                    onLoginSuccess = {
                                        currentScreen = Screen.Search
                                    }
                                )
                            }
                            is Screen.Search -> {
                                SearchScreen(
                                    apiService = apiService,
                                    queryText = searchQueryText,
                                    onQueryTextChange = { searchQueryText = it },
                                    sourceIdFilter = searchSourceFilter,
                                    onSourceIdFilterChange = { searchSourceFilter = it },
                                    searchModeAssisted = searchModeAssisted,
                                    onSearchModeAssistedChange = { searchModeAssisted = it },
                                    searchModeHybrid = searchModeHybrid,
                                    onSearchModeHybridChange = { searchModeHybrid = it },
                                    searchModeRerank = searchModeRerank,
                                    onSearchModeRerankChange = { searchModeRerank = it },
                                    results = searchResults,
                                    onResultsChange = { searchResults = it },
                                    isSearching = isSearching,
                                    onIsSearchingChange = { isSearching = it },
                                    shouldFocusSearch = shouldFocusSearch,
                                    onFocusConsumed = { shouldFocusSearch = false },
                                    onNavigateToDetail = { id ->
                                        currentScreen = Screen.Detail(id)
                                    },
                                    activeTab = activeTab,
                                    onTabChange = { activeTab = it },
                                    onLogout = {
                                        tokenManager.clear()
                                        searchQueryText = ""
                                        searchSourceFilter = ""
                                        searchResults = emptyList()
                                        searchModeAssisted = false
                                        searchModeHybrid = true
                                        searchModeRerank = true
                                        currentScreen = Screen.Login
                                    }
                                )
                            }
                            is Screen.Detail -> {
                                DetailScreen(
                                    itemId = screen.itemId,
                                    apiService = apiService,
                                    onBack = {
                                        currentScreen = Screen.Search
                                    }
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent) {
        if (intent.action == Intent.ACTION_VIEW) {
            val data = intent.data

            // Handle detail deep link: rustrag://detail?id=xxx
            if (data?.scheme == "rustrag" && data.host == "detail") {
                val id = data.getQueryParameter("id")
                if (!id.isNullOrBlank()) {
                    detailItemIdState.value = id
                    return
                }
            }

            // Extract from standard query param or from Gemini bundle extra
            val query = data?.getQueryParameter("query") ?: intent.getStringExtra("query")
            if (!query.isNullOrBlank()) {
                searchQueryState.value = query
            }
            if (data?.getQueryParameter("focus") == "true" || data?.host == "search") {
                shouldFocusSearchState.value = true
            }
        }
    }
}
