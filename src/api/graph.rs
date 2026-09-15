use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

use std::collections::HashMap;

use crate::db::{
    DuplicateEdgeGroup, GraphEdgeRecord, GraphEdgeType, GraphNeighborhood, GraphNodeDistance,
    GraphStatus, ManualEdgeInput, VectorStore,
};

use super::error::{
    ApiError, default_metadata, map_graph_error, metadata_schema, validate_graph_depth,
    validate_graph_limit, validate_metadata, validate_non_empty,
};
use super::harness::display_title;
use super::items::AdminItemPayload;
use super::state::AppState;
use super::store_search::{DeleteResponse, invalidate_cms_nodes};

/// Look up display titles for every distinct item id referenced by `edges`.
/// Best-effort: ids that no longer resolve to an item are simply omitted.
pub(crate) fn titles_for_edges(
    store: &dyn VectorStore,
    edges: &[GraphEdgeRecord],
) -> HashMap<String, String> {
    let mut ids: Vec<&str> = edges
        .iter()
        .flat_map(|e| [e.from_item_id.as_str(), e.to_item_id.as_str()])
        .collect();
    ids.sort_unstable();
    ids.dedup();

    ids.into_iter()
        .filter_map(|id| {
            let item = store.get_item(id).ok().flatten()?;
            Some((id.to_owned(), display_title(&item)))
        })
        .collect()
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
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateManualEdgeRequest {
    pub from_item_id: String,
    pub to_item_id: String,
    pub relation: Option<String>,
    pub sort_order: Option<String>,
    pub weight: Option<f32>,
    pub directed: Option<bool>,
    #[serde(default = "default_metadata")]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateGraphEdgeRequest {
    pub relation: Option<String>,
    pub sort_order: Option<String>,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
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
    pub sort_order: String,
    pub weight: f32,
    pub directed: bool,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
    /// Extracted display title of `from_item_id`'s entry, when it still resolves.
    pub from_title: Option<String>,
    /// Extracted display title of `to_item_id`'s entry, when it still resolves.
    pub to_title: Option<String>,
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

pub(crate) fn edge_payload(value: GraphEdgeRecord, titles: &HashMap<String, String>) -> GraphEdgePayload {
    let from_title = titles.get(&value.from_item_id).cloned();
    let to_title = titles.get(&value.to_item_id).cloned();
    GraphEdgePayload {
        id: value.id,
        from_item_id: value.from_item_id,
        to_item_id: value.to_item_id,
        edge_type: value.edge_type,
        relation: value.relation,
        sort_order: value.sort_order,
        weight: value.weight,
        directed: value.directed,
        metadata: value.metadata,
        created_at: value.created_at,
        updated_at: value.updated_at,
        from_title,
        to_title,
    }
}

impl From<GraphNeighborhood> for GraphNeighborhoodResponse {
    fn from(value: GraphNeighborhood) -> Self {
        // Neighborhood nodes are already fully loaded, so titles can be
        // derived from them directly without another store round-trip.
        let titles: HashMap<String, String> = value
            .nodes
            .iter()
            .map(|item| (item.id.clone(), display_title(item)))
            .collect();
        Self {
            center_id: value.center_id,
            nodes: value.nodes.into_iter().map(Into::into).collect(),
            edges: value
                .edges
                .into_iter()
                .map(|e| edge_payload(e, &titles))
                .collect(),
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

pub(crate) async fn graph_status(
    State(state): State<AppState>,
) -> Result<Json<GraphStatusResponse>, ApiError> {
    let store = state.store.clone();
    let status = tokio::task::spawn_blocking(move || store.graph_status())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(status.into()))
}

pub(crate) async fn list_graph_edges(
    State(state): State<AppState>,
    Query(query): Query<ListGraphEdgesQuery>,
) -> Result<Json<GraphEdgesResponse>, ApiError> {
    let store = state.store.clone();
    let item_id = query.item_id;
    let edge_type = query.edge_type;

    let status = query.status;

    let edges = tokio::task::spawn_blocking(move || {
        let edges = store.list_graph_edges(item_id.as_deref(), edge_type, status.as_deref())?;
        let titles = titles_for_edges(store.as_ref(), &edges);
        Ok::<_, anyhow::Error>(
            edges
                .into_iter()
                .map(|e| edge_payload(e, &titles))
                .collect::<Vec<_>>(),
        )
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(map_graph_error)?;

    Ok(Json(GraphEdgesResponse { edges }))
}

pub(crate) async fn graph_neighborhood(
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

pub(crate) async fn rebuild_graph(
    State(state): State<AppState>,
) -> Result<Json<GraphRebuildResponse>, ApiError> {
    let store = state.store.clone();
    let rebuilt_edges = tokio::task::spawn_blocking(move || store.rebuild_similarity_graph())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(map_graph_error)?;

    Ok(Json(GraphRebuildResponse { rebuilt_edges }))
}

#[tracing::instrument(name = "api.graph.list_duplicates", skip(state))]
pub(crate) async fn list_duplicate_edges(
    State(state): State<AppState>,
) -> Result<Json<Vec<DuplicateEdgeGroup>>, ApiError> {
    let store = state.store.clone();
    let groups = tokio::task::spawn_blocking(move || store.list_duplicate_edges())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(map_graph_error)?;

    Ok(Json(groups))
}

pub(crate) async fn create_manual_edge(
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
        relation: request.relation.map(Cow::Owned),
        sort_order: request.sort_order,
        weight: request.weight.unwrap_or(1.0),
        directed: request.directed.unwrap_or(false),
        metadata: request.metadata,
    };

    let edge = tokio::task::spawn_blocking(move || {
        let edge = store.add_manual_edge(input)?;
        let titles = titles_for_edges(store.as_ref(), std::slice::from_ref(&edge));
        Ok::<_, anyhow::Error>(edge_payload(edge, &titles))
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(map_graph_error)?;

    invalidate_cms_nodes(&state, [edge.from_item_id.clone(), edge.to_item_id.clone()]).await?;

    Ok((StatusCode::CREATED, Json(edge)))
}

pub(crate) async fn update_graph_edge(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateGraphEdgeRequest>,
) -> Result<Json<GraphEdgePayload>, ApiError> {
    validate_metadata(&request.metadata)?;

    let store = state.store.clone();
    let edge = tokio::task::spawn_blocking(move || {
        let edge =
            store.update_graph_edge(&id, request.relation, request.metadata, request.sort_order)?;
        let titles = titles_for_edges(store.as_ref(), std::slice::from_ref(&edge));
        Ok::<_, anyhow::Error>(edge_payload(edge, &titles))
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(map_graph_error)?;

    invalidate_cms_nodes(&state, [edge.from_item_id.clone(), edge.to_item_id.clone()]).await?;

    Ok(Json(edge))
}

pub(crate) async fn delete_graph_edge(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, ApiError> {
    let store = state.store.clone();
    let cms_runtime = state.cms_runtime.clone();
    let edge_before_delete = tokio::task::spawn_blocking({
        let store = store.clone();
        let id = id.clone();
        move || store.get_graph_edge(&id)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(map_graph_error)?;
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

    if let Some(edge) = edge_before_delete {
        tokio::task::spawn_blocking(move || {
            cms_runtime.invalidate_nodes([edge.from_item_id, edge.to_item_id])
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    }

    Ok(Json(DeleteResponse { id, deleted }))
}
