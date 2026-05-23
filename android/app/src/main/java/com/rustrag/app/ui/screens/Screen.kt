package com.rustrag.app.ui.screens

sealed interface Screen {
    data object Login : Screen
    data object Search : Screen
    data class Detail(val itemId: String) : Screen
}
