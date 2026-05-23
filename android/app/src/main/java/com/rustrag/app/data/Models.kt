package com.rustrag.app.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

@Serializable
data class DeviceCodeRequest(
    @SerialName("client_name") val clientName: String? = null
)

@Serializable
data class DeviceCodeResponse(
    @SerialName("device_code") val deviceCode: String,
    @SerialName("user_code") val userCode: String,
    @SerialName("verification_uri") val verificationUri: String,
    @SerialName("verification_uri_complete") val verificationUriComplete: String,
    @SerialName("expires_in") val expiresIn: Long,
    @SerialName("interval") val interval: Long
)

@Serializable
data class DeviceTokenRequest(
    @SerialName("device_code") val deviceCode: String
)

@Serializable
data class DeviceTokenResponse(
    @SerialName("access_token") val accessToken: String,
    @SerialName("token_type") val tokenType: String,
    @SerialName("token_id") val tokenId: String,
    @SerialName("expires_at") val expiresAt: Long? = null
)

@Serializable
data class DeviceTokenErrorResponse(
    val error: String
)

@Serializable
data class SearchRequest(
    val query: String,
    @SerialName("top_k") val topK: Int = 5,
    @SerialName("source_id") val sourceId: String? = null,
    @SerialName("type") val typeName: String? = null,
    val hybrid: Boolean = true,
    val rerank: Boolean? = null,
    @SerialName("max_distance") val maxDistance: Float = 0.8f
)

@Serializable
data class SearchResultPayload(
    val id: String,
    val text: String,
    val metadata: JsonElement,
    @SerialName("source_id") val sourceId: String,
    @SerialName("created_at") val createdAt: Long,
    @SerialName("updated_at") val updatedAt: Long,
    val distance: Float,
    val path: String? = null,
    val analysis: StoreAnalysis? = null
)

@Serializable
data class SearchResponse(
    val results: List<SearchResultPayload>
)

@Serializable
data class StoreRequest(
    val id: String? = null,
    val text: String,
    val metadata: JsonElement,
    @SerialName("source_id") val sourceId: String,
    val path: String? = null,
    @SerialName("type") val typeName: String? = null,
    val data: JsonElement? = null
)

@Serializable
data class StoreResponse(
    val id: String,
    @SerialName("source_id") val sourceId: String,
    @SerialName("created_at") val createdAt: Long
)

@Serializable
data class StoreAnalysisVerdict(
    @SerialName("target_id") val targetId: String,
    val relation: String,
    val confidence: Float,
    val reason: String
)

@Serializable
data class StoreAnalysis(
    val verdicts: List<StoreAnalysisVerdict>? = null,
    val tags: List<String>? = null,
    val title: String? = null,
    val summary: String? = null,
    @SerialName("doc_type") val docType: String? = null
)

@Serializable
data class EntryNeighbor(
    val id: String,
    val title: String? = null,
    val relationship: String? = null,
    @SerialName("source_type") val sourceType: String? = null,
    val thumbnail: String? = null
)

@Serializable
data class AdminItemPayload(
    val id: String,
    val text: String,
    val metadata: JsonElement,
    @SerialName("source_id") val sourceId: String,
    @SerialName("created_at") val createdAt: Long,
    @SerialName("updated_at") val updatedAt: Long,
    val path: String? = null,
    @SerialName("type") val typeName: String? = null,
    val data: JsonElement? = null,
    val neighbors: List<EntryNeighbor>? = null,
    val analysis: StoreAnalysis? = null
)

@Serializable
data class AdminItemsResponse(
    val items: List<AdminItemPayload>,
    @SerialName("total_count") val totalCount: Long
)

@Serializable
data class UpdateItemRequest(
    val text: String,
    val metadata: JsonElement,
    @SerialName("source_id") val sourceId: String,
    val path: String? = null,
    @SerialName("type") val typeName: String? = null,
    val data: JsonElement? = null
)

