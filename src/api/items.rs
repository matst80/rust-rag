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
    /// Any other query parameters are treated as metadata filters (e.g. ?todo=mats)
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

pub(crate) async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AdminItemPayload>, ApiError> {
    let store = state.store.clone();
    let id_for_lookup = id.clone();
    let (item, analysis, neighborhood) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
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

        // Sort: Manual edges first, then Similarity edges (by weight)
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
        };
        let embedding = embedder.embed(&item.text)?;
        store.upsert_item(item.clone(), &embedding)?;
        Ok(item)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(|error| map_missing_item("item", error))?;

    invalidate_cms_nodes(&state, [updated.id.clone()]).await?;

    Ok(Json(updated.into()))
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
