package com.rustrag.app

import androidx.appfunctions.service.AppFunction
import androidx.appfunctions.AppFunctionContext
import com.rustrag.app.data.RagApiService

class RagAppFunctions(private val apiService: RagApiService) {

    /**
     * Stores a text entry in the RAG memory database.
     *
     * @param context The function execution context.
     * @param text The text memory content to store.
     * @param sourceId The destination namespace (e.g. "inbox", "wiki", "knowledge").
     * @param path Optional wiki path structure to file it under.
     * @return The unique ID of the stored entry.
     */
    @AppFunction(isDescribedByKDoc = true)
    suspend fun storeEntry(
        context: AppFunctionContext,
        text: String,
        sourceId: String,
        path: String? = null
    ): String {
        val result = apiService.storeEntry(text = text, sourceId = sourceId, path = path)
        return result.id
    }

    /**
     * Searches the RAG second brain knowledge base for relevant memories.
     *
     * @param context The function execution context.
     * @param query The search query or semantic question.
     * @param sourceId Optional namespace or category to filter by (e.g., "wiki").
     * @return A list of matching text snippets retrieved from memory.
     */
    @AppFunction(isDescribedByKDoc = true)
    suspend fun searchMemories(
        context: AppFunctionContext,
        query: String,
        sourceId: String? = null
    ): List<String> {
        val result = apiService.search(queryText = query, sourceId = sourceId, hybrid = true, rerank = null)
        return result.results.map { it.text }
    }
}
