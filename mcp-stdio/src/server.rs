use crate::{
    client::RustRagHttpClient,
    config::{SearchFormat, ToolGroup},
};
use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler,
};
use rust_rag::{
    api::{
        CreateManualEdgeRequest, DeleteResponse, GraphNeighborhoodQuery, ListGraphEdgesQuery,
        ListItemsQuery, ListMessagesQuery, SearchRequest, SearchResponse, SearchResultPayload,
        SendMessageRequest, SmartStoreRequest, SmartStoreResponse, StoreRequest, UpdateItemRequest,
    },
    db::GraphEdgeType,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt::Write};

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
    search_format: SearchFormat,
    tool_router: ToolRouter<Self>,
}

impl RustRagMcpServer {
    pub fn new(
        client: RustRagHttpClient,
        enabled_groups: &BTreeSet<ToolGroup>,
        info: BridgeServerInfo,
        search_format: SearchFormat,
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
        if enabled_groups.contains(&ToolGroup::Messages) {
            tool_router = tool_router + Self::messages_tools();
        }

        Self {
            client,
            info,
            search_format,
            tool_router,
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RustRagMcpServer {
    fn get_info(&self) -> ServerInfo {
        let info =
            ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
                Implementation::new(self.info.name.clone(), self.info.version.clone()),
            );

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
    #[schemars(schema_with = "rust_rag::api::metadata_schema")]
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
        self.client
            .health()
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(
        description = "Persist knowledge, decisions, summaries, or cross-agent context. Use a stable descriptive `id` (e.g. 'project_x_v1_architecture'), pick the right `source_id` namespace ('knowledge', 'memory', 'agent_notes', 'project:<name>:knowledge'), write `text` as comprehensive markdown, and add `metadata` with `author` + `tags` for searchability."
    )]
    async fn store_entry(
        &self,
        Parameters(request): Parameters<StoreRequest>,
    ) -> Result<Json<rust_rag::api::StoreResponse>, String> {
        self.client
            .store(&request)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(
        description = "Use LLM to extract and store multiple entries from a messy text blob. Automatically determines source_id and metadata."
    )]
    async fn smart_store(
        &self,
        Parameters(request): Parameters<SmartStoreRequest>,
    ) -> Result<Json<SmartStoreResponse>, String> {
        self.client
            .smart_store(&request)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(
        description = "Semantic search across stored entries — use FIRST when starting any task to load prior context and avoid duplicating another agent's work. Omit `source_id` for global cross-agent search; pass it to scope to one namespace. Returns ranked vector hits plus `related` items manually linked from the top hit (not just vector-similar)."
    )]
    async fn search_entries(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, String> {
        let response = self
            .client
            .search(&request)
            .await
            .map_err(stringify_error)?;
        Ok(format_search_response(
            &response,
            &request.query,
            self.search_format,
        ))
    }

    #[tool(
        description = "Fetch full text + metadata of a single entry by id. Use after `search_entries` or when a hand-off message references a specific entry id."
    )]
    async fn get_entry(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<rust_rag::api::AdminItemPayload>, String> {
        self.client
            .get_item(&id)
            .await
            .map(Json)
            .map_err(stringify_error)
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

#[rmcp::tool_router(router = messages_tools)]
impl RustRagMcpServer {
    #[tool(
        description = "Post updates, status, or hand-offs to a channel for humans or other agents. Standard hand-off pattern: finish work, `store_entry` the details, then `send_message` summarizing + citing the stored entry id (e.g. 'Part 1 done. Specs in entry `part1_summary`. Over to Agent B.'). Channels: `general` for broad updates, `agent-collaboration` / `task-handover` for structured hand-offs. sender_kind defaults to 'agent' when called by an MCP/agent client."
    )]
    async fn send_message(
        &self,
        Parameters(request): Parameters<SendMessageRequest>,
    ) -> Result<Json<rust_rag::api::MessagePayload>, String> {
        let mut request = request;
        if request.sender_kind.is_none() {
            request.sender_kind = Some(rust_rag::db::MessageSenderKind::Agent);
        }
        self.client
            .send_message(&request)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(
        description = "Read messages from a channel — use on agent startup to pick up hand-offs directed at you. Filters: channel, sender, since (ms epoch), until (ms epoch), limit, offset, sort_order ('asc' or 'desc')."
    )]
    async fn message_history(
        &self,
        Parameters(query): Parameters<ListMessagesQuery>,
    ) -> Result<Json<rust_rag::api::MessagesResponse>, String> {
        self.client
            .list_messages(&query)
            .await
            .map(Json)
            .map_err(stringify_error)
    }

    #[tool(description = "List all message channels with their message counts and last activity.")]
    async fn list_message_channels(
        &self,
    ) -> Result<Json<rust_rag::api::ChannelsResponse>, String> {
        self.client
            .list_message_channels()
            .await
            .map(Json)
            .map_err(stringify_error)
    }
}

fn stringify_error(error: anyhow::Error) -> String {
    error.to_string()
}

fn format_search_response(
    response: &SearchResponse,
    query: &str,
    format: SearchFormat,
) -> CallToolResult {
    match format {
        SearchFormat::Json => {
            let value = serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({}));
            CallToolResult::structured(value)
        }
        SearchFormat::Markdown => {
            CallToolResult::success(vec![Content::text(format_search_markdown(response, query))])
        }
        SearchFormat::Both => {
            let value = serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({}));
            let mut result = CallToolResult::success(vec![Content::text(format_search_markdown(
                response, query,
            ))]);
            result.structured_content = Some(value);
            result
        }
    }
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
    if let Some(ctx) = &hit.chunk_context {
        let _ = writeln!(out, "\n> ... {} ...", ctx.trim());
    } else {
        let _ = writeln!(out, "\n{}", hit.text.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_rag::api::RelatedResultPayload;

    fn hit(id: &str, distance: f32, text: &str) -> SearchResultPayload {
        SearchResultPayload {
            id: id.to_owned(),
            text: text.to_owned(),
            metadata: serde_json::json!({}),
            source_id: "memory".to_owned(),
            created_at: 1,
            distance,
        }
    }

    #[test]
    fn markdown_includes_results_and_related_sections() {
        let response = SearchResponse {
            results: vec![hit("doc-top", 0.15, "kubernetes ingress overview")],
            related: vec![RelatedResultPayload {
                id: "doc-storage".to_owned(),
                text: "persistent volumes".to_owned(),
                metadata: serde_json::json!({}),
                source_id: "memory".to_owned(),
                created_at: 2,
                distance: 0.55,
                relation: Some("supports".to_owned()),
            }],
        };

        let md = format_search_markdown(&response, "kubernetes ingress");
        assert!(md.contains("# Search: kubernetes ingress"));
        assert!(md.contains("doc-top"));
        assert!(md.contains("85%"));
        assert!(md.contains("User-linked related"));
        assert!(md.contains("doc-storage"));
        assert!(md.contains("relation: supports"));
        assert!(
            md.contains("persistent volumes"),
            "related body text should be inlined so the LLM doesn't need an extra get_entry call"
        );
    }

    #[test]
    fn json_format_sets_structured_content_only() {
        let response = SearchResponse {
            results: vec![hit("doc-1", 0.1, "hello")],
            related: Vec::new(),
        };
        let result = format_search_response(&response, "query", SearchFormat::Json);
        assert!(result.structured_content.is_some());
    }

    #[test]
    fn both_format_sets_markdown_and_structured() {
        let response = SearchResponse {
            results: vec![hit("doc-1", 0.1, "hello")],
            related: Vec::new(),
        };
        let result = format_search_response(&response, "query", SearchFormat::Both);
        assert!(result.structured_content.is_some());
        let text = result
            .content
            .iter()
            .find_map(|c| match &c.raw {
                rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .expect("text content present");
        assert!(text.contains("# Search: query"));
    }
}
