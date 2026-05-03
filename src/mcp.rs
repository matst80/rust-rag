//! In-process Model Context Protocol server.
//!
//! This mounts the same tool surface as the `mcp-stdio` bridge, but talks to
//! the store and embedder directly instead of round-tripping through HTTP.
//! The `StreamableHttpService` service is nested into the main axum router at
//! `/mcp`, gated by the same bearer-token middleware that protects every
//! other write path.

use crate::{
    api::{
        ActiveUserPayload, AdminItemPayload, AdminItemsResponse, AppState, CategoriesResponse,
        ChannelsResponse, ClearChannelResponse, CreateManualEdgeRequest, DeleteResponse,
        GraphEdgePayload, GraphEdgesResponse, GraphNeighborhoodQuery, GraphNeighborhoodResponse,
        GraphRebuildResponse, GraphStatusResponse, HealthResponse, ListGraphEdgesQuery,
        ListItemsQuery, MessagePayload, MessagesResponse, SearchRequest, SearchResponse,
        SearchResultPayload, StoreRequest, StoreResponse, UpdateItemRequest, metadata_schema,
        search_core, store_entry_core,
    },
    db::{
        GraphEdgeType, ItemRecord, ListItemsRequest, ManualEdgeInput, MessageQuery,
        MessageSenderKind, MessageUpdate, NewMessage, SortOrder,
    },
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
const SERVER_INSTRUCTIONS: &str = "rust-rag retrieval store + cross-agent collaboration surface.\n\
\n\
PERSISTENT CONTEXT: Store decisions, system state, and task context here so any later agent (or future you) sees it.\n\
SHARED CHANNELS: Use messaging tools (`send_message`, `list_messages`) for structured hand-offs between agents and humans.\n\
CROSS-AGENT AWARENESS: Before starting a task, run `search_entries` (omit `source_id` for global search) to check if another agent already covered it. Read entry `agent_collaboration_guide` in source `knowledge` for the full protocol.\n\
\n\
NAMESPACES (`source_id`): short lowercase buckets — e.g. `knowledge` (durable facts/architecture), `memory` (per-agent notes), `agent_notes`, or `project:<name>:knowledge` / `project:<name>:todos` for project-scoped work.\n\
\n\
TYPICAL FLOW:\n\
1. `search_entries` to load prior context.\n\
2. Do work.\n\
3. `store_entry` (stable id, descriptive metadata.tags + author) to persist outcome.\n\
4. `send_message` to hand off, citing the stored entry id.";

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
pub struct SendMessageParams {
    pub channel: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Option<serde_json::Value>,
    /// Optional sender override (defaults to "claude-manager").
    #[serde(default)]
    pub sender: Option<String>,
    /// "human" | "agent" | "system" (default "agent").
    #[serde(default)]
    pub sender_kind: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListMessagesParams {
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub sender: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    /// Inclusive lower bound on created_at (ms since epoch).
    #[serde(default)]
    pub since: Option<i64>,
    /// Inclusive upper bound on created_at (ms since epoch).
    #[serde(default)]
    pub until: Option<i64>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    /// "asc" | "desc" (default "desc").
    #[serde(default)]
    pub sort_order: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateMessageParams {
    pub id: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Option<serde_json::Value>,
    /// When true, append `text` to existing body instead of replacing.
    #[serde(default)]
    pub append: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ClearChannelParams {
    pub channel: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListPresenceParams {
    #[serde(default)]
    pub channel: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PresenceChannelEntry {
    pub channel: String,
    pub users: Vec<ActiveUserPayload>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PresenceResponse {
    pub channels: Vec<PresenceChannelEntry>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChannelSummaryParams {
    pub channel: String,
    #[serde(default)]
    pub preview_count: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelSummaryPayload {
    pub channel: String,
    pub total_recent: i64,
    pub by_sender: std::collections::HashMap<String, i64>,
    pub by_kind: std::collections::HashMap<String, i64>,
    pub last_activity: i64,
    pub active_users: Vec<ActiveUserPayload>,
    pub preview: Vec<MessagePayload>,
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

    #[tool(description = "Persist knowledge, decisions, summaries, or cross-agent context. Use a stable descriptive `id` (e.g. 'project_x_v1_architecture'), pick the right `source_id` namespace ('knowledge', 'memory', 'agent_notes', 'project:<name>:knowledge'), write `text` as comprehensive markdown, and add `metadata` with `author` + `tags` for searchability.")]
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
        description = "Semantic search across stored entries — use FIRST when starting any task to load prior context and avoid duplicating another agent's work. Omit `source_id` for global cross-agent search; pass it to scope to one namespace. Returns ranked vector hits plus `related` items manually linked from the top hit (not just vector-similar)."
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

    #[tool(description = "Fetch full text + metadata of a single entry by id. Use after `search_entries` or a hand-off message references a specific entry id.")]
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

    #[tool(
        description = "Post updates, status, or hand-offs to a channel for humans or other agents. Standard hand-off pattern: finish work, `store_entry` the details, then `send_message` summarizing + citing the stored entry id (e.g. 'Part 1 done. Specs in entry `part1_summary`. Over to Agent B.'). Channels: `general` for broad updates, `agent-collaboration` / `task-handover` for structured hand-offs. Defaults: kind='text', sender_kind='agent', sender='claude-manager'."
    )]
    async fn send_message(
        &self,
        Parameters(params): Parameters<SendMessageParams>,
    ) -> Result<Json<MessagePayload>, String> {
        if params.channel.trim().is_empty() {
            return Err("channel cannot be empty".to_owned());
        }
        let kind = params
            .kind
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("text")
            .to_owned();
        let metadata = params.metadata.unwrap_or_else(|| serde_json::json!({}));
        if !metadata.is_object() {
            return Err("metadata must be a JSON object".to_owned());
        }
        let metadata_empty = matches!(&metadata, serde_json::Value::Object(map) if map.is_empty());
        if params.text.trim().is_empty() && metadata_empty {
            return Err("text or metadata required".to_owned());
        }
        let sender = params
            .sender
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("claude-manager")
            .to_owned();
        let sender_kind = match params.sender_kind.as_deref() {
            Some("human") => MessageSenderKind::Human,
            Some("system") => MessageSenderKind::System,
            _ => MessageSenderKind::Agent,
        };
        let new_message = NewMessage {
            id: uuid::Uuid::now_v7().to_string(),
            channel: params.channel,
            sender,
            sender_kind,
            text: params.text,
            kind,
            metadata,
            created_at: now_ms(),
        };
        let messages = self.state.messages.clone();
        let record = tokio::task::spawn_blocking(move || messages.send_message(new_message))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        self.state.message_notify.notify_waiters();
        Ok(Json(record.into()))
    }

    #[tool(
        description = "Read messages from a channel — use on agent startup to pick up hand-offs directed at you. Filters: channel, sender, kind, since, limit. When `channel` is provided, the response also includes presence (active_users)."
    )]
    async fn list_messages(
        &self,
        Parameters(params): Parameters<ListMessagesParams>,
    ) -> Result<Json<MessagesResponse>, String> {
        let limit = params.limit.unwrap_or(50).min(500);
        let messages = self.state.messages.clone();
        let query = MessageQuery {
            channel: params.channel.clone(),
            sender: params.sender,
            kind: params.kind,
            min_created_at: params.since,
            max_created_at: params.until,
            limit: Some(limit),
            offset: params.offset,
            sort_order: match params.sort_order.as_deref() {
                Some("asc") => SortOrder::Asc,
                _ => SortOrder::Desc,
            },
        };
        let (rows, total) = tokio::task::spawn_blocking(move || messages.list_messages(query))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let active_users: Vec<ActiveUserPayload> = match params.channel.as_deref() {
            Some(ch) => self
                .state
                .presence
                .list(ch)
                .into_iter()
                .map(Into::into)
                .collect(),
            None => Vec::new(),
        };
        Ok(Json(MessagesResponse {
            messages: rows.into_iter().map(Into::into).collect(),
            total_count: total,
            active_users,
            deleted_ids: Vec::new(),
        }))
    }

    #[tool(description = "List all known channels with message counts and last activity timestamp.")]
    async fn list_channels(&self) -> Result<Json<ChannelsResponse>, String> {
        let messages = self.state.messages.clone();
        let channels = tokio::task::spawn_blocking(move || messages.list_channels())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(Json(ChannelsResponse {
            channels: channels.into_iter().map(Into::into).collect(),
        }))
    }

    #[tool(
        description = "Update an existing message body and/or metadata. With append=true, text is appended instead of replaced. Useful for streaming or annotating prior posts."
    )]
    async fn update_message(
        &self,
        Parameters(params): Parameters<UpdateMessageParams>,
    ) -> Result<Json<MessagePayload>, String> {
        let messages = self.state.messages.clone();
        let id = params.id.clone();
        let update = MessageUpdate {
            text: params.text,
            metadata: params.metadata,
            append_text: params.append.unwrap_or(false),
        };
        let now = now_ms();
        let record = tokio::task::spawn_blocking(move || messages.update_message(&id, update, now))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("message {} not found", params.id))?;
        self.state.message_notify.notify_waiters();
        Ok(Json(record.into()))
    }

    #[tool(description = "Wipe every message in a channel. Returns the number of rows deleted.")]
    async fn clear_channel(
        &self,
        Parameters(params): Parameters<ClearChannelParams>,
    ) -> Result<Json<ClearChannelResponse>, String> {
        let channel = params.channel.trim().to_owned();
        if channel.is_empty() {
            return Err("channel cannot be empty".to_owned());
        }
        let messages = self.state.messages.clone();
        let target = channel.clone();
        let wiped = tokio::task::spawn_blocking(move || messages.clear_channel(&target))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        for row in &wiped {
            self.state.tombstones.record(&row.channel, &row.id);
        }
        if !wiped.is_empty() {
            self.state.message_notify.notify_waiters();
        }
        Ok(Json(ClearChannelResponse {
            channel,
            deleted_count: wiped.len(),
        }))
    }

    #[tool(
        description = "List active users (presence). With `channel` returns users active in that channel; without it returns presence for every channel."
    )]
    async fn list_presence(
        &self,
        Parameters(params): Parameters<ListPresenceParams>,
    ) -> Result<Json<PresenceResponse>, String> {
        let entries = match params.channel.as_deref() {
            Some(ch) => {
                let users: Vec<ActiveUserPayload> = self
                    .state
                    .presence
                    .list(ch)
                    .into_iter()
                    .map(Into::into)
                    .collect();
                vec![PresenceChannelEntry {
                    channel: ch.to_owned(),
                    users,
                }]
            }
            None => self
                .state
                .presence
                .list_all()
                .into_iter()
                .map(|(channel, users)| PresenceChannelEntry {
                    channel,
                    users: users.into_iter().map(Into::into).collect(),
                })
                .collect(),
        };
        Ok(Json(PresenceResponse { channels: entries }))
    }

    #[tool(
        description = "Cheap channel stats (no LLM call): counts by sender + kind, last activity, active users, and last N message previews."
    )]
    async fn channel_summary(
        &self,
        Parameters(params): Parameters<ChannelSummaryParams>,
    ) -> Result<Json<ChannelSummaryPayload>, String> {
        let preview_count = params.preview_count.unwrap_or(5).min(30).max(1);
        let messages = self.state.messages.clone();
        let channel = params.channel.clone();
        let query = MessageQuery {
            channel: Some(channel.clone()),
            limit: Some(100),
            sort_order: SortOrder::Desc,
            ..Default::default()
        };
        let (rows, _) = tokio::task::spawn_blocking(move || messages.list_messages(query))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let total_recent = rows.len();
        let mut by_sender = std::collections::HashMap::<String, i64>::new();
        let mut by_kind = std::collections::HashMap::<String, i64>::new();
        let mut last_activity: i64 = 0;
        for m in &rows {
            *by_sender.entry(m.sender.clone()).or_insert(0) += 1;
            *by_kind.entry(m.kind.clone()).or_insert(0) += 1;
            if m.created_at > last_activity {
                last_activity = m.created_at;
            }
        }
        let preview: Vec<MessagePayload> = rows
            .into_iter()
            .rev()
            .take(preview_count)
            .map(Into::into)
            .collect();
        let active_users: Vec<ActiveUserPayload> = self
            .state
            .presence
            .list(&channel)
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(Json(ChannelSummaryPayload {
            channel,
            total_recent: total_recent as i64,
            by_sender,
            by_kind,
            last_activity,
            active_users,
            preview,
        }))
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

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
