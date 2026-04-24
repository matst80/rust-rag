use crate::{
    config::{AuthConfig, ChunkingConfig, MultimodalConfig, OpenAiChatConfig},
    db::{
        AuthStore, CategorySummary, GraphEdgeRecord, GraphEdgeType, GraphNeighborhood,
        GraphNodeDistance, GraphStatus, ItemRecord, ListItemsRequest, ManualEdgeInput,
        NewUserEvent, SearchHit, SortOrder, UserEventType, UserMemoryStore, VectorStore,
        PROFILE_EVENTS_WINDOW, PROFILE_REFRESH_AFTER,
    },
    embedding::EmbeddingService,
};
use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use jsonwebtoken::{DecodingKey, Validation, decode};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};
use tower_http::{services::ServeDir, trace::TraceLayer};
use uuid::Uuid;

mod auth;
mod chunking;
mod multimodal;
mod openai;
mod query;

pub use auth::SessionSubject;

#[derive(Clone)]
pub struct AppState {
    pub embedder: Arc<EmbedderHandle>,
    pub store: Arc<dyn VectorStore>,
    pub auth_store: Arc<dyn AuthStore>,
    pub user_memory: Arc<dyn UserMemoryStore>,
    pub auth: Arc<AuthConfig>,
    pub openai_chat: Arc<OpenAiChatConfig>,
    pub multimodal: Arc<MultimodalConfig>,
    pub upload_path: Arc<String>,
    pub chunking: Arc<ChunkingConfig>,
    pub http_client: reqwest::Client,
    pub multimodal_client: reqwest::Client,
    pub(in crate::api) pending_tokens: Arc<auth::PendingTokenCache>,
}

impl AppState {
    pub fn new(
        embedder: Arc<EmbedderHandle>,
        store: Arc<dyn VectorStore>,
        auth_store: Arc<dyn AuthStore>,
        user_memory: Arc<dyn UserMemoryStore>,
        auth: AuthConfig,
        openai_chat: OpenAiChatConfig,
        multimodal: MultimodalConfig,
        upload_path: String,
        chunking: ChunkingConfig,
    ) -> Self {
        let timeout_secs = openai_chat.timeout_secs.max(1);
        let multimodal_timeout = multimodal.timeout_secs.max(1);
        Self {
            embedder,
            store,
            auth_store,
            user_memory,
            auth: Arc::new(auth),
            openai_chat: Arc::new(openai_chat),
            multimodal: Arc::new(multimodal),
            upload_path: Arc::new(upload_path),
            chunking: Arc::new(chunking),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .expect("http client should build"),
            multimodal_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(multimodal_timeout))
                .build()
                .expect("multimodal http client should build"),
            pending_tokens: Arc::new(auth::PendingTokenCache::default()),
        }
    }

    pub fn mcp_allowed_hosts(&self) -> Vec<String> {
        self.auth.mcp_allowed_hosts.clone()
    }

    #[cfg(test)]
    pub fn new_ready(
        embedder: Arc<dyn EmbeddingService>,
        store: Arc<dyn VectorStore>,
        auth_store: Arc<dyn AuthStore>,
    ) -> Self {
        let openai_chat = OpenAiChatConfig {
            timeout_secs: 60,
            ..OpenAiChatConfig::default()
        };
        Self {
            embedder: Arc::new(EmbedderHandle::ready(embedder)),
            store,
            auth_store,
            user_memory: Arc::new(NoopUserMemory),
            auth: Arc::new(AuthConfig::default()),
            openai_chat: Arc::new(openai_chat),
            multimodal: Arc::new(MultimodalConfig::default()),
            upload_path: Arc::new("uploads".to_owned()),
            chunking: Arc::new(ChunkingConfig::default()),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("http client should build"),
            multimodal_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("multimodal http client should build"),
            pending_tokens: Arc::new(auth::PendingTokenCache::default()),
        }
    }
}

pub struct EmbedderHandle {
    inner: RwLock<EmbedderState>,
}

enum EmbedderState {
    Loading,
    Ready(Arc<dyn EmbeddingService>),
    Failed(String),
}

impl EmbedderHandle {
    pub fn loading() -> Self {
        Self {
            inner: RwLock::new(EmbedderState::Loading),
        }
    }

    pub fn ready(embedder: Arc<dyn EmbeddingService>) -> Self {
        Self {
            inner: RwLock::new(EmbedderState::Ready(embedder)),
        }
    }

    pub fn mark_ready(&self, embedder: Arc<dyn EmbeddingService>) {
        *self.inner.write().expect("embedder state lock poisoned") = EmbedderState::Ready(embedder);
    }

    pub fn mark_failed(&self, error: String) {
        *self.inner.write().expect("embedder state lock poisoned") = EmbedderState::Failed(error);
    }

    pub(crate) fn get_ready(&self) -> Result<Arc<dyn EmbeddingService>, ApiError> {
        match &*self.inner.read().expect("embedder state lock poisoned") {
            EmbedderState::Loading => Err(ApiError::ServiceUnavailable(
                "embedder is still loading".to_owned(),
            )),
            EmbedderState::Ready(embedder) => Ok(embedder.clone()),
            EmbedderState::Failed(error) => Err(ApiError::ServiceUnavailable(format!(
                "embedder failed to initialize: {error}"
            ))),
        }
    }

    pub(crate) fn health(&self) -> (StatusCode, Json<HealthResponse>) {
        match &*self.inner.read().expect("embedder state lock poisoned") {
            EmbedderState::Loading => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "loading".to_owned(),
                    error: None,
                }),
            ),
            EmbedderState::Ready(_) => (
                StatusCode::OK,
                Json(HealthResponse {
                    status: "ready".to_owned(),
                    error: None,
                }),
            ),
            EmbedderState::Failed(error) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "failed".to_owned(),
                    error: Some(error.clone()),
                }),
            ),
        }
    }
}

pub fn metadata_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let serde_json::Value::Object(map) = serde_json::json!({
        "type": "object",
        "additionalProperties": true,
        "description": "Free-form JSON object of string-keyed metadata.",
    }) else {
        unreachable!()
    };
    schemars::Schema::from(map)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChunkConfig {
    /// Maximum characters per chunk (≈ max_chars/4 tokens). Default 1024 (~256 tokens).
    #[serde(default = "default_chunk_max_chars")]
    pub max_chars: usize,
    /// Characters of previous-chunk tail prepended to each chunk's embedding for
    /// contextual awareness. The stored text is NOT affected. Default 200 (~50 tokens).
    #[serde(default = "default_chunk_overlap_chars")]
    pub overlap_chars: usize,
}

fn default_chunk_max_chars() -> usize {
    1536
}
fn default_chunk_overlap_chars() -> usize {
    200
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StoreRequest {
    /// Optional stable identifier. If omitted, a UUIDv7 is generated.
    pub id: Option<String>,
    /// The natural-language content to embed and store.
    pub text: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    /// User-defined namespace/category for this entry (e.g. "memory", "knowledge", "notes").
    /// Entries sharing a source_id are grouped together; search and listing can filter on it.
    /// Pick a short, lowercase, stable identifier per logical bucket of content.
    pub source_id: String,
    /// If provided, the text is split into overlapping chunks before embedding.
    /// Each chunk is stored as a separate item keyed `{id}:c:{index}`.
    /// Omit for short texts or when you want the entry treated as a single unit.
    pub chunk: Option<ChunkConfig>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchRequest {
    /// Natural-language query. It is embedded and compared against stored entries.
    pub query: String,
    /// Maximum number of ranked hits to return.
    /// Optional; defaults to 5.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Restrict the search to entries with this source_id (namespace).
    /// Omit to search across every source_id.
    pub source_id: Option<String>,
    /// Optional toggle for hybrid search (Vector + Keyword). Defaults to true.
    #[serde(default = "default_hybrid")]
    pub hybrid: bool,
    /// Maximum distance threshold for results. Default 0.8.
    #[serde(default = "default_max_distance")]
    pub max_distance: f32,
}

fn default_hybrid() -> bool {
    true
}

fn default_top_k() -> usize {
    5
}

fn default_max_distance() -> f32 {
    0.8
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateItemRequest {
    /// New content to embed and store in place of the existing text.
    pub text: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    /// Namespace/category the entry belongs to. See StoreRequest.source_id.
    pub source_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListItemsQuery {
    /// Restrict the listing to a single source_id. Omit to list across all namespaces.
    pub source_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_order: Option<SortOrder>,
    pub min_created_at: Option<i64>,
    pub max_created_at: Option<i64>,
    /// Any other query parameters are treated as metadata filters (e.g. ?todo=mats)
    #[serde(flatten)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GraphNeighborhoodQuery {
    pub depth: Option<usize>,
    pub limit: Option<usize>,
    pub edge_type: Option<GraphEdgeType>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListGraphEdgesQuery {
    pub item_id: Option<String>,
    pub edge_type: Option<GraphEdgeType>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateManualEdgeRequest {
    pub from_item_id: String,
    pub to_item_id: String,
    pub relation: Option<String>,
    pub weight: Option<f32>,
    pub directed: Option<bool>,
    #[serde(default = "default_metadata")]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StoreResponse {
    pub id: String,
    pub source_id: String,
    pub created_at: i64,
    /// IDs of stored chunks, present only when the request included a `chunk` config
    /// and the text was long enough to require splitting (≥ 2 chunks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteResponse {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SearchResultPayload {
    pub id: String,
    pub text: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    pub source_id: String,
    pub created_at: i64,
    pub distance: f32,
    /// Stitched context window for chunk results: the matched chunk surrounded by
    /// its immediate neighbours. Null for non-chunk entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_context: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct RelatedResultPayload {
    pub id: String,
    pub text: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    pub source_id: String,
    pub created_at: i64,
    pub distance: f32,
    pub relation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SearchResponse {
    pub results: Vec<SearchResultPayload>,
    #[serde(default)]
    pub related: Vec<RelatedResultPayload>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct AdminCategoryPayload {
    pub source_id: String,
    pub item_count: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CategoriesResponse {
    pub categories: Vec<AdminCategoryPayload>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct AdminItemPayload {
    pub id: String,
    pub text: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    pub source_id: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct AdminItemsResponse {
    pub items: Vec<AdminItemPayload>,
    pub total_count: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GraphStatusResponse {
    pub enabled: bool,
    pub build_on_startup: bool,
    pub similarity_top_k: usize,
    pub similarity_max_distance: f32,
    pub cross_source: bool,
    pub item_count: i64,
    pub edge_count: i64,
    pub similarity_edge_count: i64,
    pub manual_edge_count: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GraphEdgePayload {
    pub id: String,
    pub from_item_id: String,
    pub to_item_id: String,
    pub edge_type: GraphEdgeType,
    pub relation: Option<String>,
    pub weight: f32,
    pub directed: bool,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GraphEdgesResponse {
    pub edges: Vec<GraphEdgePayload>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GraphNeighborhoodResponse {
    pub center_id: String,
    pub nodes: Vec<AdminItemPayload>,
    pub edges: Vec<GraphEdgePayload>,
    pub pairwise_distances: Vec<GraphNodeDistancePayload>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GraphNodeDistancePayload {
    pub from_item_id: String,
    pub to_item_id: String,
    pub distance: f32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GraphRebuildResponse {
    pub rebuilt_edges: usize,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct HealthResponse {
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionResponse {
    pub authenticated: bool,
    pub auth_enabled: bool,
    pub user: Option<SessionUser>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionUser {
    pub name: Option<String>,
    pub email: Option<String>,
    pub preferred_username: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    name: Option<String>,
    email: Option<String>,
    preferred_username: Option<String>,
    exp: usize,
}

#[allow(dead_code)]
pub(crate) struct NoopUserMemory;

impl UserMemoryStore for NoopUserMemory {
    fn log_user_event(&self, _: NewUserEvent) -> anyhow::Result<()> {
        Ok(())
    }
    fn touch_item_accesses(&self, _: &[String], _: i64) -> anyhow::Result<()> {
        Ok(())
    }
    fn get_user_profile(&self, _: &str) -> anyhow::Result<Option<crate::db::UserProfile>> {
        Ok(None)
    }
    fn upsert_user_profile(&self, _: crate::db::UserProfile) -> anyhow::Result<()> {
        Ok(())
    }
    fn get_recent_query_embeddings(&self, _: &str, _: usize) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
    fn count_events_since(&self, _: &str, _: i64) -> anyhow::Result<i64> {
        Ok(0)
    }
}

pub fn router(state: AppState) -> Router {
    let protected_routes = Router::new()
        .route("/store", post(store))
        .route("/api/store", post(store))
        .route("/api/store/smart", post(smart_store))
        .route("/search", post(search))
        .route("/api/search", post(search))
        .route(
            "/api/openai/v1/chat/completions",
            post(openai::chat_completions),
        )
        .route("/api/query/assisted", post(query::assisted_query))
        .route("/graph/status", get(graph_status))
        .route("/api/graph/status", get(graph_status))
        .route("/graph/edges", get(list_graph_edges))
        .route("/api/graph/edges", get(list_graph_edges))
        .route("/graph/neighborhood/{id}", get(graph_neighborhood))
        .route("/api/graph/neighborhood/{id}", get(graph_neighborhood))
        .route("/admin/categories", get(list_categories))
        .route("/admin/items", get(list_items))
        .route("/admin/items/oversized", get(list_large_items))
        .route(
            "/admin/items/{id}",
            get(get_item).put(update_item).delete(delete_item),
        )
        .route("/admin/items/{id}/rechunk", post(rechunk_item))
        .route("/admin/items/{id}/llm-rechunk", post(llm_rechunk_item))
        .route("/admin/graph/rebuild", post(rebuild_graph))
        .route("/admin/graph/edges", post(create_manual_edge))
        .route("/admin/graph/edges/{id}", delete(delete_graph_edge))
        .route("/api/ingest/image", post(multimodal::ingest_image))
        .route_service("/mcp", crate::mcp::streamable_http_service(state.clone()))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ))
        .with_state(state.clone());

    let upload_path = state.upload_path.as_str().to_owned();
    Router::new()
        .route("/healthz", get(health))
        .nest_service("/assets", ServeDir::new(&upload_path))
        .merge(auth::public_routes())
        .merge(auth::session_routes(state.clone()))
        .merge(protected_routes)
        .fallback_service(ServeDir::new("static-frontend/dist"))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn require_api_key(
    State(state): State<AppState>,
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        "checking api key for request"
    );
    if !state.auth.is_enabled() {
        request.extensions_mut().insert(SessionSubject(None));
        return Ok(next.run(request).await);
    }

    let provided = request
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            request
                .headers()
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    if let Some(ref key) = provided {
        if state.auth.matches_api_key(key) {
            request.extensions_mut().insert(SessionSubject(None));
            return Ok(next.run(request).await);
        }

        if key.starts_with(auth::MCP_TOKEN_PREFIX) {
            let hash = auth::hash_token(key);
            let auth_store = state.auth_store.clone();
            let record =
                tokio::task::spawn_blocking(move || auth_store.find_mcp_token_by_hash(&hash))
                    .await
                    .map_err(ApiError::TaskJoin)?
                    .map_err(ApiError::Internal)?;
            if let Some(record) = record {
                let now = current_timestamp_millis()?;
                if record
                    .expires_at
                    .map(|expiry| expiry <= now)
                    .unwrap_or(false)
                {
                    tracing::warn!(token_id = %record.id, "rejecting expired MCP token");
                } else {
                    let touch_store = state.auth_store.clone();
                    let touch_id = record.id.clone();
                    tokio::task::spawn_blocking(move || {
                        if let Err(error) = touch_store.touch_mcp_token(&touch_id, now) {
                            tracing::warn!(error = %error, "failed to update token last_used_at");
                        }
                    });
                    tracing::debug!(token_id = %record.id, "authorized via MCP token");
                    request
                        .extensions_mut()
                        .insert(SessionSubject(record.subject));
                    return Ok(next.run(request).await);
                }
            }
        }
    }

    if let Some(secret) = state.auth.session_secret.as_deref() {
        if let Some(cookies) = request
            .headers()
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
        {
            for cookie in cookies.split(';') {
                let mut parts = cookie.trim().splitn(2, '=');
                if let (Some(name), Some(value)) = (parts.next(), parts.next()) {
                    if name == "rag_session" {
                        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
                        validation.validate_aud = false; // Token has no aud claim

                        match decode::<Claims>(
                            value,
                            &DecodingKey::from_secret(secret.as_bytes()),
                            &validation,
                        ) {
                            Ok(token_data) => {
                                tracing::info!("authorized via session cookie");
                                request
                                    .extensions_mut()
                                    .insert(SessionSubject(Some(token_data.claims.sub)));
                                return Ok(next.run(request).await);
                            }
                            Err(err) => {
                                tracing::warn!(error = %err, "failed to decode session cookie");
                            }
                        }
                    }
                }
            }
        } else {
            tracing::info!("no cookie header found in request");
        }
    } else {
        tracing::warn!(
            "AUTH_SESSION_SECRET not configured in backend - session cookies will be ignored"
        );
    }

    tracing::warn!(
        has_x_api_key = provided.is_some(),
        has_cookies = request.headers().contains_key(axum::http::header::COOKIE),
        "unauthorized request: no valid credential found"
    );

    Err(ApiError::Unauthorized(
        "missing x-api-key header, bearer token or valid session cookie".to_owned(),
    ))
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    state.embedder.health()
}

async fn store(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    Json(request): Json<StoreRequest>,
) -> Result<(StatusCode, Json<StoreResponse>), ApiError> {
    let response = store_entry_core(&state, request, session.0).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn search(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    search_core(&state, request, session.0).await.map(Json)
}

pub(crate) async fn store_entry_core(
    state: &AppState,
    request: StoreRequest,
    subject: Option<String>,
) -> Result<StoreResponse, ApiError> {
    let id = resolve_store_id(request.id);
    validate_non_empty("text", &request.text)?;
    validate_metadata(&request.metadata)?;
    validate_source_id(&request.source_id)?;

    let embedder = state.embedder.get_ready()?;
    let store = state.store.clone();
    let created_at = current_timestamp_millis()?;
    let source_id = request.source_id.clone();

    let chunk_ids = if let Some(ref cfg) = request.chunk {
        // Build chunk slices (returns 1 slice if text fits in one chunk).
        let slices =
            chunking::chunk_document(&request.text, cfg.max_chars, cfg.overlap_chars);
        let n = slices.len();

        if n <= 1 {
            // Text fits in a single chunk — store as a normal entry, no chunking metadata.
            let item = ItemRecord {
                id: id.clone(),
                text: request.text.clone(),
                metadata: request.metadata.clone(),
                source_id: source_id.clone(),
                created_at,
            };
            let embed_text = slices.into_iter().next().map(|s| s.embed_text).unwrap_or_else(|| request.text.clone());
            tokio::task::spawn_blocking(move || -> Result<()> {
                let embedding = embedder.embed(&embed_text)?;
                store.upsert_item(item, &embedding)?;
                Ok(())
            })
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
            None
        } else {
            let parent_id = id.clone();
            let mut ids: Vec<String> = Vec::with_capacity(n);

            for (i, slice) in slices.into_iter().enumerate() {
                let chunk_id = format!("{parent_id}:c:{i}");
                ids.push(chunk_id.clone());

                let mut meta = request.metadata.clone();
                if let Value::Object(ref mut map) = meta {
                    map.insert(
                        "_chunk".to_owned(),
                        serde_json::json!({ "parent": parent_id, "i": i, "n": n }),
                    );
                }

                let item = ItemRecord {
                    id: chunk_id,
                    text: slice.text,
                    metadata: meta,
                    source_id: source_id.clone(),
                    created_at,
                };
                let embed_text = slice.embed_text;
                let emb = embedder.clone();
                let st = store.clone();
                tokio::task::spawn_blocking(move || -> Result<()> {
                    let embedding = emb.embed(&embed_text)?;
                    st.upsert_item(item, &embedding)?;
                    Ok(())
                })
                .await
                .map_err(ApiError::TaskJoin)?
                .map_err(ApiError::Internal)?;
            }
            Some(ids)
        }
    } else {
        // No chunking requested.
        let item = ItemRecord {
            id: id.clone(),
            text: request.text.clone(),
            metadata: request.metadata,
            source_id: source_id.clone(),
            created_at,
        };
        tokio::task::spawn_blocking(move || -> Result<()> {
            let embedding = embedder.embed(&item.text)?;
            store.upsert_item(item, &embedding)?;
            Ok(())
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
        None
    };

    let stored_ids: Vec<String> = chunk_ids
        .as_deref()
        .unwrap_or(&[id.clone()])
        .to_vec();

    if let Some(sub) = subject {
        let memory = state.user_memory.clone();
        let event_id = Uuid::now_v7().to_string();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = memory.log_user_event(NewUserEvent {
                id: event_id,
                subject: sub,
                event_type: UserEventType::Store,
                query: None,
                query_embedding: None,
                item_ids: stored_ids,
                created_at,
            }) {
                tracing::warn!(error = %e, "failed to log store event");
            }
        });
    }

    Ok(StoreResponse {
        id,
        source_id,
        created_at,
        chunk_ids,
    })
}

pub(crate) async fn search_core(
    state: &AppState,
    request: SearchRequest,
    subject: Option<String>,
) -> Result<SearchResponse, ApiError> {
    if request.top_k == 0 {
        return Err(ApiError::BadRequest(
            "top_k must be greater than zero".to_owned(),
        ));
    }
    if let Some(source_id) = request.source_id.as_deref() {
        validate_source_id(source_id)?;
    }

    // Load user interest profile for personalization (best-effort).
    let interest_embedding: Option<Vec<f32>> = if let Some(ref sub) = subject {
        let memory = state.user_memory.clone();
        let sub = sub.clone();
        tokio::task::spawn_blocking(move || {
            memory
                .get_user_profile(&sub)
                .ok()
                .flatten()
                .and_then(|p| p.interest_embedding)
        })
        .await
        .unwrap_or(None)
    } else {
        None
    };

    let embedder = state.embedder.get_ready()?;
    let store = state.store.clone();
    let query = request.query.clone();
    let top_k = request.top_k;
    let source_id = request.source_id;
    let max_distance = request.max_distance;
    let now_ms = current_timestamp_millis()?;

    let (results, related, raw_embedding) = tokio::task::spawn_blocking(
        move || -> Result<(Vec<SearchHit>, Vec<(SearchHit, Option<String>)>, Vec<f32>)> {
            let raw = embedder.embed(&query)?;
            let embedding = if let Some(ref interest) = interest_embedding {
                blend_embeddings(&raw, interest, 0.8)
            } else {
                raw.clone()
            };
            let hits = if request.hybrid {
                store.search_hybrid(&query, &embedding, top_k, source_id.as_deref())?
            } else {
                store.search(&embedding, top_k, source_id.as_deref())?
            };
            let mut filtered: Vec<SearchHit> = hits
                .into_iter()
                .filter(|hit| hit.distance <= max_distance)
                .collect();

            // Sort by decay-adjusted score but keep the raw semantic distance for
            // the response so the reported values remain pure vector/BM25 distances.
            filtered.sort_by(|a, b| {
                decay_sort_key(a, now_ms)
                    .partial_cmp(&decay_sort_key(b, now_ms))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let related = if let Some(top) = filtered.first() {
                let edges = store
                    .list_graph_edges(Some(&top.id), Some(GraphEdgeType::Manual))
                    .ok()
                    .unwrap_or_default();
                let existing: HashSet<&str> = filtered.iter().map(|hit| hit.id.as_str()).collect();
                let mut relations: HashMap<String, Option<String>> = HashMap::new();
                for edge in edges {
                    let neighbor_id = if edge.from_item_id == top.id {
                        edge.to_item_id
                    } else {
                        edge.from_item_id
                    };
                    if neighbor_id == top.id || existing.contains(neighbor_id.as_str()) {
                        continue;
                    }
                    relations.entry(neighbor_id).or_insert(edge.relation);
                }
                if relations.is_empty() {
                    Vec::new()
                } else {
                    let ids: Vec<String> = relations.keys().cloned().collect();
                    let mut hits = store.distances_for_ids(&embedding, &ids)?;
                    hits.sort_by(|a, b| {
                        a.distance
                            .partial_cmp(&b.distance)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    hits.into_iter()
                        .map(|hit| {
                            let relation = relations.get(&hit.id).and_then(Clone::clone);
                            (hit, relation)
                        })
                        .collect()
                }
            } else {
                Vec::new()
            };

            Ok((filtered, related, raw))
        },
    )
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    // Fire-and-forget: log the search event and update access counts.
    if let Some(sub) = subject {
        let memory = state.user_memory.clone();
        let hit_ids: Vec<String> = results.iter().map(|h| h.id.clone()).collect();
        let event_id = Uuid::now_v7().to_string();
        let query_text = request.query.clone();
        let profile_memory = memory.clone();
        let sub_clone = sub.clone();
        tokio::task::spawn_blocking(move || {
            let event = NewUserEvent {
                id: event_id,
                subject: sub_clone.clone(),
                event_type: UserEventType::Search,
                query: Some(query_text),
                query_embedding: Some(raw_embedding),
                item_ids: hit_ids.clone(),
                created_at: now_ms,
            };
            if let Err(e) = memory.log_user_event(event) {
                tracing::warn!(error = %e, "failed to log search event");
            }
            if let Err(e) = memory.touch_item_accesses(&hit_ids, now_ms) {
                tracing::warn!(error = %e, "failed to update item access counts");
            }
            // Refresh interest profile if enough new events have accumulated.
            maybe_refresh_profile(&*profile_memory, &sub_clone, now_ms);
        });
    }

    // Enrich chunk results with adjacent-chunk context (best-effort).
    let store_for_chunks = state.store.clone();
    let result_payloads: Vec<SearchResultPayload> = tokio::task::spawn_blocking(move || {
        results
            .into_iter()
            .map(|hit| {
                let ctx = chunk_context_for_hit(&*store_for_chunks, &hit);
                SearchResultPayload {
                    chunk_context: ctx,
                    ..SearchResultPayload::from(hit)
                }
            })
            .collect()
    })
    .await
    .map_err(ApiError::TaskJoin)?;

    Ok(SearchResponse {
        results: result_payloads,
        related: related
            .into_iter()
            .map(|(hit, relation)| RelatedResultPayload {
                id: hit.id,
                text: hit.text,
                metadata: hit.metadata,
                source_id: hit.source_id,
                created_at: hit.created_at,
                distance: hit.distance,
                relation,
            })
            .collect(),
    })
}

/// If `hit` is a chunk (metadata contains `_chunk.parent`), fetch the previous
/// and next sibling chunks and stitch them together as a context window.
/// Returns `None` for non-chunk entries or when neighbours can't be fetched.
fn chunk_context_for_hit(store: &dyn VectorStore, hit: &SearchHit) -> Option<String> {
    let chunk_meta = hit.metadata.get("_chunk")?;
    let parent = chunk_meta.get("parent")?.as_str()?;
    let index = chunk_meta.get("i")?.as_u64()? as usize;
    let total = chunk_meta.get("n")?.as_u64()? as usize;

    let prev_text = if index > 0 {
        store
            .get_item(&format!("{parent}:c:{}", index - 1))
            .ok()
            .flatten()
            .map(|r| r.text)
    } else {
        None
    };
    let next_text = if index + 1 < total {
        store
            .get_item(&format!("{parent}:c:{}", index + 1))
            .ok()
            .flatten()
            .map(|r| r.text)
    } else {
        None
    };

    // Only build context if at least one neighbour exists.
    if prev_text.is_none() && next_text.is_none() {
        return None;
    }

    let mut parts: Vec<&str> = Vec::with_capacity(3);
    if let Some(ref p) = prev_text {
        parts.push(p.as_str());
    }
    parts.push(hit.text.as_str());
    if let Some(ref n) = next_text {
        parts.push(n.as_str());
    }
    Some(parts.join("\n\n"))
}

/// Returns a sort key that adds a mild time-decay penalty to the semantic
/// distance. The raw `hit.distance` is NOT mutated — callers report the
/// original value, decay only affects ordering.
fn decay_sort_key(hit: &SearchHit, now_ms: i64) -> f32 {
    if hit.created_at <= 0 {
        return hit.distance;
    }
    let age_days = ((now_ms - hit.created_at).max(0) as f64 / 86_400_000.0).min(730.0);
    // Penalty proportional to the item's own distance: highly-relevant items
    // are barely affected; borderline hits are nudged down for freshness.
    let penalty = 0.0002 * age_days as f32 * hit.distance.max(0.01);
    (hit.distance + penalty).clamp(0.0, 1.0)
}

/// Blend two embeddings: `weight * a + (1-weight) * b`, then L2-normalize.
fn blend_embeddings(a: &[f32], b: &[f32], weight: f32) -> Vec<f32> {
    let mut blended: Vec<f32> = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| weight * x + (1.0 - weight) * y)
        .collect();
    let norm: f32 = blended.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for v in &mut blended {
            *v /= norm;
        }
    }
    blended
}

/// Recompute the user's interest embedding from recent query embeddings when
/// enough new events have occurred since the last profile update.
fn maybe_refresh_profile(memory: &dyn UserMemoryStore, subject: &str, now_ms: i64) {
    let profile = match memory.get_user_profile(subject) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load user profile for refresh");
            return;
        }
    };
    let horizon = profile.as_ref().map(|p| p.event_horizon).unwrap_or(0);
    let new_events = match memory.count_events_since(subject, horizon) {
        Ok(n) => n,
        Err(_) => return,
    };
    if new_events < PROFILE_REFRESH_AFTER {
        return;
    }
    let embeddings = match memory.get_recent_query_embeddings(subject, PROFILE_EVENTS_WINDOW) {
        Ok(e) if !e.is_empty() => e,
        _ => return,
    };
    let dim = embeddings[0].len();
    let mut centroid = vec![0.0f32; dim];
    for emb in &embeddings {
        for (c, v) in centroid.iter_mut().zip(emb) {
            *c += v;
        }
    }
    let n = embeddings.len() as f32;
    for c in &mut centroid {
        *c /= n;
    }
    let norm: f32 = centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for c in &mut centroid {
            *c /= norm;
        }
    }
    let updated = crate::db::UserProfile {
        subject: subject.to_owned(),
        interest_embedding: Some(centroid),
        event_horizon: now_ms,
        updated_at: now_ms,
    };
    if let Err(e) = memory.upsert_user_profile(updated) {
        tracing::warn!(error = %e, "failed to save user profile");
    }
}

async fn graph_status(
    State(state): State<AppState>,
) -> Result<Json<GraphStatusResponse>, ApiError> {
    let store = state.store.clone();
    let status = tokio::task::spawn_blocking(move || store.graph_status())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(status.into()))
}

async fn list_graph_edges(
    State(state): State<AppState>,
    Query(query): Query<ListGraphEdgesQuery>,
) -> Result<Json<GraphEdgesResponse>, ApiError> {
    let store = state.store.clone();
    let item_id = query.item_id;
    let edge_type = query.edge_type;

    let edges =
        tokio::task::spawn_blocking(move || store.list_graph_edges(item_id.as_deref(), edge_type))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(map_graph_error)?;

    Ok(Json(GraphEdgesResponse {
        edges: edges.into_iter().map(Into::into).collect(),
    }))
}

async fn graph_neighborhood(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<GraphNeighborhoodQuery>,
) -> Result<Json<GraphNeighborhoodResponse>, ApiError> {
    let depth = query.depth.unwrap_or(1);
    let limit = query.limit.unwrap_or(100);
    validate_graph_depth(depth)?;
    validate_graph_limit(limit)?;

    let store = state.store.clone();
    let edge_type = query.edge_type;
    let neighborhood =
        tokio::task::spawn_blocking(move || store.graph_neighborhood(&id, depth, limit, edge_type))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(map_graph_error)?;

    Ok(Json(neighborhood.into()))
}

async fn list_categories(
    State(state): State<AppState>,
) -> Result<Json<CategoriesResponse>, ApiError> {
    let store = state.store.clone();
    let categories = tokio::task::spawn_blocking(move || store.list_categories())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(CategoriesResponse {
        categories: categories.into_iter().map(Into::into).collect(),
    }))
}

async fn list_items(
    State(state): State<AppState>,
    Query(query): Query<ListItemsQuery>,
) -> Result<Json<AdminItemsResponse>, ApiError> {
    if let Some(source_id) = query.source_id.as_deref() {
        validate_source_id(source_id)?;
    }

    let store = state.store.clone();
    let request = ListItemsRequest {
        source_id: query.source_id,
        limit: query.limit,
        offset: query.offset,
        sort_order: query.sort_order.unwrap_or(SortOrder::Desc),
        metadata_filter: query.metadata,
        min_created_at: query.min_created_at,
        max_created_at: query.max_created_at,
    };

    let (items, total_count) = tokio::task::spawn_blocking(move || store.list_items(request))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(AdminItemsResponse {
        items: items.into_iter().map(Into::into).collect(),
        total_count,
    }))
}

async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AdminItemPayload>, ApiError> {
    let store = state.store.clone();
    let item = tokio::task::spawn_blocking(move || store.get_item(&id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound("item not found".to_owned()))?;

    Ok(Json(item.into()))
}

async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateItemRequest>,
) -> Result<Json<AdminItemPayload>, ApiError> {
    validate_metadata(&request.metadata)?;
    validate_source_id(&request.source_id)?;

    let embedder = state.embedder.get_ready()?;
    let store = state.store.clone();

    let updated = tokio::task::spawn_blocking(move || -> Result<ItemRecord> {
        let existing = store
            .get_item(&id)?
            .ok_or_else(|| anyhow::anyhow!("item {id} not found"))?;
        let item = ItemRecord {
            id: existing.id,
            text: request.text,
            metadata: request.metadata,
            source_id: request.source_id,
            created_at: existing.created_at,
        };
        let embedding = embedder.embed(&item.text)?;
        store.upsert_item(item.clone(), &embedding)?;
        Ok(item)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(|error| map_missing_item("item", error))?;

    Ok(Json(updated.into()))
}

async fn delete_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, ApiError> {
    let store = state.store.clone();
    let deleted = tokio::task::spawn_blocking({
        let id = id.clone();
        move || store.delete_item(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    if !deleted {
        return Err(ApiError::NotFound(format!("item {id} not found")));
    }

    Ok(Json(DeleteResponse { id, deleted }))
}

#[derive(Debug, Deserialize)]
struct LargeItemsQuery {
    min_chars: Option<usize>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_large_items(
    State(state): State<AppState>,
    Query(query): Query<LargeItemsQuery>,
) -> Result<Json<AdminItemsResponse>, ApiError> {
    let min_chars = query.min_chars.unwrap_or(state.chunking.large_item_threshold);
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    let store = state.store.clone();
    let (items, total_count) =
        tokio::task::spawn_blocking(move || store.list_large_items(min_chars, limit, offset))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
    Ok(Json(AdminItemsResponse {
        items: items.into_iter().map(Into::into).collect(),
        total_count,
    }))
}

#[derive(Debug, Deserialize)]
struct RechunkRequest {
    max_chars: Option<usize>,
    overlap_chars: Option<usize>,
}

async fn rechunk_item(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Path(id): Path<String>,
    Json(body): Json<RechunkRequest>,
) -> Result<Json<StoreResponse>, ApiError> {
    state.embedder.get_ready()?;
    let store = state.store.clone();

    let item = tokio::task::spawn_blocking({
        let id = id.clone();
        move || store.get_item(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?
    .ok_or_else(|| ApiError::NotFound(format!("item {id} not found")))?;

    let max_chars = body.max_chars.unwrap_or(state.chunking.chunk_max_chars);
    let overlap_chars = body.overlap_chars.unwrap_or(state.chunking.chunk_overlap_chars);

    let request = StoreRequest {
        id: Some(item.id.clone()),
        text: item.text,
        metadata: item.metadata,
        source_id: item.source_id,
        chunk: Some(ChunkConfig { max_chars, overlap_chars }),
    };
    let response = store_entry_core(&state, request, subject.0).await?;

    // Delete the original unchunked item if chunking produced multiple chunks.
    if response.chunk_ids.as_ref().is_some_and(|ids| !ids.is_empty()) {
        let store = state.store.clone();
        let parent_id = id.clone();
        tokio::task::spawn_blocking(move || store.delete_item(&parent_id))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
    }

    Ok(Json(response))
}

const LLM_RECHUNK_SYSTEM_PROMPT: &str = "\
You are a document chunking assistant. Your task is to split the provided text into \
self-contained, semantically coherent chunks suitable for retrieval-augmented generation (RAG). \
\n\n\
Rules:\n\
- Each chunk should be independently understandable — no dangling references.\n\
- Preserve meaning; do not summarize or paraphrase.\n\
- Use natural boundaries (paragraphs, sections, list items).\n\
- Aim for chunks of roughly 200–600 words.\n\
- Avoid chunks shorter than 2 sentences unless the section is naturally brief.\n\
\n\n\
Respond ONLY with a JSON array of strings, each string being one chunk. No markdown, no explanation.\n\
Example: [\"chunk one text\", \"chunk two text\"]";

#[derive(Debug, Deserialize)]
struct LlmRechunkRequest {
    model: Option<String>,
    max_chunks: Option<usize>,
}

async fn llm_rechunk_item(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Path(id): Path<String>,
    Json(body): Json<LlmRechunkRequest>,
) -> Result<Json<StoreResponse>, ApiError> {
    let openai_config = state.openai_chat.clone();
    if !openai_config.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "upstream OpenAI chat configuration is not set".to_owned(),
        ));
    }
    let model = body
        .model
        .or_else(|| openai_config.default_model.clone())
        .ok_or_else(|| ApiError::BadRequest("model required".to_owned()))?;

    let store = state.store.clone();
    let item = tokio::task::spawn_blocking({
        let id = id.clone();
        move || store.get_item(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?
    .ok_or_else(|| ApiError::NotFound(format!("item {id} not found")))?;

    let max_chunks = body.max_chunks.unwrap_or(20);
    let user_prompt = format!(
        "Split this text into at most {max_chunks} chunks.\n\nTEXT:\n{}",
        item.text
    );

    let payload = serde_json::json!({
        "model": model,
        "temperature": 0,
        "messages": [
            {"role": "system", "content": LLM_RECHUNK_SYSTEM_PROMPT},
            {"role": "user", "content": user_prompt}
        ]
    });

    let base_url = openai_config
        .base_url
        .as_deref()
        .expect("is_configured() already checked");

    let mut req = state
        .http_client
        .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
        .json(&payload);
    if let Some(key) = openai_config.api_key.as_deref() {
        req = req.bearer_auth(key);
    }

    let response = req
        .send()
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .error_for_status()
        .map_err(|e| ApiError::Internal(e.into()))?;

    #[derive(serde::Deserialize)]
    struct Resp { choices: Vec<Choice> }
    #[derive(serde::Deserialize)]
    struct Choice { message: Msg }
    #[derive(serde::Deserialize)]
    struct Msg { content: Option<String> }

    let chat: Resp = response
        .json()
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let content = chat
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();

    let texts: Vec<String> = serde_json::from_str(&content).map_err(|_| {
        ApiError::Internal(anyhow::anyhow!(
            "LLM did not return a JSON array of strings; got: {content}"
        ))
    })?;

    if texts.is_empty() {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "LLM returned an empty chunk list"
        )));
    }

    let embedder = state.embedder.get_ready()?;
    let parent_id = item.id.clone();
    let source_id = item.source_id.clone();
    let base_metadata = item.metadata.clone();
    let now = current_timestamp_millis()?;
    let store = state.store.clone();
    let total = texts.len();

    let chunk_ids: Vec<String> = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<String>> {
        let mut ids = Vec::with_capacity(total);
        for (i, text) in texts.into_iter().enumerate() {
            let chunk_id = format!("{parent_id}:c:{i}");
            let mut metadata = base_metadata.clone();
            if let Some(obj) = metadata.as_object_mut() {
                obj.insert(
                    "_chunk".to_owned(),
                    serde_json::json!({ "parent": parent_id, "i": i, "n": total }),
                );
            }
            let record = ItemRecord {
                id: chunk_id.clone(),
                text: text.clone(),
                metadata,
                source_id: source_id.clone(),
                created_at: now,
            };
            let embedding = embedder.embed(&text)?;
            store.upsert_item(record, &embedding)?;
            ids.push(chunk_id);
        }
        store.delete_item(&parent_id)?;
        Ok(ids)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    if let Some(ref sub) = subject.0 {
        let memory = state.user_memory.clone();
        let event = NewUserEvent {
            id: Uuid::new_v4().to_string(),
            subject: sub.clone(),
            event_type: UserEventType::Store,
            query: None,
            query_embedding: None,
            item_ids: chunk_ids.clone(),
            created_at: now,
        };
        let _ = tokio::task::spawn_blocking(move || memory.log_user_event(event)).await;
    }

    Ok(Json(StoreResponse {
        id: item.id,
        source_id: item.source_id,
        created_at: now,
        chunk_ids: Some(chunk_ids),
    }))
}

const LLM_SMART_STORE_SYSTEM_PROMPT: &str = "\
You are a knowledge extraction assistant for a personal RAG (retrieval-augmented generation) system. \
Analyze the provided text and extract structured, searchable knowledge items.\n\n\
For each logical piece of information:\n\
1. Write a concise, self-contained text chunk — preserve important details, remove fluff\n\
2. Assign a source_id category. Use one of: knowledge, reference, notes, code, recipe, \
   medical, finance, travel, or a short lowercase identifier that best fits\n\
3. Extract metadata: title (short descriptive title), topic (main subject), \
   tags (array of relevant keywords)\n\n\
Rules:\n\
- Split into MULTIPLE items when the text covers distinct topics or has natural section breaks\n\
- Each item must be independently useful — no dangling references to other chunks\n\
- Aim for 1-4 sentences per item; longer only when the information is tightly coupled\n\
- Preserve technical accuracy; never fabricate or add information not present\n\
- If a URL or author is present in the context, include it in metadata\n\n\
Respond ONLY with a JSON array. No markdown fences, no explanation.\n\
Schema: [{\"text\": \"...\", \"source_id\": \"...\", \"metadata\": {\"title\": \"...\", \"topic\": \"...\", \"tags\": [\"...\"]}}]";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SmartStoreContext {
    pub url: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SmartStoreRequest {
    pub text: String,
    pub context: Option<SmartStoreContext>,
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SmartStoreResponse {
    pub items: Vec<StoreResponse>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SmartStoreItem {
    pub text: String,
    pub source_id: String,
    pub metadata: Value,
}

async fn smart_store(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    Json(body): Json<SmartStoreRequest>,
) -> Result<(StatusCode, Json<SmartStoreResponse>), ApiError> {
    let openai_config = state.openai_chat.clone();
    if !openai_config.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "upstream OpenAI chat configuration is not set".to_owned(),
        ));
    }
    let model = body
        .model
        .or_else(|| openai_config.default_model.clone())
        .ok_or_else(|| ApiError::BadRequest("model required".to_owned()))?;

    validate_non_empty("text", &body.text)?;

    let mut user_prompt = body.text.clone();
    if let Some(ctx) = &body.context {
        let mut ctx_lines = Vec::new();
        if let Some(url) = &ctx.url {
            ctx_lines.push(format!("URL: {url}"));
        }
        if let Some(title) = &ctx.title {
            ctx_lines.push(format!("Page title: {title}"));
        }
        if !ctx_lines.is_empty() {
            user_prompt = format!("{}\n\nTEXT:\n{}", ctx_lines.join("\n"), body.text);
        }
    }

    let payload = serde_json::json!({
        "model": model,
        "temperature": 0,
        "messages": [
            {"role": "system", "content": LLM_SMART_STORE_SYSTEM_PROMPT},
            {"role": "user", "content": user_prompt}
        ]
    });

    let base_url = openai_config
        .base_url
        .as_deref()
        .expect("is_configured() already checked");

    let mut req = state
        .http_client
        .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
        .json(&payload);
    if let Some(key) = openai_config.api_key.as_deref() {
        req = req.bearer_auth(key);
    }

    let response = req
        .send()
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .error_for_status()
        .map_err(|e| ApiError::Internal(e.into()))?;

    #[derive(serde::Deserialize)]
    struct Resp { choices: Vec<Choice> }
    #[derive(serde::Deserialize)]
    struct Choice { message: Msg }
    #[derive(serde::Deserialize)]
    struct Msg { content: Option<String> }

    let chat: Resp = response
        .json()
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let content = chat
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();

    let items: Vec<SmartStoreItem> = serde_json::from_str(&content).map_err(|_| {
        ApiError::Internal(anyhow::anyhow!(
            "LLM did not return a valid JSON array; got: {content}"
        ))
    })?;

    if items.is_empty() {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "LLM returned an empty item list"
        )));
    }

    let mut responses = Vec::with_capacity(items.len());
    for item in items {
        let store_req = StoreRequest {
            id: None,
            text: item.text,
            metadata: item.metadata,
            source_id: item.source_id,
            chunk: None,
        };
        let resp = store_entry_core(&state, store_req, session.0.clone()).await?;
        responses.push(resp);
    }

    Ok((StatusCode::CREATED, Json(SmartStoreResponse { items: responses })))
}

async fn rebuild_graph(
    State(state): State<AppState>,
) -> Result<Json<GraphRebuildResponse>, ApiError> {
    let store = state.store.clone();
    let rebuilt_edges = tokio::task::spawn_blocking(move || store.rebuild_similarity_graph())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(map_graph_error)?;

    Ok(Json(GraphRebuildResponse { rebuilt_edges }))
}

async fn create_manual_edge(
    State(state): State<AppState>,
    Json(request): Json<CreateManualEdgeRequest>,
) -> Result<(StatusCode, Json<GraphEdgePayload>), ApiError> {
    validate_non_empty("from_item_id", &request.from_item_id)?;
    validate_non_empty("to_item_id", &request.to_item_id)?;
    validate_metadata(&request.metadata)?;

    let store = state.store.clone();
    let input = ManualEdgeInput {
        from_item_id: request.from_item_id,
        to_item_id: request.to_item_id,
        relation: request.relation,
        weight: request.weight.unwrap_or(1.0),
        directed: request.directed.unwrap_or(false),
        metadata: request.metadata,
    };

    let edge = tokio::task::spawn_blocking(move || store.add_manual_edge(input))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(map_graph_error)?;

    Ok((StatusCode::CREATED, Json(edge.into())))
}

async fn delete_graph_edge(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, ApiError> {
    let store = state.store.clone();
    let deleted = tokio::task::spawn_blocking({
        let id = id.clone();
        move || store.delete_graph_edge(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(map_graph_error)?;

    if !deleted {
        return Err(ApiError::NotFound(format!("graph edge {id} not found")));
    }

    Ok(Json(DeleteResponse { id, deleted }))
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    ServiceUnavailable(String),
    #[error(transparent)]
    Internal(anyhow::Error),
    #[error(transparent)]
    TaskJoin(#[from] tokio::task::JoinError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message.clone()),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message.clone()),
            Self::ServiceUnavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message.clone()),
            Self::Internal(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::TaskJoin(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };

        if status.is_server_error() {
            tracing::error!(
                status = %status,
                error = %self,
                "api request failed"
            );
        }

        (
            status,
            Json(ErrorResponse {
                error: error_message,
            }),
        )
            .into_response()
    }
}

impl From<SearchHit> for SearchResultPayload {
    fn from(value: SearchHit) -> Self {
        Self {
            id: value.id,
            text: value.text,
            metadata: value.metadata,
            source_id: value.source_id,
            created_at: value.created_at,
            distance: value.distance,
            chunk_context: None,
        }
    }
}

impl From<CategorySummary> for AdminCategoryPayload {
    fn from(value: CategorySummary) -> Self {
        Self {
            source_id: value.source_id,
            item_count: value.item_count,
        }
    }
}

impl From<ItemRecord> for AdminItemPayload {
    fn from(value: ItemRecord) -> Self {
        Self {
            id: value.id,
            text: value.text,
            metadata: value.metadata,
            source_id: value.source_id,
            created_at: value.created_at,
        }
    }
}

impl From<GraphStatus> for GraphStatusResponse {
    fn from(value: GraphStatus) -> Self {
        Self {
            enabled: value.enabled,
            build_on_startup: value.build_on_startup,
            similarity_top_k: value.similarity_top_k,
            similarity_max_distance: value.similarity_max_distance,
            cross_source: value.cross_source,
            item_count: value.item_count,
            edge_count: value.edge_count,
            similarity_edge_count: value.similarity_edge_count,
            manual_edge_count: value.manual_edge_count,
        }
    }
}

impl From<GraphEdgeRecord> for GraphEdgePayload {
    fn from(value: GraphEdgeRecord) -> Self {
        Self {
            id: value.id,
            from_item_id: value.from_item_id,
            to_item_id: value.to_item_id,
            edge_type: value.edge_type,
            relation: value.relation,
            weight: value.weight,
            directed: value.directed,
            metadata: value.metadata,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<GraphNeighborhood> for GraphNeighborhoodResponse {
    fn from(value: GraphNeighborhood) -> Self {
        Self {
            center_id: value.center_id,
            nodes: value.nodes.into_iter().map(Into::into).collect(),
            edges: value.edges.into_iter().map(Into::into).collect(),
            pairwise_distances: value
                .pairwise_distances
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<GraphNodeDistance> for GraphNodeDistancePayload {
    fn from(value: GraphNodeDistance) -> Self {
        Self {
            from_item_id: value.from_item_id,
            to_item_id: value.to_item_id,
            distance: value.distance,
        }
    }
}

pub(super) fn validate_metadata(metadata: &Value) -> Result<(), ApiError> {
    if !metadata.is_object() {
        return Err(ApiError::BadRequest(
            "metadata must be a JSON object".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn validate_source_id(source_id: &str) -> Result<(), ApiError> {
    if source_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "source_id must not be empty".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn validate_non_empty(field: &str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::BadRequest(format!("{field} must not be empty")));
    }
    Ok(())
}

pub(super) fn resolve_store_id(id: Option<String>) -> String {
    match id {
        Some(id) => {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                Uuid::now_v7().to_string()
            } else {
                trimmed.to_owned()
            }
        }
        None => Uuid::now_v7().to_string(),
    }
}

pub(super) fn validate_graph_depth(depth: usize) -> Result<(), ApiError> {
    if depth > 5 {
        return Err(ApiError::BadRequest(
            "depth must be less than or equal to 5".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn validate_graph_limit(limit: usize) -> Result<(), ApiError> {
    if limit == 0 || limit > 500 {
        return Err(ApiError::BadRequest(
            "limit must be between 1 and 500".to_owned(),
        ));
    }
    Ok(())
}

fn default_metadata() -> Value {
    Value::Object(serde_json::Map::new())
}

pub(super) fn current_timestamp_millis() -> Result<i64, ApiError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;
    Ok(now.as_millis() as i64)
}

fn map_missing_item(kind: &str, error: anyhow::Error) -> ApiError {
    let error_string = error.to_string();
    if error_string.contains("not found") {
        ApiError::NotFound(error_string)
    } else {
        ApiError::Internal(error.context(format!("failed to update {kind}")))
    }
}

pub(super) fn map_graph_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("graph support is disabled") {
        ApiError::ServiceUnavailable(message)
    } else if message.contains("not found") {
        ApiError::NotFound(message)
    } else if message.contains("must") || message.contains("distinct") {
        ApiError::BadRequest(message)
    } else {
        ApiError::Internal(error.context("graph operation failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use serde_json::json;
    use std::{
        collections::{BTreeMap, HashMap, HashSet, VecDeque},
        sync::Mutex,
    };

    struct MockEmbedder {
        embedding: Vec<f32>,
        seen_inputs: Mutex<Vec<String>>,
    }

    impl MockEmbedder {
        fn new(embedding: Vec<f32>) -> Self {
            Self {
                embedding,
                seen_inputs: Mutex::new(Vec::new()),
            }
        }
    }

    impl EmbeddingService for MockEmbedder {
        fn embed(&self, text: &str) -> Result<Vec<f32>> {
            self.seen_inputs
                .lock()
                .expect("embedder mutex poisoned")
                .push(text.to_owned());
            Ok(self.embedding.clone())
        }
    }

    struct MockStore {
        stored: Mutex<Vec<(ItemRecord, Vec<f32>)>>,
        search_results: Mutex<Vec<SearchHit>>,
        search_source_ids: Mutex<Vec<Option<String>>>,
        graph_enabled: bool,
        graph_edges: Mutex<Vec<GraphEdgeRecord>>,
        graph_rebuilds: Mutex<usize>,
        mcp_tokens: Mutex<Vec<crate::db::McpTokenRecord>>,
        mcp_token_hashes: Mutex<HashMap<String, String>>,
        device_auths: Mutex<Vec<crate::db::DeviceAuthRecord>>,
    }

    impl Default for MockStore {
        fn default() -> Self {
            Self {
                stored: Mutex::new(Vec::new()),
                search_results: Mutex::new(Vec::new()),
                search_source_ids: Mutex::new(Vec::new()),
                graph_enabled: false,
                graph_edges: Mutex::new(Vec::new()),
                graph_rebuilds: Mutex::new(0),
                mcp_tokens: Mutex::new(Vec::new()),
                mcp_token_hashes: Mutex::new(HashMap::new()),
                device_auths: Mutex::new(Vec::new()),
            }
        }
    }

    impl MockStore {
        fn with_results(results: Vec<SearchHit>) -> Self {
            Self {
                search_results: Mutex::new(results),
                ..Self::default()
            }
        }

        fn seed(items: Vec<ItemRecord>) -> Self {
            Self {
                stored: Mutex::new(items.into_iter().map(|item| (item, Vec::new())).collect()),
                ..Self::default()
            }
        }

        fn seed_graph(items: Vec<ItemRecord>, edges: Vec<GraphEdgeRecord>) -> Self {
            Self {
                stored: Mutex::new(items.into_iter().map(|item| (item, Vec::new())).collect()),
                graph_enabled: true,
                graph_edges: Mutex::new(edges),
                ..Self::default()
            }
        }
    }

    impl VectorStore for MockStore {
        fn upsert_item(&self, item: ItemRecord, embedding: &[f32]) -> Result<()> {
            let mut stored = self.stored.lock().expect("store mutex poisoned");
            if let Some(existing) = stored
                .iter_mut()
                .find(|(existing, _)| existing.id == item.id)
            {
                *existing = (item, embedding.to_vec());
            } else {
                stored.push((item, embedding.to_vec()));
            }
            Ok(())
        }

        fn search(
            &self,
            _query_embedding: &[f32],
            _top_k: usize,
            source_id: Option<&str>,
        ) -> Result<Vec<SearchHit>> {
            self.search_source_ids
                .lock()
                .expect("store mutex poisoned")
                .push(source_id.map(str::to_owned));
            Ok(self
                .search_results
                .lock()
                .expect("store mutex poisoned")
                .clone())
        }

        fn search_hybrid(
            &self,
            _query_text: &str,
            query_embedding: &[f32],
            top_k: usize,
            source_id: Option<&str>,
        ) -> Result<Vec<SearchHit>> {
            self.search(query_embedding, top_k, source_id)
        }

        fn distances_for_ids(
            &self,
            _query_embedding: &[f32],
            ids: &[String],
        ) -> Result<Vec<SearchHit>> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            let results = self.search_results.lock().expect("store mutex poisoned");
            let mut hits = Vec::new();
            for id in ids {
                if let Some(hit) = results.iter().find(|h| &h.id == id) {
                    hits.push(hit.clone());
                    continue;
                }
                if let Some((item, _)) = stored.iter().find(|(item, _)| &item.id == id) {
                    hits.push(SearchHit {
                        id: item.id.clone(),
                        text: item.text.clone(),
                        metadata: item.metadata.clone(),
                        source_id: item.source_id.clone(),
                        created_at: item.created_at,
                        distance: 0.0,
                    });
                }
            }
            Ok(hits)
        }

        fn list_categories(&self) -> Result<Vec<CategorySummary>> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            let mut counts = BTreeMap::<String, i64>::new();
            for (item, _) in stored.iter() {
                *counts.entry(item.source_id.clone()).or_default() += 1;
            }
            Ok(counts
                .into_iter()
                .map(|(source_id, item_count)| CategorySummary {
                    source_id,
                    item_count,
                })
                .collect())
        }

        fn list_large_items(&self, min_chars: usize, _limit: usize, _offset: usize) -> Result<(Vec<ItemRecord>, i64)> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            let items: Vec<_> = stored
                .iter()
                .filter(|(item, _)| {
                    item.text.len() > min_chars
                        && item.metadata.get("_chunk").is_none()
                })
                .map(|(item, _)| item.clone())
                .collect();
            let total = items.len() as i64;
            Ok((items, total))
        }

        fn list_items(&self, request: ListItemsRequest) -> Result<(Vec<ItemRecord>, i64)> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            let mut items = stored
                .iter()
                .filter(|(item, _)| {
                    if let Some(source) = &request.source_id {
                        if &item.source_id != source { return false; }
                    }
                    if let Some(min) = request.min_created_at {
                        if item.created_at < min { return false; }
                    }
                    if let Some(max) = request.max_created_at {
                        if item.created_at > max { return false; }
                    }
                    for (key, val) in &request.metadata_filter {
                        if let Some(meta_val) = item.metadata.get(key) {
                            if meta_val.as_str() != Some(val) { return false; }
                        } else {
                            return false;
                        }
                    }
                    true
                })
                .map(|(item, _)| item.clone())
                .collect::<Vec<_>>();

            let total_count = items.len() as i64;

            items.sort_by(|a, b| {
                let ordering = b
                    .created_at
                    .cmp(&a.created_at)
                    .then_with(|| a.id.cmp(&b.id));
                match request.sort_order {
                    SortOrder::Asc => ordering.reverse(),
                    SortOrder::Desc => ordering,
                }
            });

            let offset = request.offset.unwrap_or(0);
            let limit = request.limit.unwrap_or(100);
            let paged_items = items.into_iter().skip(offset).take(limit).collect();

            Ok((paged_items, total_count))
        }

        fn get_item(&self, id: &str) -> Result<Option<ItemRecord>> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            Ok(stored
                .iter()
                .find(|(item, _)| item.id == id)
                .map(|(item, _)| item.clone()))
        }

        fn delete_item(&self, id: &str) -> Result<bool> {
            let mut stored = self.stored.lock().expect("store mutex poisoned");
            let before = stored.len();
            stored.retain(|(item, _)| item.id != id);
            Ok(stored.len() != before)
        }

        fn graph_status(&self) -> Result<GraphStatus> {
            let item_count = self.stored.lock().expect("store mutex poisoned").len() as i64;
            let edges = self.graph_edges.lock().expect("store mutex poisoned");
            let similarity_edge_count = edges
                .iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Similarity)
                .count() as i64;
            let manual_edge_count = edges
                .iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Manual)
                .count() as i64;

            Ok(GraphStatus {
                enabled: self.graph_enabled,
                build_on_startup: false,
                similarity_top_k: 5,
                similarity_max_distance: 0.75,
                cross_source: false,
                item_count,
                edge_count: edges.len() as i64,
                similarity_edge_count,
                manual_edge_count,
            })
        }

        fn graph_neighborhood(
            &self,
            center_id: &str,
            depth: usize,
            limit: usize,
            edge_type: Option<GraphEdgeType>,
        ) -> Result<GraphNeighborhood> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let items = self
                .stored
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .map(|(item, _)| item.clone())
                .collect::<Vec<_>>();
            let item_index = items
                .iter()
                .map(|item| (item.id.clone(), item.clone()))
                .collect::<HashMap<_, _>>();
            if !item_index.contains_key(center_id) {
                anyhow::bail!("item {center_id} not found");
            }

            let edges = self.list_graph_edges(None, edge_type)?;
            let mut visited = HashSet::from([center_id.to_owned()]);
            let mut order = vec![center_id.to_owned()];
            let mut queue = VecDeque::from([(center_id.to_owned(), 0usize)]);
            let mut neighborhood_edges = HashMap::new();

            while let Some((current_id, current_depth)) = queue.pop_front() {
                if current_depth >= depth {
                    continue;
                }

                for edge in edges
                    .iter()
                    .filter(|edge| edge.from_item_id == current_id || edge.to_item_id == current_id)
                {
                    neighborhood_edges.insert(edge.id.clone(), edge.clone());
                    for next in [&edge.from_item_id, &edge.to_item_id] {
                        if visited.len() >= limit || visited.contains(next) {
                            continue;
                        }
                        visited.insert(next.clone());
                        order.push(next.clone());
                        queue.push_back((next.clone(), current_depth + 1));
                    }
                }
            }

            let nodes = order
                .into_iter()
                .filter_map(|id| item_index.get(&id).cloned())
                .collect::<Vec<_>>();
            let mut edges = neighborhood_edges
                .into_values()
                .filter(|edge| {
                    visited.contains(&edge.from_item_id) && visited.contains(&edge.to_item_id)
                })
                .collect::<Vec<_>>();
            edges.sort_by(|a, b| a.id.cmp(&b.id));

            Ok(GraphNeighborhood {
                center_id: center_id.to_owned(),
                nodes,
                edges,
                pairwise_distances: vec![],
            })
        }

        fn list_graph_edges(
            &self,
            item_id: Option<&str>,
            edge_type: Option<GraphEdgeType>,
        ) -> Result<Vec<GraphEdgeRecord>> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let edges = self.graph_edges.lock().expect("store mutex poisoned");
            Ok(edges
                .iter()
                .filter(|edge| {
                    item_id.is_none_or(|id| edge.from_item_id == id || edge.to_item_id == id)
                })
                .filter(|edge| edge_type.is_none_or(|kind| edge.edge_type == kind))
                .cloned()
                .collect())
        }

        fn rebuild_similarity_graph(&self) -> Result<usize> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            *self.graph_rebuilds.lock().expect("store mutex poisoned") += 1;
            Ok(self
                .graph_edges
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Similarity)
                .count())
        }

        fn add_manual_edge(&self, input: ManualEdgeInput) -> Result<GraphEdgeRecord> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let items = self.stored.lock().expect("store mutex poisoned");
            if !items.iter().any(|(item, _)| item.id == input.from_item_id) {
                anyhow::bail!("item {} not found", input.from_item_id);
            }
            if !items.iter().any(|(item, _)| item.id == input.to_item_id) {
                anyhow::bail!("item {} not found", input.to_item_id);
            }
            drop(items);

            let mut edges = self.graph_edges.lock().expect("store mutex poisoned");
            let timestamp = edges.len() as i64 + 1;
            let edge = GraphEdgeRecord {
                id: format!("manual-{}", edges.len() + 1),
                from_item_id: input.from_item_id,
                to_item_id: input.to_item_id,
                edge_type: GraphEdgeType::Manual,
                relation: input.relation,
                weight: input.weight,
                directed: input.directed,
                metadata: input.metadata,
                created_at: timestamp,
                updated_at: timestamp,
            };
            edges.push(edge.clone());
            Ok(edge)
        }

        fn delete_graph_edge(&self, id: &str) -> Result<bool> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let mut edges = self.graph_edges.lock().expect("store mutex poisoned");
            if edges
                .iter()
                .any(|edge| edge.id == id && edge.edge_type == GraphEdgeType::Similarity)
            {
                anyhow::bail!("similarity edges must be rebuilt, not deleted manually");
            }
            let before = edges.len();
            edges.retain(|edge| edge.id != id);
            Ok(edges.len() != before)
        }
    }

    impl AuthStore for MockStore {
        fn create_mcp_token(
            &self,
            token: crate::db::NewMcpToken,
        ) -> Result<crate::db::McpTokenRecord> {
            let record = crate::db::McpTokenRecord {
                id: token.id.clone(),
                name: token.name.clone(),
                subject: token.subject.clone(),
                created_at: token.created_at,
                last_used_at: None,
                expires_at: token.expires_at,
            };
            self.mcp_token_hashes
                .lock()
                .expect("store mutex poisoned")
                .insert(token.token_hash, token.id);
            self.mcp_tokens
                .lock()
                .expect("store mutex poisoned")
                .push(record.clone());
            Ok(record)
        }

        fn find_mcp_token_by_hash(&self, hash: &str) -> Result<Option<crate::db::McpTokenRecord>> {
            let hashes = self.mcp_token_hashes.lock().expect("store mutex poisoned");
            let tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            Ok(hashes
                .get(hash)
                .and_then(|id| tokens.iter().find(|record| record.id == *id).cloned()))
        }

        fn touch_mcp_token(&self, id: &str, now: i64) -> Result<()> {
            let mut tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            for record in tokens.iter_mut() {
                if record.id == id {
                    record.last_used_at = Some(now);
                    break;
                }
            }
            Ok(())
        }

        fn list_mcp_tokens(&self, subject: Option<&str>) -> Result<Vec<crate::db::McpTokenRecord>> {
            let tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            Ok(tokens
                .iter()
                .filter(|record| match subject {
                    Some(subject) => record.subject.as_deref() == Some(subject),
                    None => true,
                })
                .cloned()
                .collect())
        }

        fn delete_mcp_token(&self, id: &str, subject: Option<&str>) -> Result<bool> {
            let mut tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            let before = tokens.len();
            tokens.retain(|record| {
                record.id != id
                    || match subject {
                        Some(subject) => record.subject.as_deref() != Some(subject),
                        None => false,
                    }
            });
            Ok(tokens.len() != before)
        }

        fn create_device_auth(
            &self,
            request: crate::db::NewDeviceAuth,
        ) -> Result<crate::db::DeviceAuthRecord> {
            let record = crate::db::DeviceAuthRecord {
                device_code: request.device_code,
                user_code: request.user_code,
                status: crate::db::DeviceAuthStatus::Pending,
                token_id: None,
                subject: None,
                client_name: request.client_name,
                created_at: request.created_at,
                expires_at: request.expires_at,
                interval_secs: request.interval_secs,
                last_polled_at: None,
            };
            self.device_auths
                .lock()
                .expect("store mutex poisoned")
                .push(record.clone());
            Ok(record)
        }

        fn find_device_auth_by_device_code(
            &self,
            device_code: &str,
        ) -> Result<Option<crate::db::DeviceAuthRecord>> {
            Ok(self
                .device_auths
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .find(|record| record.device_code == device_code)
                .cloned())
        }

        fn find_device_auth_by_user_code(
            &self,
            user_code: &str,
        ) -> Result<Option<crate::db::DeviceAuthRecord>> {
            Ok(self
                .device_auths
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .find(|record| record.user_code == user_code)
                .cloned())
        }

        fn approve_device_auth(
            &self,
            user_code: &str,
            token_id: &str,
            subject: Option<&str>,
            now: i64,
        ) -> Result<bool> {
            let mut auths = self.device_auths.lock().expect("store mutex poisoned");
            for record in auths.iter_mut() {
                if record.user_code == user_code
                    && matches!(record.status, crate::db::DeviceAuthStatus::Pending)
                    && record.expires_at > now
                {
                    record.status = crate::db::DeviceAuthStatus::Approved;
                    record.token_id = Some(token_id.to_owned());
                    record.subject = subject.map(str::to_owned);
                    return Ok(true);
                }
            }
            Ok(false)
        }

        fn touch_device_poll(&self, device_code: &str, now: i64) -> Result<()> {
            let mut auths = self.device_auths.lock().expect("store mutex poisoned");
            for record in auths.iter_mut() {
                if record.device_code == device_code {
                    record.last_polled_at = Some(now);
                    break;
                }
            }
            Ok(())
        }

        fn expire_device_auths(&self, now: i64) -> Result<usize> {
            let mut auths = self.device_auths.lock().expect("store mutex poisoned");
            let mut expired = 0;
            for record in auths.iter_mut() {
                if matches!(record.status, crate::db::DeviceAuthStatus::Pending)
                    && record.expires_at <= now
                {
                    record.status = crate::db::DeviceAuthStatus::Expired;
                    expired += 1;
                }
            }
            Ok(expired)
        }
    }

    fn manual_edge(id: &str, from: &str, to: &str) -> GraphEdgeRecord {
        GraphEdgeRecord {
            id: id.to_owned(),
            from_item_id: from.to_owned(),
            to_item_id: to.to_owned(),
            edge_type: GraphEdgeType::Manual,
            relation: Some("supports".to_owned()),
            weight: 1.0,
            directed: true,
            metadata: json!({"kind": "manual"}),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn similarity_edge(id: &str, from: &str, to: &str) -> GraphEdgeRecord {
        GraphEdgeRecord {
            id: id.to_owned(),
            from_item_id: from.to_owned(),
            to_item_id: to.to_owned(),
            edge_type: GraphEdgeType::Similarity,
            relation: None,
            weight: 0.9,
            directed: false,
            metadata: json!({"distance": 0.2}),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn store_route_embeds_and_persists_payload() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.25, 0.75]));
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/store")
            .json(&json!({
                "id": "doc-1",
                "text": "hello world",
                "metadata": { "source": "unit-test" },
                "source_id": "knowledge"
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body = response.json::<StoreResponse>();
        assert_eq!(body.id, "doc-1");
        assert_eq!(body.source_id, "knowledge");
        assert!(body.created_at > 0);

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0.id, "doc-1");
        assert_eq!(stored[0].0.metadata, json!({ "source": "unit-test" }));
        assert_eq!(stored[0].0.source_id, "knowledge");
        assert!(stored[0].0.created_at > 0);
        assert_eq!(stored[0].1, vec![0.25, 0.75]);
    }

    #[tokio::test]
    async fn store_route_generates_id_when_missing() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.25, 0.75]));
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/store")
            .json(&json!({
                "text": "hello world",
                "metadata": { "source": "unit-test" },
                "source_id": "knowledge"
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body = response.json::<StoreResponse>();
        assert!(!body.id.trim().is_empty());
        assert_eq!(body.source_id, "knowledge");

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0.id, body.id);
    }

    #[tokio::test]
    async fn store_route_generates_id_when_blank() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.25, 0.75]));
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/store")
            .json(&json!({
                "id": "   ",
                "text": "hello world",
                "metadata": { "source": "unit-test" },
                "source_id": "knowledge"
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body = response.json::<StoreResponse>();
        assert!(!body.id.trim().is_empty());

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0.id, body.id);
    }

    #[tokio::test]
    async fn search_route_returns_ranked_results() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![SearchHit {
            id: "doc-7".to_owned(),
            text: "stored text".to_owned(),
            metadata: json!({ "label": "match" }),
            source_id: "memory".to_owned(),
            created_at: 1234,
            distance: 0.0125,
        }]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/search")
            .json(&json!({
                "query": "hello",
                "top_k": 1,
                "source_id": "memory"
            }))
            .await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "results": [{
                "id": "doc-7",
                "text": "stored text",
                "metadata": { "label": "match" },
                "source_id": "memory",
                "created_at": 1234,
                "distance": 0.0125
            }],
            "related": []
        }));

        let search_source_ids = store
            .search_source_ids
            .lock()
            .expect("store mutex poisoned");
        assert_eq!(search_source_ids.as_slice(), &[Some("memory".to_owned())]);
    }

    #[tokio::test]
    async fn search_route_filters_by_max_distance() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![
            SearchHit {
                id: "doc-near".to_owned(),
                text: "close".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 1,
                distance: 0.3,
            },
            SearchHit {
                id: "doc-far".to_owned(),
                text: "far".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 2,
                distance: 1.5,
            },
        ]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "hello", "top_k": 5 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 1);
        assert_eq!(body.results[0].id, "doc-near");
    }

    #[tokio::test]
    async fn search_route_returns_related_manual_neighbors_of_top_hit() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore {
            stored: Mutex::new(vec![
                (
                    ItemRecord {
                        id: "doc-top".to_owned(),
                        text: "kubernetes ingress".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 1,
                    },
                    Vec::new(),
                ),
                (
                    ItemRecord {
                        id: "doc-linked".to_owned(),
                        text: "kubernetes storage".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 2,
                    },
                    Vec::new(),
                ),
                (
                    ItemRecord {
                        id: "doc-similar".to_owned(),
                        text: "sim neighbor".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 3,
                    },
                    Vec::new(),
                ),
            ]),
            search_results: Mutex::new(vec![SearchHit {
                id: "doc-top".to_owned(),
                text: "kubernetes ingress".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 1,
                distance: 0.2,
            }]),
            search_source_ids: Mutex::new(Vec::new()),
            graph_enabled: true,
            graph_edges: Mutex::new(vec![
                manual_edge("manual-1", "doc-top", "doc-linked"),
                similarity_edge("sim-1", "doc-top", "doc-similar"),
            ]),
            graph_rebuilds: Mutex::new(0),
            mcp_tokens: Mutex::new(Vec::new()),
            mcp_token_hashes: Mutex::new(HashMap::new()),
            device_auths: Mutex::new(Vec::new()),
        });
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "kubernetes ingress", "top_k": 5 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 1);
        assert_eq!(body.results[0].id, "doc-top");
        assert_eq!(body.related.len(), 1);
        assert_eq!(body.related[0].id, "doc-linked");
        assert_eq!(body.related[0].relation.as_deref(), Some("supports"));
    }

    #[tokio::test]
    async fn search_route_defaults_top_k_when_omitted() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![SearchHit {
            id: "doc-1".to_owned(),
            text: "hit".to_owned(),
            metadata: json!({}),
            source_id: "memory".to_owned(),
            created_at: 1,
            distance: 0.1,
        }]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "hello" }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 1);
    }

    #[tokio::test]
    async fn search_route_excludes_related_already_in_results() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore {
            stored: Mutex::new(vec![
                (
                    ItemRecord {
                        id: "doc-top".to_owned(),
                        text: "top".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 1,
                    },
                    Vec::new(),
                ),
                (
                    ItemRecord {
                        id: "doc-linked".to_owned(),
                        text: "linked".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 2,
                    },
                    Vec::new(),
                ),
            ]),
            search_results: Mutex::new(vec![
                SearchHit {
                    id: "doc-top".to_owned(),
                    text: "top".to_owned(),
                    metadata: json!({}),
                    source_id: "memory".to_owned(),
                    created_at: 1,
                    distance: 0.1,
                },
                SearchHit {
                    id: "doc-linked".to_owned(),
                    text: "linked".to_owned(),
                    metadata: json!({}),
                    source_id: "memory".to_owned(),
                    created_at: 2,
                    distance: 0.4,
                },
            ]),
            search_source_ids: Mutex::new(Vec::new()),
            graph_enabled: true,
            graph_edges: Mutex::new(vec![manual_edge("manual-1", "doc-top", "doc-linked")]),
            graph_rebuilds: Mutex::new(0),
            mcp_tokens: Mutex::new(Vec::new()),
            mcp_token_hashes: Mutex::new(HashMap::new()),
            device_auths: Mutex::new(Vec::new()),
        });
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "top", "top_k": 5 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 2);
        assert!(body.related.is_empty(), "doc-linked is already in results");
    }

    #[tokio::test]
    async fn search_route_respects_custom_max_distance() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![
            SearchHit {
                id: "doc-near".to_owned(),
                text: "close".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 1,
                distance: 0.3,
            },
            SearchHit {
                id: "doc-far".to_owned(),
                text: "far".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 2,
                distance: 1.5,
            },
        ]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "hello", "top_k": 5, "max_distance": 2.0 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 2);
    }

    #[tokio::test]
    async fn graph_status_route_reports_disabled_state() {
        let store = Arc::new(MockStore::seed(vec![]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/graph/status").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "enabled": false,
            "build_on_startup": false,
            "similarity_top_k": 5,
            "similarity_max_distance": 0.75,
            "cross_source": false,
            "item_count": 0,
            "edge_count": 0,
            "similarity_edge_count": 0,
            "manual_edge_count": 0
        }));
    }

    #[tokio::test]
    async fn graph_neighborhood_route_returns_nodes_and_edges() {
        let store = Arc::new(MockStore::seed_graph(
            vec![
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "one".to_owned(),
                    metadata: json!({"kind":"a"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 100,
                },
                ItemRecord {
                    id: "doc-2".to_owned(),
                    text: "two".to_owned(),
                    metadata: json!({"kind":"b"}),
                    source_id: "memory".to_owned(),
                    created_at: 200,
                },
                ItemRecord {
                    id: "doc-3".to_owned(),
                    text: "three".to_owned(),
                    metadata: json!({"kind":"c"}),
                    source_id: "memory".to_owned(),
                    created_at: 300,
                },
            ],
            vec![
                similarity_edge("sim-1", "doc-2", "doc-3"),
                manual_edge("manual-1", "doc-2", "doc-1"),
            ],
        ));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .get("/graph/neighborhood/doc-2?depth=1&limit=10")
            .await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "center_id": "doc-2",
            "nodes": [
                {
                    "id": "doc-2",
                    "text": "two",
                    "metadata": {"kind":"b"},
                    "source_id": "memory",
                    "created_at": 200
                },
                {
                    "id": "doc-3",
                    "text": "three",
                    "metadata": {"kind":"c"},
                    "source_id": "memory",
                    "created_at": 300
                },
                {
                    "id": "doc-1",
                    "text": "one",
                    "metadata": {"kind":"a"},
                    "source_id": "knowledge",
                    "created_at": 100
                }
            ],
            "edges": [
                {
                    "id": "manual-1",
                    "from_item_id": "doc-2",
                    "to_item_id": "doc-1",
                    "edge_type": "manual",
                    "relation": "supports",
                    "weight": 1.0,
                    "directed": true,
                    "metadata": {"kind":"manual"},
                    "created_at": 1,
                    "updated_at": 1
                },
                {
                    "id": "sim-1",
                    "from_item_id": "doc-2",
                    "to_item_id": "doc-3",
                    "edge_type": "similarity",
                    "relation": null,
                    "weight": 0.9,
                    "directed": false,
                    "metadata": {"distance":0.2},
                    "created_at": 1,
                    "updated_at": 1
                }
            ],
            "pairwise_distances": []
        }));
    }

    #[tokio::test]
    async fn create_and_delete_manual_edge_routes_work() {
        let store = Arc::new(MockStore::seed_graph(
            vec![
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "one".to_owned(),
                    metadata: json!({"kind":"a"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 100,
                },
                ItemRecord {
                    id: "doc-2".to_owned(),
                    text: "two".to_owned(),
                    metadata: json!({"kind":"b"}),
                    source_id: "memory".to_owned(),
                    created_at: 200,
                },
            ],
            vec![],
        ));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let created = server
            .post("/admin/graph/edges")
            .json(&json!({
                "from_item_id": "doc-2",
                "to_item_id": "doc-1",
                "relation": "supports",
                "metadata": { "kind": "manual" }
            }))
            .await;

        assert_eq!(created.status_code(), StatusCode::CREATED);
        let created_body = created.json::<GraphEdgePayload>();
        assert_eq!(created_body.edge_type, GraphEdgeType::Manual);

        let deleted = server
            .delete(&format!("/admin/graph/edges/{}", created_body.id))
            .await;

        deleted.assert_status_ok();
        deleted.assert_json(&json!({
            "id": created_body.id,
            "deleted": true
        }));
        assert!(
            store
                .graph_edges
                .lock()
                .expect("store mutex poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn rebuild_graph_route_returns_edge_count() {
        let store = Arc::new(MockStore::seed_graph(
            vec![ItemRecord {
                id: "doc-1".to_owned(),
                text: "one".to_owned(),
                metadata: json!({"kind":"a"}),
                source_id: "knowledge".to_owned(),
                created_at: 100,
            }],
            vec![similarity_edge("sim-1", "doc-1", "doc-2")],
        ));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server.post("/admin/graph/rebuild").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "rebuilt_edges": 1
        }));
        assert_eq!(
            *store.graph_rebuilds.lock().expect("store mutex poisoned"),
            1
        );
    }

    #[tokio::test]
    async fn list_categories_route_returns_category_counts() {
        let store = Arc::new(MockStore::seed(vec![
            ItemRecord {
                id: "doc-1".to_owned(),
                text: "one".to_owned(),
                metadata: json!({"kind":"a"}),
                source_id: "knowledge".to_owned(),
                created_at: 100,
            },
            ItemRecord {
                id: "doc-2".to_owned(),
                text: "two".to_owned(),
                metadata: json!({"kind":"b"}),
                source_id: "memory".to_owned(),
                created_at: 200,
            },
            ItemRecord {
                id: "doc-3".to_owned(),
                text: "three".to_owned(),
                metadata: json!({"kind":"c"}),
                source_id: "memory".to_owned(),
                created_at: 300,
            },
        ]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/categories").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "categories": [
                { "source_id": "knowledge", "item_count": 1 },
                { "source_id": "memory", "item_count": 2 }
            ]
        }));
    }

    #[tokio::test]
    async fn list_items_route_can_filter_by_category() {
        let store = Arc::new(MockStore::seed(vec![
            ItemRecord {
                id: "doc-1".to_owned(),
                text: "one".to_owned(),
                metadata: json!({"kind":"a"}),
                source_id: "knowledge".to_owned(),
                created_at: 100,
            },
            ItemRecord {
                id: "doc-2".to_owned(),
                text: "two".to_owned(),
                metadata: json!({"kind":"b"}),
                source_id: "memory".to_owned(),
                created_at: 200,
            },
        ]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/items?source_id=memory").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "items": [{
                "id": "doc-2",
                "text": "two",
                "metadata": {"kind":"b"},
                "source_id": "memory",
                "created_at": 200
            }],
            "total_count": 1
        }));
    }

    #[tokio::test]
    async fn get_item_route_returns_full_entry() {
        let store = Arc::new(MockStore::seed(vec![ItemRecord {
            id: "doc-1".to_owned(),
            text: "full content".to_owned(),
            metadata: json!({ "kind": "reference" }),
            source_id: "knowledge".to_owned(),
            created_at: 42,
        }]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.0]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/items/doc-1").await;
        response.assert_status_ok();
        response.assert_json(&json!({
            "id": "doc-1",
            "text": "full content",
            "metadata": { "kind": "reference" },
            "source_id": "knowledge",
            "created_at": 42
        }));
    }

    #[tokio::test]
    async fn get_item_route_returns_404_when_missing() {
        let store = Arc::new(MockStore::seed(vec![]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.0]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/items/nope").await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_item_route_reembeds_and_preserves_created_at() {
        let store = Arc::new(MockStore::seed(vec![ItemRecord {
            id: "doc-1".to_owned(),
            text: "old".to_owned(),
            metadata: json!({"kind":"old"}),
            source_id: "knowledge".to_owned(),
            created_at: 123,
        }]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.9, 0.1]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .put("/admin/items/doc-1")
            .json(&json!({
                "text": "new text",
                "metadata": { "kind": "new" },
                "source_id": "memory"
            }))
            .await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "id": "doc-1",
            "text": "new text",
            "metadata": { "kind": "new" },
            "source_id": "memory",
            "created_at": 123
        }));

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored[0].0.source_id, "memory");
        assert_eq!(stored[0].0.created_at, 123);
        assert_eq!(stored[0].1, vec![0.9, 0.1]);
    }

    #[tokio::test]
    async fn delete_item_route_removes_item() {
        let store = Arc::new(MockStore::seed(vec![ItemRecord {
            id: "doc-1".to_owned(),
            text: "old".to_owned(),
            metadata: json!({"kind":"old"}),
            source_id: "knowledge".to_owned(),
            created_at: 123,
        }]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.9, 0.1]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server.delete("/admin/items/doc-1").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "id": "doc-1",
            "deleted": true
        }));
        assert!(
            store
                .stored
                .lock()
                .expect("store mutex poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn health_route_reports_loading_state() {
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new(
            Arc::new(EmbedderHandle::loading()),
            store.clone(),
            store,
            Arc::new(NoopUserMemory),
            AuthConfig::default(),
            OpenAiChatConfig {
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
            ChunkingConfig::default(),
        )));

        let response = server.get("/healthz").await;

        assert_eq!(response.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        response.assert_json(&json!({
            "status": "loading",
            "error": null
        }));
    }

    #[tokio::test]
    async fn openai_chat_route_returns_unauthorized_when_api_key_is_missing() {
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new(
            Arc::new(EmbedderHandle::loading()),
            store.clone(),
            store,
            Arc::new(NoopUserMemory),
            AuthConfig {
                enabled: true,
                frontend_api_key: Some("expected-key".to_owned()),
                ..AuthConfig::default()
            },
            OpenAiChatConfig {
                base_url: Some("http://127.0.0.1:8081".to_owned()),
                default_model: Some("current_model.gguf".to_owned()),
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
            ChunkingConfig::default(),
        )));

        let response = server
            .post("/api/openai/v1/chat/completions")
            .json(&json!({
                "messages": [
                    { "role": "user", "content": "hello" }
                ],
                "stream": true
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
        response.assert_json(&json!({
            "error": "missing x-api-key header, bearer token or valid session cookie"
        }));
    }

    fn mint_session_cookie(secret: &str, sub: &str) -> String {
        use jsonwebtoken::{EncodingKey, Header, encode};

        #[derive(serde::Serialize)]
        struct Claims<'a> {
            sub: &'a str,
            exp: usize,
        }

        let exp = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600) as usize;
        let token = encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &Claims { sub, exp },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();
        format!("rag_session={token}")
    }

    fn auth_test_state() -> (AppState, Arc<MockStore>) {
        let store = Arc::new(MockStore::default());
        let embedder: Arc<dyn EmbeddingService> = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let auth_store: Arc<dyn AuthStore> = store.clone();
        let vector_store: Arc<dyn VectorStore> = store.clone();
        let state = AppState::new(
            Arc::new(EmbedderHandle::ready(embedder)),
            vector_store,
            auth_store,
            Arc::new(NoopUserMemory),
            AuthConfig {
                enabled: true,
                session_secret: Some("test-session-secret".to_owned()),
                app_base_url: Some("http://localhost:3000".to_owned()),
                device_code_ttl_secs: 120,
                device_code_interval_secs: 0,
                ..AuthConfig::default()
            },
            OpenAiChatConfig {
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
            ChunkingConfig::default(),
        );
        (state, store)
    }

    #[tokio::test]
    async fn device_flow_end_to_end_mints_bearer_usable_on_protected_routes() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));

        let code_response = server
            .post("/auth/device/code")
            .json(&json!({"client_name": "unit-test"}))
            .await;
        code_response.assert_status_ok();
        let code_body = code_response.json::<auth::DeviceCodeResponse>();

        let pending = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code_body.device_code}))
            .await;
        assert_eq!(pending.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(
            pending.json::<Value>()["error"],
            json!("authorization_pending")
        );

        let unauth_approve = server
            .post("/auth/device/approve")
            .json(&json!({"user_code": code_body.user_code}))
            .await;
        assert_eq!(unauth_approve.status_code(), StatusCode::UNAUTHORIZED);

        let cookie = mint_session_cookie(&secret, "user-123");
        let approve = server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code_body.user_code}))
            .await;
        approve.assert_status_ok();

        let granted = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code_body.device_code}))
            .await;
        granted.assert_status_ok();
        let token_body = granted.json::<auth::DeviceTokenResponse>();
        assert!(token_body.access_token.starts_with("rag_mcp_"));

        let search = server
            .post("/search")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token_body.access_token)
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({"query": "x"}))
            .await;
        assert_ne!(
            search.status_code(),
            StatusCode::UNAUTHORIZED,
            "minted MCP token should be accepted by protected route"
        );

        let again = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code_body.device_code}))
            .await;
        assert_eq!(
            again.status_code(),
            StatusCode::BAD_REQUEST,
            "token plaintext should only be fetchable once"
        );
    }

    #[tokio::test]
    async fn mcp_endpoint_rejects_unauthenticated_requests() {
        let (state, _store) = auth_test_state();
        let server = TestServer::new(router(state));

        let unauth = server.post("/mcp").json(&json!({"jsonrpc": "2.0"})).await;
        assert_eq!(unauth.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn mcp_endpoint_accepts_authenticated_requests() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));

        let code = server
            .post("/auth/device/code")
            .json(&json!({}))
            .await
            .json::<auth::DeviceCodeResponse>();
        let cookie = mint_session_cookie(&secret, "user-mcp");
        server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code.user_code}))
            .await
            .assert_status_ok();
        let token = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code.device_code}))
            .await
            .json::<auth::DeviceTokenResponse>();

        let response = server
            .post("/mcp")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token.access_token)
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .add_header(
                axum::http::header::HOST,
                "localhost".parse::<axum::http::HeaderValue>().unwrap(),
            )
            .add_header(
                axum::http::header::ACCEPT,
                "application/json, text/event-stream"
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "rust-rag-test", "version": "0.0.1" }
                }
            }))
            .await;
        assert_ne!(
            response.status_code(),
            StatusCode::UNAUTHORIZED,
            "MCP endpoint should accept the minted bearer",
        );
    }

    #[tokio::test]
    async fn revoked_token_is_rejected() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));

        let code = server
            .post("/auth/device/code")
            .json(&json!({}))
            .await
            .json::<auth::DeviceCodeResponse>();
        let cookie = mint_session_cookie(&secret, "user-123");
        server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code.user_code}))
            .await
            .assert_status_ok();
        let token = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code.device_code}))
            .await
            .json::<auth::DeviceTokenResponse>();

        let listed = server
            .get("/api/auth/tokens")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .await;
        listed.assert_status_ok();
        let tokens = listed.json::<auth::ListTokensResponse>();
        assert_eq!(tokens.tokens.len(), 1);
        assert_eq!(tokens.tokens[0].id, token.token_id);

        server
            .delete(&format!("/api/auth/tokens/{}", token.token_id))
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .await
            .assert_status_ok();

        let search = server
            .post("/search")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token.access_token)
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({"query": "x"}))
            .await;
        assert_eq!(search.status_code(), StatusCode::UNAUTHORIZED);
    }
}
