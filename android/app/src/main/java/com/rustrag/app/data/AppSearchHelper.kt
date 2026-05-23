package com.rustrag.app.data

import android.content.Context
import android.os.Build
import android.util.Log
import androidx.appsearch.app.AppSearchSession
import androidx.appsearch.app.PutDocumentsRequest
import androidx.appsearch.app.SetSchemaRequest
import androidx.appsearch.localstorage.LocalStorage
import androidx.appsearch.platformstorage.PlatformStorage
import com.google.common.util.concurrent.FutureCallback
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

class AppSearchHelper(private val context: Context) {

    private var session: AppSearchSession? = null

    private suspend fun getSession(): AppSearchSession = withContext(Dispatchers.IO) {
        session?.let { return@withContext it }

        val sessionFuture = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            PlatformStorage.createSearchSessionAsync(
                PlatformStorage.SearchContext.Builder(context, "rag_memories")
                    .build()
            )
        } else {
            LocalStorage.createSearchSessionAsync(
                LocalStorage.SearchContext.Builder(context, "rag_memories")
                    .build()
            )
        }
        val activeSession = sessionFuture.await()

        // Setup schema
        val setSchemaRequest = SetSchemaRequest.Builder()
            .addDocumentClasses(RagMemoryDocument::class.java)
            .setSchemaTypeDisplayedBySystem("RagMemoryDocument", true)
            .setForceOverride(true)
            .build()
        activeSession.setSchemaAsync(setSchemaRequest).await()
        Log.d("AppSearchHelper", "AppSearch schema configured successfully with global search visibility.")

        session = activeSession
        activeSession
    }

    suspend fun indexMemories(results: List<SearchResultPayload>): Unit = withContext(Dispatchers.IO) {
        if (results.isEmpty()) return@withContext
        try {
            val activeSession = getSession()
            val documents = results.map { payload ->
                RagMemoryDocument(
                    namespace = payload.sourceId.ifBlank { "default" },
                    id = payload.id,
                    text = payload.text,
                    path = payload.path
                )
            }
            val putRequest = PutDocumentsRequest.Builder()
                .addDocuments(documents)
                .build()
            activeSession.putAsync(putRequest).await()
            Log.d("AppSearchHelper", "Successfully indexed ${documents.size} memories to AppSearch.")
        } catch (e: Exception) {
            Log.e("AppSearchHelper", "Error indexing memories to AppSearch", e)
        }
    }
}

// Extension to await ListenableFuture in Kotlin Coroutines
private suspend fun <T> ListenableFuture<T>.await(): T = suspendCancellableCoroutine { cont ->
    Futures.addCallback(this, object : FutureCallback<T> {
        override fun onSuccess(result: T?) {
            @Suppress("UNCHECKED_CAST")
            if (result == null) {
                cont.resume(null as T)
            } else {
                cont.resume(result)
            }
        }

        override fun onFailure(t: Throwable) {
            cont.resumeWithException(t)
        }
    }, { run -> run.run() })

    cont.invokeOnCancellation {
        this.cancel(true)
    }
}
