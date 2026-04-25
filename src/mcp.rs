//! In-process Model Context Protocol server.
//!
//! This mounts the same tool surface as the `mcp-stdio` bridge, but talks to
//! the store and embedder directly instead of round-tripping through HTTP.
//! The `StreamableHttpService` service is nested into the main axum router at
//! `/mcp`, gated by the same bearer-token middleware that protects every
//! other write path.

use crate::{
    api::{
        AdminItemPayload, AdminItemsResponse, AppState, CategoriesResponse,
        CreateManualEdgeRequest, DeleteResponse, GraphEdgePayload, GraphEdgesResponse,
        GraphNeighborhoodQuery, GraphNeighborhoodResponse, GraphRebuildResponse,
        GraphStatusResponse, HealthResponse, ListGraphEdgesQuery, ListItemsQuery, SearchRequest,
        SearchResponse, SearchResultPayload, StoreRequest, StoreResponse, UpdateItemRequest,
        metadata_schema, search_core, store_entry_core,
    },
    db::{GraphEdgeType, ItemRecord, ListItemsRequest, ManualEdgeInput, SortOrder},
};
use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt::Write as _, sync::Arc, time::Duration};

const SERVER_NAME: &str = "rust-rag";
const SERVER_INSTRUCTIONS: &str = "This server exposes the rust-rag retrieval store directly. \
Entries are grouped by a user-defined `source_id` (a short lowercase namespace such as \"memory\", \"knowledge\", or \"notes\"). \
Use `search_entries` for semantic retrieval, `store_entry` to add content, admin tools to manage items, and graph tools for manual links.";

#[derive(Clone)]
pub struct RustRagMcpServer {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl RustRagMcpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RustRagMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                SERVER_NAME.to_owned(),
                env!("CARGO_PKG_VERSION").to_owned(),
            ))
            .with_instructions(SERVER_INSTRUCTIONS.to_owned())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateItemParams {
    pub id: String,
    pub text: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: serde_json::Value,
    pub source_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GraphNeighborhoodParams {
    pub id: String,
    pub depth: Option<usize>,
    pub limit: Option<usize>,
    pub edge_type: Option<GraphEdgeType>,
}

#[tool_router(router = tool_router)]
impl RustRagMcpServer {
    #[tool(description = "Return rust-rag service health and embedder readiness.")]
    async fn health_status(&self) -> Result<Json<HealthResponse>, String> {
        let (_, body) = self.state.embedder.health();
        Ok(Json(body.0))
    }

    #[tool(description = "Store a text entry with metadata and source_id in rust-rag.")]
    async fn store_entry(
        &self,
        Parameters(request): Parameters<StoreRequest>,
    ) -> Result<Json<StoreResponse>, String> {
        store_entry_core(&self.state, request, None)
            .await
            .map(Json)
            .map_err(stringify_api_error)
    }

    #[tool(
        description = "Run semantic search against stored entries. Returns ranked vector hits plus `related` items that the user manually linked from the top hit (not just vector-similar)."
    )]
    async fn search_entries(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, String> {
        let query = request.query.clone();
        let response = search_core(&self.state, request, None)
            .await
            .map_err(stringify_api_error)?;
        Ok(format_search_result(&response, &query))
    }

    #[tool(description = "Fetch a single stored entry by its id.")]
    async fn get_entry(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<AdminItemPayload>, String> {
        let store = self.state.store.clone();
        tokio::task::spawn_blocking(move || store.get_item(&id))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?
            .map(|record| Json(record.into()))
            .ok_or_else(|| "item not found".to_owned())
    }

    #[tool(description = "List all source_id categories and their item counts.")]
    async fn list_categories(&self) -> Result<Json<CategoriesResponse>, String> {
        let store = self.state.store.clone();
        let categories = tokio::task::spawn_blocking(move || store.list_categories())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(CategoriesResponse {
            categories: categories.into_iter().map(Into::into).collect(),
        }))
    }

    #[tool(description = "List items, optionally filtered by source_id.")]
    async fn list_items(
        &self,
        Parameters(query): Parameters<ListItemsQuery>,
    ) -> Result<Json<AdminItemsResponse>, String> {
        let store = self.state.store.clone();
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
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(AdminItemsResponse {
            items: items.into_iter().map(Into::into).collect(),
            total_count,
        }))
    }

    #[tool(description = "Update an existing item by id.")]
    async fn update_item(
        &self,
        Parameters(params): Parameters<UpdateItemParams>,
    ) -> Result<Json<AdminItemPayload>, String> {
        let id = params.id.clone();
        let request = UpdateItemRequest {
            text: params.text,
            metadata: params.metadata,
            source_id: params.source_id,
        };
        let embedder = self
            .state
            .embedder
            .get_ready()
            .map_err(stringify_api_error)?;
        let store = self.state.store.clone();

        tokio::task::spawn_blocking(move || -> anyhow::Result<ItemRecord> {
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
        .map_err(|error| error.to_string())?
        .map(|record| Json(record.into()))
        .map_err(|error| error.to_string())
    }

    #[tool(description = "Delete an item by id.")]
    async fn delete_item(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        let store = self.state.store.clone();
        let target_id = id.clone();
        let deleted = tokio::task::spawn_blocking(move || store.delete_item(&target_id))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        if !deleted {
            return Err(format!("item {id} not found"));
        }
        Ok(Json(DeleteResponse { id, deleted }))
    }

    #[tool(description = "Return current graph configuration and edge counts.")]
    async fn graph_status(&self) -> Result<Json<GraphStatusResponse>, String> {
        let store = self.state.store.clone();
        let status = tokio::task::spawn_blocking(move || store.graph_status())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(status.into()))
    }

    #[tool(description = "List graph edges, optionally filtered by item_id or edge type.")]
    async fn list_graph_edges(
        &self,
        Parameters(query): Parameters<ListGraphEdgesQuery>,
    ) -> Result<Json<GraphEdgesResponse>, String> {
        let store = self.state.store.clone();
        let edges = tokio::task::spawn_blocking(move || {
            store.list_graph_edges(query.item_id.as_deref(), query.edge_type)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        Ok(Json(GraphEdgesResponse {
            edges: edges.into_iter().map(Into::into).collect(),
        }))
    }

    #[tool(description = "Return the graph neighborhood around a center item id.")]
    async fn graph_neighborhood(
        &self,
        Parameters(params): Parameters<GraphNeighborhoodParams>,
    ) -> Result<Json<GraphNeighborhoodResponse>, String> {
        let store = self.state.store.clone();
        let GraphNeighborhoodParams {
            id,
            depth,
            limit,
            edge_type,
        } = params;
        let query = GraphNeighborhoodQuery {
            depth,
            limit,
            edge_type,
        };
        let depth = query.depth.unwrap_or(1);
        let limit = query.limit.unwrap_or(100);
        let edge_type = query.edge_type;
        let neighborhood = tokio::task::spawn_blocking(move || {
            store.graph_neighborhood(&id, depth, limit, edge_type)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        Ok(Json(neighborhood.into()))
    }

    #[tool(description = "Rebuild similarity edges across the graph.")]
    async fn rebuild_graph(&self) -> Result<Json<GraphRebuildResponse>, String> {
        let store = self.state.store.clone();
        let rebuilt_edges = tokio::task::spawn_blocking(move || store.rebuild_similarity_graph())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(GraphRebuildResponse { rebuilt_edges }))
    }

    #[tool(description = "Create a manual graph edge between two items.")]
    async fn create_manual_edge(
        &self,
        Parameters(request): Parameters<CreateManualEdgeRequest>,
    ) -> Result<Json<GraphEdgePayload>, String> {
        let store = self.state.store.clone();
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
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(edge.into()))
    }

    #[tool(description = "Delete a graph edge by id.")]
    async fn delete_graph_edge(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        let store = self.state.store.clone();
        let target_id = id.clone();
        let deleted = tokio::task::spawn_blocking(move || store.delete_graph_edge(&target_id))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        if !deleted {
            return Err(format!("graph edge {id} not found"));
        }
        Ok(Json(DeleteResponse { id, deleted }))
    }
}

fn stringify_api_error(error: crate::api::ApiError) -> String {
    error.to_string()
}

fn format_search_result(response: &SearchResponse, query: &str) -> CallToolResult {
    let value = serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({}));
    let mut result =
        CallToolResult::success(vec![Content::text(format_search_markdown(response, query))]);
    result.structured_content = Some(value);
    result
}

fn format_search_markdown(response: &SearchResponse, query: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Search: {query}");

    if response.results.is_empty() {
        let _ = writeln!(out, "\nNo matching entries.");
        return out;
    }

    let _ = writeln!(
        out,
        "\nFound {} result{}.",
        response.results.len(),
        if response.results.len() == 1 { "" } else { "s" }
    );

    for (index, hit) in response.results.iter().enumerate() {
        write_result_entry(&mut out, index + 1, hit, None);
    }

    if !response.related.is_empty() {
        let _ = writeln!(
            out,
            "\n## Linked related ({})\n\nItems from the top hit. Ranked by similarity to the query.",
            response.related.len()
        );
        for (index, related) in response.related.iter().enumerate() {
            let hit = SearchResultPayload {
                id: related.id.clone(),
                text: related.text.clone(),
                metadata: related.metadata.clone(),
                source_id: related.source_id.clone(),
                created_at: related.created_at,
                distance: related.distance,
                chunk_context: None,
            };
            write_result_entry(&mut out, index + 1, &hit, related.relation.as_deref());
        }
    }

    out
}

fn write_result_entry(
    out: &mut String,
    index: usize,
    hit: &SearchResultPayload,
    relation: Option<&str>,
) {
    let relevance = ((1.0 - hit.distance).clamp(0.0, 1.0) * 100.0).round() as i64;
    let suffix = match relation {
        Some(r) => format!(" — relation: {r}"),
        None => String::new(),
    };
    let _ = writeln!(
        out,
        "\n### {index}. `{id}` — {relevance}% [{source}]{suffix}",
        id = hit.id,
        source = hit.source_id,
    );
    let _ = writeln!(out, "\n{}", hit.text.trim());
}

/// Build the `StreamableHttpService` tower service that serves MCP traffic.
/// Returns a `tower::Service<http::Request<_>, Response = _, Error = Infallible>`
/// that can be mounted under an axum router.
pub fn streamable_http_service(
    state: AppState,
) -> StreamableHttpService<RustRagMcpServer, LocalSessionManager> {
    let allowed_hosts = state.mcp_allowed_hosts();
    let factory_state = state;
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts)
        .with_sse_keep_alive(Some(Duration::from_secs(15)));
    StreamableHttpService::new(
        move || Ok(RustRagMcpServer::new(factory_state.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}
