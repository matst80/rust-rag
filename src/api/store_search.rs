use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::db::{
    GraphEdgeType, ItemRecord, ListItemsRequest, NewUserEvent, PROFILE_EVENTS_WINDOW,
    PROFILE_REFRESH_AFTER, SearchHit, SortOrder, UserEventType, UserMemoryStore, VectorStore,
};

use super::analysis;
use super::auth::SessionSubject;
use super::chunking;
use super::error::{
    ApiError, api_validation_error, current_timestamp_millis, metadata_schema, resolve_store_id,
    validate_metadata, validate_non_empty, validate_source_id,
};
use super::state::AppState;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
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
    /// Optional wiki-style hierarchical path for this entry (slash-separated,
    /// e.g. `engineering/runbooks/db`). User-asserted, normalized server-side.
    /// Used by tree navigation (`GET /api/entries/tree`) and is independent of
    /// `source_id` (the namespace) — paths group entries within a namespace.
    /// Distinct from chunk-level `section_path` which is chunker-derived.
    #[serde(default)]
    pub path: Option<String>,
    /// Optional structured-data type name. References a registered schema in
    /// the `schemas` table; if set, `data` must validate against that schema.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// Typed payload validated against the schema for `type`. Required when
    /// `type` is set.
    #[serde(default)]
    pub data: Option<Value>,
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
    /// Restrict results to entries whose `type` equals this value.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,

    /// Optional toggle for hybrid search (Vector + Keyword). Defaults to true.
    #[serde(default = "default_hybrid")]
    pub hybrid: bool,
    /// When `Some(true)`, run the cross-encoder reranker on the top-N
    /// candidate pool and return the reranked top-K. `Some(false)` skips
    /// reranking even when the server has one loaded. `None` defaults to
    /// the server-side `RAG_RERANKER_DEFAULT` (false unless explicitly set).
    /// Has no effect when the server is started without a reranker.
    #[serde(default)]
    pub rerank: Option<bool>,
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
    pub updated_at: i64,
    pub distance: f32,
    /// Stitched context window for chunk results: the matched chunk surrounded by
    /// its immediate neighbours. Null for non-chunk entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_context: Option<String>,
    /// Header breadcrumb of the chunk that scored best for this document.
    /// Empty for non-markdown content or for stores that don't track
    /// section paths (sqlite legacy).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub section_path: Vec<String>,
    /// Which retrievers contributed to this hit. `["dense"]` for non-hybrid
    /// search, `["dense","sparse"]` when hybrid RRF saw the hit on both
    /// sides, `["sparse"]` when only sparse matched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retrievers: Vec<String>,
    /// User-asserted wiki path (e.g. `team/handbook`). `None` when the entry
    /// has no path set. Distinct from chunk-level `section_path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Persisted LLM-on-store analysis (verdicts, tags, doc_type, etc.).
    /// `None` when no analysis has been run yet.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub analysis: Option<Value>,
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

impl From<SearchHit> for SearchResultPayload {
    fn from(value: SearchHit) -> Self {
        Self {
            id: value.id,
            text: value.text,
            metadata: value.metadata,
            source_id: value.source_id,
            created_at: value.created_at,
            updated_at: value.updated_at,
            distance: value.distance,
            chunk_context: None,
            section_path: value.section_path,
            retrievers: value.retrievers,
            path: value.path,
            analysis: value.analysis,
        }
    }
}

pub(crate) async fn store(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    Json(request): Json<StoreRequest>,
) -> Result<(StatusCode, Json<StoreResponse>), ApiError> {
    let response = store_entry_core(&state, request, session.0).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub(crate) async fn search(
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
    let path = match request.path.as_deref() {
        Some(p) => crate::db::normalize_path(p).map_err(|e| ApiError::BadRequest(e.to_string()))?,
        None => None,
    };

    if let Some(ref type_name) = request.type_name {
        let data = request.data.clone().ok_or_else(|| {
            ApiError::BadRequest(format!("type `{type_name}` requires a `data` payload"))
        })?;
        let cache = state.schema_cache.clone();
        let store = state.store.clone();
        let tn = type_name.clone();
        tokio::task::spawn_blocking(move || cache.validate(&tn, &data, store.as_ref()))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(api_validation_error)?;
    } else if request.data.is_some() {
        return Err(ApiError::BadRequest(
            "`data` is only valid when `type` is set".to_string(),
        ));
    }

    let embedder = state.embedder.get_ready()?;
    let store = state.store.clone();
    let created_at = current_timestamp_millis()?;
    let source_id = request.source_id.clone();

    let chunk_ids = if let Some(ref cfg) = request.chunk {
        // Build chunk slices (returns 1 slice if text fits in one chunk).
        let slices = chunking::chunk_document(&request.text, cfg.max_chars, cfg.overlap_chars);
        let n = slices.len();

        if n <= 1 {
            // Text fits in a single chunk — store as a normal entry, no chunking metadata.
            let item = ItemRecord {
                id: id.clone(),
                text: request.text.clone(),
                metadata: request.metadata.clone(),
                source_id: source_id.clone(),
                created_at,
                updated_at: created_at,
                path: path.clone(),
                type_name: request.type_name.clone(),
                data: request.data.clone(),
                analysis: None,
            };
            let embed_text = slices
                .into_iter()
                .next()
                .map(|s| s.embed_text)
                .unwrap_or_else(|| request.text.clone());
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
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
                    updated_at: created_at,
                    path: path.clone(),
                    type_name: None,
                    data: None,
                    analysis: None,
                };
                let embed_text = slice.embed_text;
                let emb = embedder.clone();
                let st = store.clone();
                tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
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
    } else if let Some(md_chunker) = state.md_chunker.clone() {
        // Parent/child path (Postgres + bge-m3): chunk markdown-aware on
        // tokens, embed each chunk, write one document with N chunks.
        let item = ItemRecord {
            id: id.clone(),
            text: request.text.clone(),
            metadata: request.metadata,
            source_id: source_id.clone(),
            created_at,
            updated_at: created_at,
            path: path.clone(),
            type_name: request.type_name.clone(),
            data: request.data.clone(),
            analysis: None,
        };
        let text = request.text.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let chunks = md_chunker.chunks(&text);
            if chunks.is_empty() {
                anyhow::bail!("chunker produced no chunks");
            }
            let mut embedded = Vec::with_capacity(chunks.len());
            for c in chunks {
                let (embedding, sparse) = embedder.embed_both(&c.content)?;
                let sparse = if sparse.is_empty() {
                    None
                } else {
                    Some(sparse)
                };
                embedded.push(crate::db::DocChunk {
                    position: c.position,
                    content: c.content,
                    embedding,
                    section_path: c.section_path,
                    sparse,
                });
            }
            store.upsert_document(item, embedded)?;
            Ok(())
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
        None
    } else {
        // Legacy single-chunk path (SQLite-only mode).
        let item = ItemRecord {
            id: id.clone(),
            text: request.text.clone(),
            metadata: request.metadata,
            source_id: source_id.clone(),
            created_at,
            updated_at: created_at,
            path: path.clone(),
            type_name: request.type_name.clone(),
            data: request.data.clone(),
            analysis: None,
        };
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let embedding = embedder.embed(&item.text)?;
            store.upsert_item(item, &embedding)?;
            Ok(())
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
        None
    };

    let stored_ids: Vec<String> = chunk_ids.as_deref().unwrap_or(&[id.clone()]).to_vec();

    // Best-effort async LLM analysis: contradictions, edges, cluster hint,
    // tags, freshness. Only fires when RAG_ANALYSIS_ENABLED + model are set.
    // Skipped for legacy multi-chunk paths since the parent has no row.
    if chunk_ids.is_none() {
        analysis::spawn_analysis(
            state.clone(),
            id.clone(),
            request.text.clone(),
            source_id.clone(),
        );
    }

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

    invalidate_cms_nodes(state, [id.clone()]).await?;

    Ok(StoreResponse {
        id,
        source_id,
        created_at,
        chunk_ids,
    })
}

pub(crate) async fn invalidate_cms_nodes<I, S>(
    state: &AppState,
    node_ids: I,
) -> Result<(), ApiError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let cms_runtime = state.cms_runtime.clone();
    let ids: Vec<String> = node_ids.into_iter().map(Into::into).collect();
    tokio::task::spawn_blocking(move || cms_runtime.invalidate_nodes(ids))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)
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
    let type_name = request.type_name;
    let max_distance = request.max_distance;
    let now_ms = current_timestamp_millis()?;
    // Reranker is **opt-in**: the cross-encoder adds noticeable latency
    // (seconds on consumer GPUs without tensor cores), so we run it only
    // when the request explicitly asks for it. `RAG_RERANKER_DEFAULT=true`
    // flips the server-side default for callers that don't pass the field.
    let reranker = state.reranker.clone();
    let server_default_rerank = std::env::var("RAG_RERANKER_DEFAULT")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let do_rerank = reranker.is_some() && request.rerank.unwrap_or(server_default_rerank);
    let reranker_top_n = state.reranker_top_n;
    let candidate_top_k = if do_rerank {
        reranker_top_n.max(top_k)
    } else {
        top_k
    };

    let (results, related, raw_embedding) = if query.trim().is_empty() {
        let store = state.store.clone();
        let source_id = source_id.clone();
        let type_name = type_name.clone();
        let top_k = top_k;
        tokio::task::spawn_blocking(
            move || -> anyhow::Result<(Vec<SearchHit>, Vec<(SearchHit, Option<String>)>, Vec<f32>)> {
                let (items, _) = store.list_items(ListItemsRequest {
                    source_id,
                    type_name,
                    limit: Some(top_k),
                    sort_order: SortOrder::Desc,
                    ..Default::default()
                })?;
                let mut hits: Vec<SearchHit> = items.into_iter().map(SearchHit::from).collect();
                // For empty queries, we set retrievers to "recent"
                for hit in &mut hits {
                    hit.retrievers = vec!["recent".to_owned()];
                }
                Ok((hits, Vec::new(), Vec::new()))
            },
        )
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
    } else {
        tokio::task::spawn_blocking(
            move || -> anyhow::Result<(Vec<SearchHit>, Vec<(SearchHit, Option<String>)>, Vec<f32>)> {
                // Hybrid path needs the bge-m3 sparse output too; embed once.
                let (raw, sparse) = if request.hybrid {
                    embedder.embed_both(&query)?
                } else {
                    (embedder.embed(&query)?, Vec::new())
                };
                let embedding = if let Some(ref interest) = interest_embedding {
                    blend_embeddings(&raw, interest, 0.8)
                } else {
                    raw.clone()
                };
                let hits = if request.hybrid {
                    store.search_hybrid(
                        &query,
                        &embedding,
                        &sparse,
                        candidate_top_k,
                        source_id.as_deref(),
                        type_name.as_deref(),
                    )?
                } else {
                    store.search(
                        &embedding,
                        candidate_top_k,
                        source_id.as_deref(),
                        type_name.as_deref(),
                    )?
                };
                let mut filtered: Vec<SearchHit> = hits
                    .into_iter()
                    .filter(|hit| hit.distance <= max_distance)
                    .collect();

                // Cross-encoder reranking. Replaces `distance` with
                // (1 - score) so existing percentage UIs keep working
                // (lower=better; reranker score 1.0 → distance 0.0).
                if do_rerank {
                    if let Some(reranker) = reranker.as_ref() {
                        // Score the matched chunk text when the store
                        // surfaced it (postgres dense/hybrid). Falls back to
                        // the document text for stores that don't chunk so
                        // sqlite-only deployments still rerank.
                        let passages: Vec<&str> = filtered
                            .iter()
                            .map(|h| h.chunk_text.as_deref().unwrap_or(h.text.as_str()))
                            .collect();
                        if !passages.is_empty() {
                            // Rerank can fail under GPU pressure (CUDA OOM in
                            // BiasGelu/MatMul) or model load issues. Falling
                            // back to dense ordering keeps search usable
                            // instead of 500-ing the whole request.
                            let rerank_started = std::time::Instant::now();
                            let max_chars = passages.iter().map(|p| p.len()).max().unwrap_or(0);
                            let total_chars: usize = passages.iter().map(|p| p.len()).sum();
                            match reranker.rerank(&query, &passages) {
                                Ok(scores) => {
                                    tracing::info!(
                                        elapsed_ms = rerank_started.elapsed().as_millis() as u64,
                                        candidates = passages.len(),
                                        max_chars,
                                        total_chars,
                                        "reranker ok"
                                    );
                                    for (hit, score) in filtered.iter_mut().zip(scores.into_iter())
                                    {
                                        hit.distance = 1.0 - score;
                                        if !hit.retrievers.iter().any(|r| r == "rerank") {
                                            hit.retrievers.push("rerank".to_owned());
                                        }
                                    }
                                    filtered.sort_by(|a, b| {
                                        a.distance
                                            .partial_cmp(&b.distance)
                                            .unwrap_or(std::cmp::Ordering::Equal)
                                    });
                                }
                                Err(error) => {
                                    tracing::warn!(
                                        error = %error,
                                        elapsed_ms = rerank_started.elapsed().as_millis() as u64,
                                        candidates = passages.len(),
                                        max_chars,
                                        "reranker failed; falling back to dense ordering"
                                    );
                                }
                            }
                        }
                        filtered.truncate(top_k);
                    }
                }

                // Sort by decay-adjusted score but keep the raw semantic distance for
                // the response so the reported values remain pure vector/BM25 distances.
                filtered.sort_by(|a, b| {
                    decay_sort_key(a, now_ms)
                        .partial_cmp(&decay_sort_key(b, now_ms))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let related = if let Some(top) = filtered.first() {
                    let top_id = top.id.clone();
                    let edges = store
                        .list_graph_edges(Some(&top_id), Some(GraphEdgeType::Manual), None)
                        .ok()
                        .unwrap_or_default();

                    // Identify anti-edges from the top hit (weight < 0.0).
                    let anti_targets: HashSet<String> = edges
                        .iter()
                        .filter(|e| e.weight < 0.0)
                        .map(|e| {
                            if e.from_item_id == top_id {
                                e.to_item_id.clone()
                            } else {
                                e.from_item_id.clone()
                            }
                        })
                        .collect();

                    // If anti-edges exist, penalize those items in the result set and re-sort.
                    if !anti_targets.is_empty() {
                        let mut changed = false;
                        for hit in &mut filtered {
                            if anti_targets.contains(&hit.id) {
                                hit.distance += 0.4; // Substantial penalty
                                if !hit.retrievers.contains(&"penalized".to_owned()) {
                                    hit.retrievers.push("penalized".to_owned());
                                }
                                changed = true;
                            }
                        }
                        if changed {
                            filtered.sort_by(|a, b| {
                                decay_sort_key(a, now_ms)
                                    .partial_cmp(&decay_sort_key(b, now_ms))
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            });
                        }
                    }

                    let existing: HashSet<&str> =
                        filtered.iter().map(|hit| hit.id.as_str()).collect();
                    let mut relations: HashMap<String, Option<String>> = HashMap::new();
                    for edge in edges {
                        // Skip anti-edges for the "Related" panel.
                        if edge.weight < 0.0 {
                            continue;
                        }

                        let neighbor_id = if edge.from_item_id == top_id {
                            edge.to_item_id
                        } else {
                            edge.from_item_id
                        };
                        if neighbor_id == top_id || existing.contains(neighbor_id.as_str()) {
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
        .map_err(ApiError::Internal)?
    };

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

#[derive(Debug, Deserialize)]
pub(crate) struct CountTokensRequest {
    text: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CountTokensResponse {
    token_count: usize,
    char_count: usize,
}

pub(crate) async fn count_tokens(
    State(state): State<AppState>,
    Json(body): Json<CountTokensRequest>,
) -> Result<Json<CountTokensResponse>, ApiError> {
    let embedder = state.embedder.get_ready()?;
    let text = body.text;
    let char_count = text.chars().count();
    let token_count = tokio::task::spawn_blocking(move || embedder.count_tokens(&text))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    Ok(Json(CountTokensResponse {
        token_count,
        char_count,
    }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct RechunkRequest {
    max_chars: Option<usize>,
    overlap_chars: Option<usize>,
}

pub(crate) async fn rechunk_item(
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
    let overlap_chars = body
        .overlap_chars
        .unwrap_or(state.chunking.chunk_overlap_chars);

    let request = StoreRequest {
        id: Some(item.id.clone()),
        text: item.text,
        metadata: item.metadata,
        source_id: item.source_id,
        chunk: Some(ChunkConfig {
            max_chars,
            overlap_chars,
        }),
        path: None,
        type_name: None,
        data: None,
    };
    let response = store_entry_core(&state, request, subject.0).await?;

    // Delete the original unchunked item if chunking produced multiple chunks.
    if response
        .chunk_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty())
    {
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
- Use natural boundaries (paragraphs, sections, list items, functions).\n\
- NEVER split code blocks across chunks. Keep a code block and its immediate context together.\n\
- Aim for natural chunks of roughly 300–800 words where possible.\n\
- Avoid chunks shorter than 3 sentences unless the section is naturally brief.\n\
\n\n\
Respond ONLY with a JSON array of strings, each string being one chunk. No markdown, no explanation.\n\
Example: [\"chunk one text\", \"chunk two text\"]";

#[derive(Debug, Deserialize)]
pub(crate) struct LlmRechunkRequest {
    model: Option<String>,
    max_chunks: Option<usize>,
}

pub(crate) async fn llm_rechunk_item(
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
        .post(format!(
            "{}/chat/completions",
            base_url.trim_end_matches('/')
        ))
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
    struct Resp {
        choices: Vec<Choice>,
    }
    #[derive(serde::Deserialize)]
    struct Choice {
        message: Msg,
    }
    #[derive(serde::Deserialize)]
    struct Msg {
        content: Option<String>,
    }

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
    let parent_path = item.path.clone();
    let now = current_timestamp_millis()?;
    let store = state.store.clone();
    let total = texts.len();

    let chunk_ids: Vec<String> =
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<String>> {
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
                    updated_at: now,
                    path: parent_path.clone(),
                    type_name: None,
                    data: None,
                    analysis: None,
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
1. Write a semantically coherent, self-contained text chunk — preserve all important technical details and context.\n\
2. Assign a source_id category. Use one of: knowledge, reference, notes, code, recipe, \
   medical, finance, travel, or a short lowercase identifier that best fits\n\
3. Extract metadata: title (short descriptive title), topic (main subject), \
   tags (array of relevant keywords)\n\n\
Rules:\n\
- Split into MULTIPLE items only when the text covers truly distinct topics or has major section breaks.\n\
- Each item must be independently useful — no dangling references to other chunks.\n\
- NEVER split a code block into multiple items. A code block should always be stored in its entirety within a single item, along with its immediate explanation.\n\
- Aim for natural, readable chunks (e.g., a full paragraph or a logical sub-section). Chunks should typically be 200-600 words; only use very short chunks if the topic is naturally brief.\n\
- Preserve technical accuracy; never fabricate or add information not present.\n\
- If a URL or author is present in the context, include it in metadata.\n\n\
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

pub(crate) async fn smart_store(
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
        .post(format!(
            "{}/chat/completions",
            base_url.trim_end_matches('/')
        ))
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
    struct Resp {
        choices: Vec<Choice>,
    }
    #[derive(serde::Deserialize)]
    struct Choice {
        message: Msg,
    }
    #[derive(serde::Deserialize)]
    struct Msg {
        content: Option<String>,
    }

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
            path: None,
            type_name: None,
            data: None,
        };
        let resp = store_entry_core(&state, store_req, session.0.clone()).await?;
        responses.push(resp);
    }

    Ok((
        StatusCode::CREATED,
        Json(SmartStoreResponse { items: responses }),
    ))
}

pub(crate) async fn append_to_entry_core(
    state: &AppState,
    id: String,
    text_to_append: String,
) -> Result<StoreResponse, ApiError> {
    let store = state.store.clone();
    let target_id = id.clone();
    let item = tokio::task::spawn_blocking(move || store.get_item(&target_id))
        .await
        .map_err(|e| ApiError::TaskJoin(e))?
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?
        .ok_or_else(|| ApiError::NotFound(format!("item `{id}` not found")))?;

    let mut new_text = item.text.clone();
    if !new_text.ends_with('\n') && !text_to_append.starts_with('\n') {
        new_text.push('\n');
    }
    new_text.push_str(&text_to_append);

    let request = StoreRequest {
        id: Some(id),
        text: new_text,
        metadata: item.metadata,
        source_id: item.source_id,
        chunk: None,
        path: item.path,
        type_name: item.type_name,
        data: item.data,
    };

    store_entry_core(state, request, None).await
}
