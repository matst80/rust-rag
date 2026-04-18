use crate::{client::RustRagHttpClient, config::ToolGroup};
use rmcp::{
    ServerHandler, tool,
    handler::server::{router::tool::ToolRouter, wrapper::{Json, Parameters}},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool_handler,
};
use rust_rag::{
    api::{
        CreateManualEdgeRequest, DeleteResponse, GraphNeighborhoodQuery, ListGraphEdgesQuery,
        ListItemsQuery, SearchRequest, StoreRequest, UpdateItemRequest,
    },
    db::GraphEdgeType,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct BridgeServerInfo {
    pub name: String,
    pub version: String,
    pub instructions: Option<String>,
}

#[derive(Clone)]
pub struct RustRagMcpServer {
    client: RustRagHttpClient,
    info: BridgeServerInfo,
    tool_router: ToolRouter<Self>,
}

impl RustRagMcpServer {
    pub fn new(
        client: RustRagHttpClient,
        enabled_groups: &BTreeSet<ToolGroup>,
        info: BridgeServerInfo,
    ) -> Self {
        let mut tool_router = ToolRouter::<Self>::new();
        if enabled_groups.contains(&ToolGroup::Core) {
            tool_router = tool_router + Self::core_tools();
        }
        if enabled_groups.contains(&ToolGroup::Admin) {
            tool_router = tool_router + Self::admin_tools();
        }
        if enabled_groups.contains(&ToolGroup::Graph) {
            tool_router = tool_router + Self::graph_tools();
        }

        Self {
            client,
            info,
            tool_router,
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RustRagMcpServer {
    fn get_info(&self) -> ServerInfo {
        let info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                self.info.name.clone(),
                self.info.version.clone(),
            ));

        match &self.info.instructions {
            Some(instructions) => info.with_instructions(instructions.clone()),
            None => info,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateItemParams {
    pub id: String,
    pub text: String,
    pub metadata: serde_json::Value,
    pub source_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GraphNeighborhoodParams {
    pub id: String,
    pub depth: Option<usize>,
    pub limit: Option<usize>,
    pub edge_type: Option<GraphEdgeType>,
}

impl From<UpdateItemParams> for UpdateItemRequest {
    fn from(value: UpdateItemParams) -> Self {
        Self {
            text: value.text,
            metadata: value.metadata,
            source_id: value.source_id,
        }
    }
}

#[rmcp::tool_router(router = core_tools)]
impl RustRagMcpServer {
    #[tool(description = "Return rust-rag service health and embedder readiness.")]
    async fn health_status(&self) -> Result<Json<rust_rag::api::HealthResponse>, String> {
        self.client.health().await.map(Json).map_err(stringify_error)
    }

    #[tool(description = "Store a text entry with metadata and source_id in rust-rag.")]
    async fn store_entry(
        &self,
        Parameters(request): Parameters<StoreRequest>,
    ) -> Result<Json<rust_rag::api::StoreResponse>, String> {
        self.client.store(&request).await.map(Json).map_err(stringify_error)
    }

    #[tool(description = "Run semantic search against stored entries.")]
    async fn search_entries(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> Result<Json<rust_rag::api::SearchResponse>, String> {
        self.client.search(&request).await.map(Json).map_err(stringify_error)
    }
}

#[rmcp::tool_router(router = admin_tools)]
impl RustRagMcpServer {
    #[tool(description = "List all source_id categories and their item counts.")]
    async fn list_categories(&self) -> Result<Json<rust_rag::api::CategoriesResponse>, String> {
        self.client
            .list_categories()
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "List items, optionally filtered by source_id.")]
    async fn list_items(
        &self,
        Parameters(query): Parameters<ListItemsQuery>,
    ) -> Result<Json<rust_rag::api::AdminItemsResponse>, String> {
        self.client
            .list_items(&query)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "Update an existing item by id.")]
    async fn update_item(
        &self,
        Parameters(params): Parameters<UpdateItemParams>,
    ) -> Result<Json<rust_rag::api::AdminItemPayload>, String> {
        let id = params.id.clone();
        let request = UpdateItemRequest::from(params);
        self.client
            .update_item(&id, &request)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "Delete an item by id.")]
    async fn delete_item(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        self.client
            .delete_item(&id)
            .await
            .map(Json)
            .map_err(stringify_error)
    }
}

#[rmcp::tool_router(router = graph_tools)]
impl RustRagMcpServer {
    #[tool(description = "Return current graph configuration and edge counts.")]
    async fn graph_status(&self) -> Result<Json<rust_rag::api::GraphStatusResponse>, String> {
        self.client
            .graph_status()
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "List graph edges, optionally filtered by item_id or edge type.")]
    async fn list_graph_edges(
        &self,
        Parameters(query): Parameters<ListGraphEdgesQuery>,
    ) -> Result<Json<rust_rag::api::GraphEdgesResponse>, String> {
        self.client
            .list_graph_edges(&query)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "Return the graph neighborhood around a center item id.")]
    async fn graph_neighborhood(
        &self,
        Parameters(params): Parameters<GraphNeighborhoodParams>,
    ) -> Result<Json<rust_rag::api::GraphNeighborhoodResponse>, String> {
        let id = params.id.clone();
        let query = GraphNeighborhoodQuery {
            depth: params.depth,
            limit: params.limit,
            edge_type: params.edge_type,
        };
        self.client
            .graph_neighborhood(&id, &query)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "Rebuild similarity edges across the graph.")]
    async fn rebuild_graph(&self) -> Result<Json<rust_rag::api::GraphRebuildResponse>, String> {
        self.client
            .rebuild_graph()
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "Create a manual graph edge between two items.")]
    async fn create_manual_edge(
        &self,
        Parameters(request): Parameters<CreateManualEdgeRequest>,
    ) -> Result<Json<rust_rag::api::GraphEdgePayload>, String> {
        self.client
            .create_manual_edge(&request)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "Delete a graph edge by id.")]
    async fn delete_graph_edge(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        self.client
            .delete_graph_edge(&id)
            .await
            .map(Json)
            .map_err(stringify_error)
    }
}

fn stringify_error(error: anyhow::Error) -> String {
    error.to_string()
}