package com.rustrag.app.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException

class RagApiService(private val tokenManager: TokenManager) {
    private val client = OkHttpClient()
    private val json = Json { ignoreUnknownKeys = true }
    private val jsonMediaType = "application/json; charset=utf-8".toMediaType()

    private fun getBaseUrl(): String = tokenManager.serverUrl

    private fun addAuthHeader(builder: Request.Builder) {
        tokenManager.token?.let {
            builder.addHeader("Authorization", "Bearer $it")
        }
    }

    // 1. POST /auth/device/code
    suspend fun requestDeviceCode(clientName: String): DeviceCodeResponse = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/auth/device/code"
        val requestBody = json.encodeToString(DeviceCodeRequest(clientName))
            .toRequestBody(jsonMediaType)

        val request = Request.Builder()
            .url(url)
            .post(requestBody)
            .build()

        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Server returned error ${response.code}: ${response.message}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            json.decodeFromString<DeviceCodeResponse>(bodyStr)
        }
    }

    // 2. POST /auth/device/token
    // Returns Result with accessToken, or throws custom Exception for polling states
    suspend fun pollDeviceToken(deviceCode: String): PollResult = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/auth/device/token"
        val requestBody = json.encodeToString(DeviceTokenRequest(deviceCode))
            .toRequestBody(jsonMediaType)

        val request = Request.Builder()
            .url(url)
            .post(requestBody)
            .build()

        client.newCall(request).execute().use { response ->
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            if (response.code == 200) {
                val tokenResp = json.decodeFromString<DeviceTokenResponse>(bodyStr)
                PollResult.Success(tokenResp)
            } else if (response.code == 400) {
                try {
                    val errResp = json.decodeFromString<DeviceTokenErrorResponse>(bodyStr)
                    when (errResp.error) {
                        "authorization_pending" -> PollResult.Pending
                        "slow_down" -> PollResult.SlowDown
                        "access_denied" -> PollResult.AccessDenied
                        "expired_token" -> PollResult.Expired
                        else -> PollResult.Error(errResp.error)
                    }
                } catch (e: Exception) {
                    PollResult.Error("HTTP 400: $bodyStr")
                }
            } else {
                PollResult.Error("HTTP ${response.code}: $bodyStr")
            }
        }
    }

    // 3. POST /api/search
    suspend fun search(
        queryText: String,
        sourceId: String? = null,
        hybrid: Boolean = true,
        rerank: Boolean? = null,
        typeName: String? = null
    ): SearchResponse = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/api/search"
        val searchReq = SearchRequest(
            query = queryText,
            sourceId = sourceId?.takeIf { it.isNotBlank() },
            hybrid = hybrid,
            rerank = rerank,
            typeName = typeName?.takeIf { it.isNotBlank() }
        )
        val requestBody = json.encodeToString(searchReq).toRequestBody(jsonMediaType)

        val requestBuilder = Request.Builder()
            .url(url)
            .post(requestBody)
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Search failed with code ${response.code}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            json.decodeFromString<SearchResponse>(bodyStr)
        }
    }

    // 4. GET /admin/items/{id}
    suspend fun getEntryDetail(id: String): AdminItemPayload = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/admin/items/$id"

        val requestBuilder = Request.Builder()
            .url(url)
            .get()
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Failed to load item detail, code ${response.code}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            json.decodeFromString<AdminItemPayload>(bodyStr)
        }
    }

    // 5. POST /api/store
    suspend fun storeEntry(
        text: String,
        sourceId: String,
        path: String? = null,
        typeName: String? = null,
        data: JsonElement? = null
    ): StoreResponse = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/api/store"
        val storeReq = StoreRequest(
            text = text,
            sourceId = sourceId.trim().lowercase(),
            path = path?.trim()?.takeIf { it.isNotEmpty() },
            metadata = buildJsonObject {},
            typeName = typeName,
            data = data
        )
        val requestBody = json.encodeToString(storeReq).toRequestBody(jsonMediaType)

        val requestBuilder = Request.Builder()
            .url(url)
            .post(requestBody)
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Store failed with code ${response.code}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            json.decodeFromString<StoreResponse>(bodyStr)
        }
    }

    // 5b. PUT /admin/items/{id}
    suspend fun updateEntry(
        id: String,
        text: String,
        sourceId: String,
        path: String? = null,
        metadata: JsonElement = buildJsonObject {},
        typeName: String? = null,
        data: JsonElement? = null
    ): AdminItemPayload = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/admin/items/$id"
        val updateReq = UpdateItemRequest(
            text = text,
            metadata = metadata,
            sourceId = sourceId.trim().lowercase(),
            path = path?.trim()?.takeIf { it.isNotEmpty() },
            typeName = typeName,
            data = data
        )
        val requestBody = json.encodeToString(updateReq).toRequestBody(jsonMediaType)

        val requestBuilder = Request.Builder()
            .url(url)
            .put(requestBody)
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Update failed with code ${response.code}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            json.decodeFromString<AdminItemPayload>(bodyStr)
        }
    }

    // 6. GET /admin/items
    suspend fun getRecentEntries(limit: Int = 15, sourceId: String? = null, typeName: String? = null): List<SearchResultPayload> = withContext(Dispatchers.IO) {
        val urlBuilder = StringBuilder("${getBaseUrl()}/admin/items?limit=$limit")
        if (!sourceId.isNullOrBlank()) {
            urlBuilder.append("&source_id=$sourceId")
        }
        if (!typeName.isNullOrBlank()) {
            urlBuilder.append("&type=$typeName")
        }
        val requestBuilder = Request.Builder()
            .url(urlBuilder.toString())
            .get()
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Failed to load recent items, code ${response.code}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            val resp = json.decodeFromString<AdminItemsResponse>(bodyStr)
            resp.items.map { item ->
                SearchResultPayload(
                    id = item.id,
                    text = item.text,
                    metadata = item.metadata,
                    sourceId = item.sourceId,
                    createdAt = item.createdAt,
                    updatedAt = item.updatedAt,
                    distance = -1f, // -1f denotes a recent item without similarity score
                    path = item.path,
                    analysis = item.analysis,
                    typeName = item.typeName,
                    data = item.data
                )
            }
        }
    }

    // 7. GET /admin/categories
    suspend fun getCategories(): List<AdminCategoryPayload> = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/admin/categories"
        val requestBuilder = Request.Builder()
            .url(url)
            .get()
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Failed to load categories, code ${response.code}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            val resp = json.decodeFromString<CategoriesResponse>(bodyStr)
            resp.categories
        }
    }

    // 8. DELETE /admin/items/{id}
    suspend fun deleteEntry(id: String): Boolean = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/admin/items/$id"
        val requestBuilder = Request.Builder()
            .url(url)
            .delete()
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Delete failed with code ${response.code}")
            }
            true
        }
    }

    // 9. POST /admin/items/{id}/reanalyze
    suspend fun reanalyzeEntry(id: String): AdminItemPayload = withContext(Dispatchers.IO) {
        val url = "${getBaseUrl()}/admin/items/$id/reanalyze"
        val requestBuilder = Request.Builder()
            .url(url)
            .post("".toRequestBody(jsonMediaType))
        addAuthHeader(requestBuilder)

        client.newCall(requestBuilder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Reanalysis failed with code ${response.code}")
            }
            val bodyStr = response.body?.string() ?: throw IOException("Empty response body")
            json.decodeFromString<AdminItemPayload>(bodyStr)
        }
    }
}


sealed interface PollResult {
    data class Success(val tokenResponse: DeviceTokenResponse) : PollResult
    data object Pending : PollResult
    data object SlowDown : PollResult
    data object AccessDenied : PollResult
    data object Expired : PollResult
    data class Error(val message: String) : PollResult
}
