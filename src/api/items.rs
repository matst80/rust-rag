use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::db::{
    CategorySummary, GraphEdgeRecord, GraphEdgeType, ItemRecord, ListItemsRequest, SortOrder,
};

use super::analysis::run_analysis;
use super::error::{
    ApiError, api_validation_error, map_missing_item, metadata_schema, validate_metadata,
    validate_source_id,
};
use super::state::AppState;
use super::store_search::invalidate_cms_nodes;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateItemRequest {
    /// New content to embed and store in place of the existing text.
    pub text: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    /// Namespace/category the entry belongs to. See StoreRequest.source_id.
    pub source_id: String,
    /// Optional wiki path. See StoreRequest.path. Pass an empty string to
    /// clear it; omit to leave it unchanged from the existing entry.
    #[serde(default)]
    pub path: Option<String>,
    /// Optional structured-data type name. See StoreRequest.type.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// Typed payload validated against the schema for `type`. Supply only when
    /// updating; omit to leave existing payload unchanged.
    #[serde(default)]
    pub data: Option<Value>,
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
    /// Restrict to entries whose `path` equals this value or sits under it
    /// (e.g. `team` matches `team` itself and `team/handbook`). Normalized
    /// server-side. Comparison is case-insensitive.
    pub path_prefix: Option<String>,
    /// Restrict to entries whose `type` equals this value. See StoreRequest.type.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// `true` restricts to entries with a wiki path set; `false` restricts to
    /// entries with no path (unorganized). Omit for unfiltered.
    pub has_path: Option<bool>,
    /// Only entries without a path, tags, or explicit links.
    pub needs_home: Option<bool>,
    /// When `true`, external graph-hub placeholders (`ext:*` ids,
    /// `source_id == "external"`, `metadata.external == true`) are included.
    /// Default `false`: they carry empty text and are hidden from listings.
    #[serde(default)]
    pub include_external: bool,
    /// Case-insensitive substring search across entry text, id, `source_id`
    /// and wiki `path`. Searches the whole corpus server-side (unlike the
    /// old client-side page filter). Blank is unfiltered.
    pub search: Option<String>,
    /// Importance range over `metadata.importance`. Missing or non-numeric
    /// counts as 0. Powers "important memories" dreaming passes in one query.
    pub min_importance: Option<i32>,
    pub max_importance: Option<i32>,
    /// Any other query parameters are treated as metadata filters (e.g.
    /// `?repo=rust-rag`). Matching is exact per key, but a comma-separated
    /// value matches ANY alternative (`?tags=work,home`), and array metadata
    /// matches when it contains the value (`tags: ["work"]` matches
    /// `?tags=work`).
    #[serde(flatten)]
    pub metadata: HashMap<String, String>,
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
    pub updated_at: i64,
    /// Token count under the embedding tokenizer, untruncated. Populated only
    /// by endpoints that opt in.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token_count: Option<usize>,
    /// User-asserted wiki path (e.g. `team/handbook`). `None` when unset.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub path: Option<String>,
    /// Persisted LLM-on-store analysis (verdicts, tags, doc_type, etc.).
    /// `None` when no analysis has been run yet.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[schemars(schema_with = "metadata_schema")]
    pub analysis: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub analysis_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub analysis_model: Option<String>,
    /// Structured-data type name. References a registered schema. `None`
    /// for untyped legacy entries.
    #[serde(skip_serializing_if = "Option::is_none", default, rename = "type")]
    pub type_name: Option<String>,
    /// Typed payload conforming to the schema for `type`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[schemars(schema_with = "metadata_schema")]
    pub data: Option<Value>,
    /// Contextual expansion: similar or related entries (id + title only).
    /// Populated by `get_entry` to provide immediate navigation context.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub neighbors: Option<Vec<EntryNeighbor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct EntryNeighbor {
    pub id: String,
    pub title: Option<String>,
    pub relationship: Option<String>,
    pub source_type: Option<String>,
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repo: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct AdminItemsResponse {
    pub items: Vec<AdminItemPayload>,
    pub total_count: i64,
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
            updated_at: value.updated_at,
            token_count: None,
            path: value.path,
            analysis: None,
            analysis_at: None,
            analysis_model: None,
            type_name: value.type_name,
            data: value.data,
            neighbors: None,
        }
    }
}

pub(crate) async fn list_categories(
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

pub(crate) async fn list_items(
    State(state): State<AppState>,
    Query(query): Query<ListItemsQuery>,
) -> Result<Json<AdminItemsResponse>, ApiError> {
    if let Some(source_id) = query.source_id.as_deref() {
        validate_source_id(source_id)?;
    }

    let path_prefix = match query.path_prefix.as_deref() {
        Some(p) => crate::db::normalize_path(p).map_err(|e| ApiError::BadRequest(e.to_string()))?,
        None => None,
    };
    let store = state.store.clone();
    let request = ListItemsRequest {
        source_id: query.source_id,
        limit: query.limit,
        offset: query.offset,
        sort_order: query.sort_order.unwrap_or(SortOrder::Desc),
        metadata_filter: query.metadata,
        min_created_at: query.min_created_at,
        max_created_at: query.max_created_at,
        path_prefix,
        type_name: query.type_name,
        has_path: query.has_path,
        needs_home: query.needs_home,
        dreamed: None,
        include_external: query.include_external,
        search: query.search,
        min_importance: query.min_importance,
        max_importance: query.max_importance,
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

/// Query filters for metadata facets. Mirrors the corresponding
/// `list_items` filters so suggestions describe the current view.
#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct MetadataFacetsQuery {
    /// Restrict the scan to a single source_id.
    pub source_id: Option<String>,
    /// Restrict to entries under this wiki path prefix.
    pub path_prefix: Option<String>,
    /// Restrict to entries of this structured-data type.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// Only entries without a path, tags, or explicit links.
    pub needs_home: Option<bool>,
    /// Same substring search as `list_items.search`.
    pub search: Option<String>,
    /// Max entries to scan (default 2000, capped at 5000).
    pub sample: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MetadataFacetValue {
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MetadataFacet {
    pub key: String,
    /// Number of scanned entries carrying this key.
    pub count: i64,
    /// Most frequent values (at most 20), by descending count.
    pub values: Vec<MetadataFacetValue>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MetadataFacetsResponse {
    pub facets: Vec<MetadataFacet>,
    /// Number of entries actually scanned.
    pub scanned: usize,
    /// `true` when the corpus exceeded the scan cap and facets are partial.
    pub truncated: bool,
}

/// Distinct metadata keys + top values over the (filtered) corpus.
///
/// Powers property-filter autocomplete in the entries UI. Pages through
/// `list_items` (no schema change, works on both SQLite and Postgres).
/// Array metadata values contribute each scalar element; nested objects
/// and nulls are skipped. Bounded: at most 100 keys, 20 values per key.
pub(crate) async fn get_metadata_facets(
    State(state): State<AppState>,
    Query(query): Query<MetadataFacetsQuery>,
) -> Result<Json<MetadataFacetsResponse>, ApiError> {
    if let Some(source_id) = query.source_id.as_deref() {
        validate_source_id(source_id)?;
    }
    let path_prefix = match query.path_prefix.as_deref() {
        Some(p) => crate::db::normalize_path(p).map_err(|e| ApiError::BadRequest(e.to_string()))?,
        None => None,
    };
    let scan_cap = query.sample.unwrap_or(2000).clamp(1, 5000);
    let store = state.store.clone();
    let response = tokio::task::spawn_blocking(move || -> anyhow::Result<MetadataFacetsResponse> {
        // key -> (entry count, value -> count)
        let mut keys: HashMap<String, (i64, HashMap<String, i64>)> = HashMap::new();
        let mut scanned = 0usize;
        let mut offset = 0usize;
        const PAGE: usize = 500;
        let mut truncated = false;
        loop {
            if offset >= scan_cap {
                truncated = true;
                break;
            }
            let (page, _) = store.list_items(ListItemsRequest {
                source_id: query.source_id.clone(),
                limit: Some(PAGE),
                offset: Some(offset),
                sort_order: SortOrder::Desc,
                metadata_filter: HashMap::new(),
                min_created_at: None,
                max_created_at: None,
                path_prefix: path_prefix.clone(),
                type_name: query.type_name.clone(),
                has_path: None,
                needs_home: query.needs_home,
                dreamed: None,
                include_external: false,
                search: query.search.clone(),
            
        min_importance: None,
        max_importance: None,
    })?;
            if page.is_empty() {
                break;
            }
            let page_len = page.len();
            scanned += page_len;
            for item in page {
                let Some(obj) = item.metadata.as_object() else {
                    continue;
                };
                for (key, value) in obj {
                    if key == "external" {
                        continue;
                    }
                    let mut flat: Vec<String> = Vec::new();
                    match value {
                        Value::String(s) => {
                            if !s.trim().is_empty() {
                                flat.push(s.clone());
                            }
                        }
                        Value::Number(_) | Value::Bool(_) => {
                            flat.push(value.to_string());
                        }
                        Value::Array(items) => {
                            for v in items {
                                match v {
                                    Value::String(s) => {
                                        if !s.trim().is_empty() {
                                            flat.push(s.clone());
                                        }
                                    }
                                    Value::Number(_) | Value::Bool(_) => {
                                        flat.push(v.to_string());
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    if flat.is_empty() {
                        continue;
                    }
                    let slot = keys.entry(key.clone()).or_insert((0, HashMap::new()));
                    slot.0 += 1;
                    for v in flat {
                        *slot.1.entry(v).or_insert(0) += 1;
                    }
                }
            }
            if page_len < PAGE {
                break;
            }
            offset += PAGE;
        }
        let mut facets: Vec<MetadataFacet> = keys
            .into_iter()
            .map(|(key, (count, values))| {
                let mut values: Vec<MetadataFacetValue> = values
                    .into_iter()
                    .map(|(value, count)| MetadataFacetValue { value, count })
                    .collect();
                values.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
                values.truncate(20);
                MetadataFacet { key, count, values }
            })
            .collect();
        facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
        facets.truncate(100);
        Ok(MetadataFacetsResponse { facets, scanned, truncated })
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(response))
}

pub(crate) async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AdminItemPayload>, ApiError> {
    let store = state.store.clone();
    let id_for_lookup = id.clone();
    let (item, analysis, neighborhood) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let item = store.get_item(&id_for_lookup)?;
            let analysis = store.get_item_analysis(&id_for_lookup)?;
            let neighborhood = store.graph_neighborhood(&id_for_lookup, 1, 10, None).ok();
            Ok((item, analysis, neighborhood))
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    let item = item.ok_or_else(|| ApiError::NotFound("item not found".to_owned()))?;
    let mut payload: AdminItemPayload = item.into();
    if let Some(a) = analysis {
        payload.analysis = Some(a.analysis);
        payload.analysis_at = Some(a.analysis_at);
        payload.analysis_model = Some(a.analysis_model);
    }

    if let Some(nbh) = neighborhood {
        let center_id = id.clone();
        let mut neighbors_with_edges: Vec<(ItemRecord, &GraphEdgeRecord)> = Vec::new();

        for n in nbh.nodes {
            if n.id == center_id {
                continue;
            }

            // Find the "best" edge for this neighbor to determine inclusion and relationship
            let best_edge = nbh.edges.iter().find(|e| {
                let is_connected = (e.from_item_id == n.id && e.to_item_id == center_id)
                    || (e.from_item_id == center_id && e.to_item_id == n.id);
                if !is_connected {
                    return false;
                }

                match e.edge_type {
                    GraphEdgeType::Manual => {
                        let status = e.metadata.get("status").and_then(|v| v.as_str());
                        let confidence = e
                            .metadata
                            .get("confidence")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0);
                        // Include confirmed edges or manual overrides with decent confidence
                        status == Some("confirmed") || (status.is_none() && confidence >= 0.7)
                    }
                    GraphEdgeType::Similarity => {
                        // "really close" threshold (approx distance < 0.25)
                        e.weight >= 0.8
                    }
                }
            });

            if let Some(edge) = best_edge {
                neighbors_with_edges.push((n, edge));
            }
        }

        // Sort: Manual edges first, then Similarity edges (preferring same_repo, then by weight)
        neighbors_with_edges.sort_by(|a, b| {
            let type_a = a.1.edge_type;
            let type_b = b.1.edge_type;
            if type_a != type_b {
                if type_a == GraphEdgeType::Manual {
                    return std::cmp::Ordering::Less;
                } else {
                    return std::cmp::Ordering::Greater;
                }
            }
            if type_a == GraphEdgeType::Manual {
                return a
                    .1
                    .sort_order
                    .cmp(&b.1.sort_order)
                    .then_with(|| a.1.id.cmp(&b.1.id));
            }
            let same_repo_a = a.1.metadata.get("same_repo").and_then(|v| v.as_bool()).unwrap_or(false);
            let same_repo_b = b.1.metadata.get("same_repo").and_then(|v| v.as_bool()).unwrap_or(false);
            if same_repo_a != same_repo_b {
                if same_repo_a {
                    return std::cmp::Ordering::Less;
                } else {
                    return std::cmp::Ordering::Greater;
                }
            }
            b.1.weight
                .partial_cmp(&a.1.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let neighbors: Vec<EntryNeighbor> = neighbors_with_edges
            .into_iter()
            .map(|(n, e)| {
                let source_type = n
                    .metadata
                    .get("source_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned());
                let thumbnail = if source_type.as_deref() == Some("image") {
                    n.metadata
                        .get("source_file")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned())
                } else {
                    None
                };
                let repo = n
                    .data
                    .as_ref()
                    .and_then(|d| d.get("repo").and_then(Value::as_str))
                    .or_else(|| n.metadata.get("repo").and_then(Value::as_str))
                    .or_else(|| n.metadata.get("data").and_then(|d| d.get("repo")).and_then(Value::as_str))
                    .map(|s| s.to_string());

                EntryNeighbor {
                    id: n.id,
                    title: n
                        .metadata
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned())
                        .or_else(|| {
                            n.analysis
                                .as_ref()
                                .and_then(|a| a.get("title"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_owned())
                        }),
                    relationship: e.relation.clone(),
                    source_type,
                    thumbnail,
                    repo,
                }
            })
            .collect();

        if !neighbors.is_empty() {
            payload.neighbors = Some(neighbors);
        }
    }

    Ok(Json(payload))
}

pub(crate) async fn reanalyze_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AdminItemPayload>, ApiError> {
    if !state.analysis.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "analysis not configured".to_owned(),
        ));
    }
    let store = state.store.clone();
    let id_for_lookup = id.clone();
    let item = tokio::task::spawn_blocking(move || store.get_item(&id_for_lookup))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound("item not found".to_owned()))?;

    let neighbor_source = if state.analysis.cross_source {
        None
    } else {
        Some(item.source_id.as_str())
    };
    let analysis = run_analysis(&state, &item.text, neighbor_source, Some(&item.id))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e.to_string())))?;
    let model = state
        .analysis
        .model
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    let json = serde_json::to_string(&analysis)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e.to_string())))?;
    let store = state.store.clone();
    let item_id = item.id.clone();
    let model_for_store = model.clone();
    tokio::task::spawn_blocking(move || {
        store.update_item_analysis(&item_id, &json, &model_for_store)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let mut payload: AdminItemPayload = item.into();
    payload.analysis = Some(serde_json::to_value(&analysis).unwrap_or(Value::Null));
    payload.analysis_at = Some(now);
    payload.analysis_model = Some(model);
    Ok(Json(payload))
}

pub(crate) async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateItemRequest>,
) -> Result<Json<AdminItemPayload>, ApiError> {
    validate_metadata(&request.metadata)?;
    validate_source_id(&request.source_id)?;
    let path_override = match request.path.as_deref() {
        Some(p) => {
            Some(crate::db::normalize_path(p).map_err(|e| ApiError::BadRequest(e.to_string()))?)
        }
        None => None,
    };

    // Validate typed-data before touching the store. When `data` is supplied
    // alongside a `type`, it must satisfy that type's schema. When only
    // `type` is supplied (no `data`), require a payload — type without data
    // is meaningless.
    if let Some(ref type_name) = request.type_name {
        if let Some(data) = request.data.clone() {
            let cache = state.schema_cache.clone();
            let store = state.store.clone();
            let tn = type_name.clone();
            tokio::task::spawn_blocking(move || cache.validate(&tn, &data, store.as_ref()))
                .await
                .map_err(ApiError::TaskJoin)?
                .map_err(api_validation_error)?;
        }
    } else if request.data.is_some() {
        return Err(ApiError::BadRequest(
            "`data` is only valid when `type` is set".to_string(),
        ));
    }

    let embedder = state.embedder.get_ready()?;
    let store = state.store.clone();
    let type_override = request.type_name.clone();
    let data_override = request.data.clone();

    let updated = tokio::task::spawn_blocking(move || -> anyhow::Result<ItemRecord> {
        let existing = store
            .get_item(&id)?
            .ok_or_else(|| anyhow::anyhow!("item {id} not found"))?;
        let new_path = match path_override {
            Some(p) => p,
            None => existing.path.clone(),
        };
        let item = ItemRecord {
            id: existing.id,
            text: request.text,
            metadata: request.metadata,
            source_id: request.source_id,
            created_at: existing.created_at,
            updated_at: Utc::now().timestamp_millis(),
            path: new_path,
            type_name: type_override.or(existing.type_name),
            data: data_override.or(existing.data),
            analysis: existing.analysis,
            dreamed_at: None,
        };
        let embedding = embedder.embed(&item.text)?;
        store.upsert_item(item.clone(), &embedding)?;
        Ok(item)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(|error| map_missing_item("item", error))?;

    invalidate_cms_nodes(&state, [updated.id.clone()]).await?;

    super::analysis::spawn_analysis(
        state.clone(),
        updated.id.clone(),
        updated.text.clone(),
        updated.source_id.clone(),
    );

    Ok(Json(updated.into()))
}

/// Bulk-delete up to 200 entries in one call. Returns per-id results so
/// the caller can report partial success; unknown ids land in `not_found`
/// instead of failing the whole batch.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BulkDeleteRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BulkDeleteResponse {
    pub deleted: Vec<String>,
    pub not_found: Vec<String>,
}

pub(crate) async fn bulk_delete_items(
    State(state): State<AppState>,
    Json(payload): Json<BulkDeleteRequest>,
) -> Result<Json<BulkDeleteResponse>, ApiError> {
    if payload.ids.is_empty() {
        return Err(ApiError::BadRequest("ids must not be empty".to_owned()));
    }
    if payload.ids.len() > 200 {
        return Err(ApiError::BadRequest(
            "bulk delete is limited to 200 ids per request".to_owned(),
        ));
    }
    // De-dupe while preserving order so repeats don't double-report.
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<String> = payload
        .ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect();
    invalidate_cms_nodes(&state, ids.clone()).await?;
    let store = state.store.clone();
    let (deleted, not_found) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<String>, Vec<String>)> {
            let mut deleted = Vec::new();
            let mut not_found = Vec::new();
            for id in &ids {
                if store.delete_item(id)? {
                    deleted.push(id.clone());
                } else {
                    not_found.push(id.clone());
                }
            }
            Ok((deleted, not_found))
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    Ok(Json(BulkDeleteResponse { deleted, not_found }))
}

pub(crate) async fn delete_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<super::store_search::DeleteResponse>, ApiError> {
    invalidate_cms_nodes(&state, [id.clone()]).await?;
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

    Ok(Json(super::store_search::DeleteResponse { id, deleted }))
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateShareRequest {
    /// Optional expiration duration in seconds from now, or timestamp in millis
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareResponse {
    pub token: String,
    pub item_id: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub url: String,
}

pub(crate) async fn create_item_share(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Option<CreateShareRequest>>,
) -> Result<Json<ShareResponse>, ApiError> {
    let store = state.store.clone();
    let id_for_lookup = id.clone();
    
    // Ensure item exists
    let exists = tokio::task::spawn_blocking({
        let store = store.clone();
        let id = id_for_lookup.clone();
        move || store.get_item(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    if exists.is_none() {
        return Err(ApiError::NotFound(format!("item {id} not found")));
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    let expires_at = payload
        .and_then(|p| p.expires_in_seconds)
        .map(|sec| chrono::Utc::now().timestamp_millis() + sec * 1000);

    let share = tokio::task::spawn_blocking({
        let store = store.clone();
        let id = id.clone();
        let token = token.clone();
        move || store.create_public_share(&id, &token, expires_at)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(ShareResponse {
        token: share.token.clone(),
        item_id: share.item_id,
        created_at: share.created_at,
        expires_at: share.expires_at,
        url: format!("/public/{}", share.token),
    }))
}

pub(crate) async fn get_item_share(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Option<ShareResponse>>, ApiError> {
    let store = state.store.clone();
    let share = tokio::task::spawn_blocking({
        let store = store.clone();
        let id = id.clone();
        move || store.get_public_share_for_item(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(share.map(|s| ShareResponse {
        token: s.token.clone(),
        item_id: s.item_id,
        created_at: s.created_at,
        expires_at: s.expires_at,
        url: format!("/public/{}", s.token),
    })))
}

pub(crate) async fn revoke_item_share(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<super::store_search::DeleteResponse>, ApiError> {
    let store = state.store.clone();
    let deleted = tokio::task::spawn_blocking({
        let store = store.clone();
        let id = id.clone();
        move || store.revoke_public_shares_for_item(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(super::store_search::DeleteResponse { id, deleted }))
}

/// One entry whose text references `target_id` via an Obsidian-style
/// wikilink (`[[id]]`, `[[id|Label]]`, `[[Title]]`) or an internal
/// `/entries/{id}` markdown link. Returned by `GET /api/items/{id}/backlinks`.
#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct BacklinkHit {
    pub id: String,
    pub title: String,
    pub source_id: String,
    pub updated_at: i64,
    /// How the reference looked in the source text (`[[...]]` or `/entries/...`).
    pub matched_ref: String,
    /// ~160-char window around the first match, for context in the UI.
    pub context: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct BacklinksResponse {
    pub id: String,
    pub title: String,
    pub backlinks: Vec<BacklinkHit>,
}

fn backlink_context(text: &str, match_start: usize, match_len: usize) -> String {
    const RADIUS: usize = 80;
    let start = match_start.saturating_sub(RADIUS);
    let end = (match_start + match_len + RADIUS).min(text.len());
    // Snap to char boundaries (text may contain multi-byte chars).
    let mut s = start;
    while s < match_start && !text.is_char_boundary(s) {
        s += 1;
    }
    let mut e = end;
    while e > match_start + match_len && !text.is_char_boundary(e) {
        e -= 1;
    }
    let mut snippet: String = text.get(s..e).unwrap_or("").replace('\n', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    if snippet.len() > 240 {
        snippet.truncate(240);
    }
    if s > 0 {
        snippet = format!("…{snippet}");
    }
    if e < text.len() {
        snippet = format!("{snippet}…");
    }
    snippet
}

/// Find the first wikilink-style reference to `id` (exact) or `title`
/// (case-insensitive) inside `text`. Returns (byte_offset, match_len,
/// matched_ref). Skips fenced code blocks and inline code spans so examples
/// don't count as backlinks — mirroring the frontend `splitWikilinks` rule.
fn find_wikilink_ref(text: &str, id: &str, title_lower: &str) -> Option<(usize, usize, String)> {
    // Mask fenced code blocks and inline code spans with spaces (same byte
    // length) so example wikilinks in code don't count — mirroring the
    // frontend splitWikilinks rule — while keeping byte offsets aligned
    // with the original text.
    let bytes = text.as_bytes();
    let mut masked = bytes.to_vec();
    let mut i = 0usize;
    while i < masked.len() {
        // Fenced block starting with ``` (must be at line start or string start).
        let at_line_start = i == 0 || masked[i - 1] == b'\n';
        if at_line_start && masked[i..].starts_with(b"```") {
            let mut j = i + 3;
            let mut closed = false;
            while j < masked.len() {
                let js = j == 0 || masked[j - 1] == b'\n';
                if js && masked[j..].starts_with(b"```") {
                    for k in i..(j + 3).min(masked.len()) {
                        masked[k] = b' ';
                    }
                    i = (j + 3).min(masked.len());
                    closed = true;
                    break;
                }
                j += 1;
            }
            if !closed {
                for k in i..masked.len() {
                    masked[k] = b' ';
                }
                break;
            }
            continue;
        }
        // Inline code span `...` (single backtick, same line).
        if masked[i] == b'`' {
            let mut j = i + 1;
            while j < masked.len() && masked[j] != b'`' && masked[j] != b'\n' {
                j += 1;
            }
            if j < masked.len() && masked[j] == b'`' {
                for k in i..=j {
                    masked[k] = b' ';
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    let searchable = String::from_utf8_lossy(&masked).into_owned();
    let open_tag = "\u{5b}\u{5b}";
    let close_tag = "\u{5d}\u{5d}";
    let mut cursor = 0usize;
    while let Some(open_rel) = searchable[cursor..].find(open_tag) {
        let open = cursor + open_rel;
        let after_open = &searchable[open + 2..];
        if let Some(close_rel) = after_open.find(close_tag) {
            let inner = &after_open[..close_rel];
            let target = inner.split('|').next().unwrap_or("").trim();
            let matched = target == id
                || (!title_lower.is_empty() && target.to_lowercase() == title_lower);
            let match_len = inner.len() + 4;
            if matched {
                let matched_ref = text.get(open..open + match_len).unwrap_or("").to_owned();
                return Some((open, match_len, matched_ref));
            }
            cursor = open + 2 + close_rel + 2;
        } else {
            break;
        }
    }
    // `/entries/{id}` markdown links also count as backlinks.
    let entry_link = format!("/entries/{id}");
    if let Some(rel) = searchable.find(entry_link.as_str()) {
        return Some((rel, entry_link.len(), entry_link));
    }
    None
}

/// Backlinks for one entry: every other entry whose text contains a
/// wikilink reference to it. Manual graph edges already surface in
/// `GET /api/graph/edges?item_id=…`; this endpoint covers the much more
/// common case where authors just write `[[id]]` / `[[Title]]` inline and
/// never create an explicit edge.
///
/// Implementation pages through `list_items` (no schema change, works on
/// both SQLite and Postgres) and matches in Rust. Bounded at 5,000 scanned
/// items / 200 returned hits — plenty for a second-brain corpus while
/// keeping the request cheap.
pub(crate) async fn get_backlinks(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BacklinksResponse>, ApiError> {
    let store = state.store.clone();
    let target_id = id.clone();
    let response = tokio::task::spawn_blocking(move || -> anyhow::Result<BacklinksResponse> {
        let target = store.get_item(&target_id)?;
        let title = match &target {
            Some(item) => super::harness::display_title(item),
            None => target_id.clone(),
        };
        let title_lower = title.to_lowercase();
        // Don't title-match when the title is just the id itself — every
        // `[[id]]` already matches exactly, and fuzzy would add nothing.
        let title_lower = if title_lower == target_id.to_lowercase() {
            String::new()
        } else {
            title_lower
        };

        let mut backlinks = Vec::new();
        let mut offset = 0usize;
        const PAGE: usize = 500;
        const SCAN_CAP: usize = 5000;
        const HIT_CAP: usize = 200;
        while offset < SCAN_CAP && backlinks.len() < HIT_CAP {
            let (page, _) = store.list_items(ListItemsRequest {
                source_id: None,
                limit: Some(PAGE),
                offset: Some(offset),
                sort_order: SortOrder::Desc,
                metadata_filter: HashMap::new(),
                min_created_at: None,
                max_created_at: None,
                path_prefix: None,
                type_name: None,
                has_path: None,
                needs_home: None,
                dreamed: None,
                include_external: false,
                search: None,
            
        min_importance: None,
        max_importance: None,
    })?;
            if page.is_empty() {
                break;
            }
            let page_len = page.len();
            for item in page {
                if item.id == target_id || backlinks.len() >= HIT_CAP {
                    continue;
                }
                if item.id.starts_with("ext:") {
                    continue;
                }
                if let Some((start, len, matched_ref)) =
                    find_wikilink_ref(&item.text, &target_id, &title_lower)
                {
                    backlinks.push(BacklinkHit {
                        id: item.id.clone(),
                        title: super::harness::display_title(&item),
                        source_id: item.source_id.clone(),
                        updated_at: item.updated_at,
                        matched_ref,
                        context: backlink_context(&item.text, start, len),
                    });
                }
            }
            if page_len < PAGE {
                break;
            }
            offset += page_len;
        }
        backlinks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(BacklinksResponse {
            id: target_id,
            title,
            backlinks,
        })
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;
    Ok(Json(response))
}

/// One member of a duplicate-title group.
#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct DuplicateTitleItem {
    pub id: String,
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub path: Option<String>,
    pub updated_at: i64,
}

/// Entries sharing one display title (case-insensitive). `all_extracted`
/// is true when no member carries an explicit `metadata.title` — i.e. every
/// title was derived from the body text (usually the first `#` heading),
/// which is the collision case that looks terrible in lists.
#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct DuplicateTitleGroup {
    pub title: String,
    pub count: usize,
    pub all_extracted: bool,
    pub items: Vec<DuplicateTitleItem>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct DuplicateTitlesResponse {
    pub groups: Vec<DuplicateTitleGroup>,
    pub scanned: usize,
}

fn header_junk_line(line: &str) -> bool {
    if line.len() < 4 {
        return true;
    }
    for prefix in ["```", "~~~", "<!--", "---"] {
        if line.starts_with(prefix) {
            return true;
        }
    }
    if line.starts_with('|') || line.starts_with('%') {
        return true;
    }
    for prefix in ["/", "~/", "../", "./", "http://", "https://", "www."] {
        if line.starts_with(prefix) {
            return true;
        }
    }
    // `Key: value` header lines (`REPO: …`, `Source: …`, `Path: …`).
    if let Some(colon) = line.find(": ") {
        let (key, _) = line.split_at(colon);
        if !key.is_empty()
            && key.len() <= 25
            && key.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " _./\\-".contains(c))
        {
            return true;
        }
    }
    false
}

/// Mirror of the frontend `entryTitle` rule: explicit `metadata.title` wins;
/// else a `#`-`####` heading (6+ chars) within the first 4 non-empty lines;
/// else the first line that reads like a title; else the id. Returns
/// (title, extracted) where extracted is false only for explicit titles.
fn frontend_style_title(text: &str, metadata: &Value, id: &str) -> (String, bool) {
    if let Some(t) = metadata.get("title").and_then(Value::as_str) {
        let t = t.trim();
        if !t.is_empty() {
            return (t.to_owned(), false);
        }
    }
    let non_empty: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).take(8).collect();
    for line in non_empty.iter().take(4) {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if (1..=4).contains(&hashes) {
            let rest = line[hashes..].trim_start();
            // Require whitespace after the hashes and 6+ chars of title.
            if line[hashes..].starts_with(' ') && rest.chars().count() >= 6 {
                return (rest.to_owned(), true);
            }
        }
    }
    for line in &non_empty {
        if header_junk_line(line) {
            continue;
        }
        let stripped = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).unwrap_or(line);
        return (stripped.to_owned(), true);
    }
    (non_empty.into_iter().next().unwrap_or(id).to_owned(), true)
}

/// Corpus-wide duplicate display titles. Pages through `list_items` (no
/// schema change; SQLite + Postgres) and groups by case-insensitive title
/// using the same rule the frontend renders. Bounded: 5,000 scanned items,
/// 200 returned groups, 10 member ids per group.
pub(crate) async fn get_duplicate_titles(
    State(state): State<AppState>,
) -> Result<Json<DuplicateTitlesResponse>, ApiError> {
    let store = state.store.clone();
    let response = tokio::task::spawn_blocking(move || -> anyhow::Result<DuplicateTitlesResponse> {
        let mut groups: HashMap<String, (String, Vec<(DuplicateTitleItem, bool)>)> = HashMap::new();
        let mut offset = 0usize;
        let mut scanned = 0usize;
        const PAGE: usize = 500;
        const SCAN_CAP: usize = 5000;
        loop {
            if offset >= SCAN_CAP {
                break;
            }
            let (page, _) = store.list_items(ListItemsRequest {
                source_id: None,
                limit: Some(PAGE),
                offset: Some(offset),
                sort_order: SortOrder::Desc,
                metadata_filter: HashMap::new(),
                min_created_at: None,
                max_created_at: None,
                path_prefix: None,
                type_name: None,
                has_path: None,
                needs_home: None,
                dreamed: None,
                include_external: false,
                search: None,
            
        min_importance: None,
        max_importance: None,
    })?;
            if page.is_empty() {
                break;
            }
            let page_len = page.len();
            scanned += page_len;
            for item in page {
                if item.id.starts_with("ext:") {
                    continue;
                }
                let (title, extracted) = frontend_style_title(&item.text, &item.metadata, &item.id);
                if title == item.id {
                    continue;
                }
                let key = title.to_lowercase();
                groups
                    .entry(key)
                    .or_insert_with(|| (title, Vec::new()))
                    .1
                    .push((
                        DuplicateTitleItem {
                            id: item.id,
                            source_id: item.source_id,
                            path: item.path,
                            updated_at: item.updated_at,
                        },
                        extracted,
                    ));
            }
            if page_len < PAGE {
                break;
            }
            offset += page_len;
        }
        let mut out: Vec<DuplicateTitleGroup> = groups
            .into_values()
            .filter(|(_, members)| members.len() > 1)
            .map(|(title, members)| {
                let all_extracted = members.iter().all(|(_, e)| *e);
                let mut items: Vec<DuplicateTitleItem> =
                    members.into_iter().map(|(m, _)| m).collect();
                items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                let count = items.len();
                items.truncate(10);
                DuplicateTitleGroup { title, count, all_extracted, items }
            })
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.title.cmp(&b.title)));
        out.truncate(200);
        Ok(DuplicateTitlesResponse { groups: out, scanned })
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;
    Ok(Json(response))
}

/// Unauthenticated public entry view
pub(crate) async fn get_public_entry(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<AdminItemPayload>, ApiError> {
    let store = state.store.clone();
    let token_clone = token.clone();
    let (item, analysis) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let share = store.get_public_share(&token_clone)?;
        let Some(share) = share else {
            return Ok((None, None));
        };
        let item = store.get_item(&share.item_id)?;
        let analysis = store.get_item_analysis(&share.item_id)?;
        Ok((item, analysis))
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    let item = item.ok_or_else(|| ApiError::NotFound("Shared entry not found or link has expired".to_owned()))?;
    let mut payload: AdminItemPayload = item.into();
    if let Some(a) = analysis {
        payload.analysis = Some(a.analysis);
        payload.analysis_at = Some(a.analysis_at);
        payload.analysis_model = Some(a.analysis_model);
    }
    Ok(Json(payload))
}
