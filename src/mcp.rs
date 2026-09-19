//! In-process Model Context Protocol server.
//!
//! This mounts the tool surface directly on the server, talking to
//! the store and embedder instead of round-tripping through HTTP.
//! The `StreamableHttpService` service is nested into the main axum router at
//! `/mcp`, gated by the same bearer-token middleware that protects every
//! other write path.

use crate::{
    api::{
        ActiveUserPayload, AdminItemPayload, AppState, ClearChannelResponse, DeleteResponse,
        EntryNeighbor, HealthResponse, ListItemsQuery, MessagePayload, SearchRequest,
        SearchResponse, SearchResultPayload, StoreRequest, StoreResponse, UpdateItemRequest,
        metadata_schema, search_core, store_entry_core,
    },
    db::{
        GraphEdgeRecord, GraphEdgeType, GraphNeighborhood, ItemRecord, ListItemsRequest,
        MessageQuery, MessageSenderKind, MessageUpdate, NewMessage, SortOrder,
    },
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use rmcp::{
    RoleServer, ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    service::RequestContext,
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
RELATED ENDPOINTS: graph/ontology curation + projection-map tools live on the admin MCP at `/mcp/admin`; ACP agent-session control at `/mcp/acp` (same auth).\n\
\n\
FIRST CALL: run `list_memory_conventions` once per session — it returns the canonical `source_id` taxonomy, required metadata fields, default typed schemas (decision/fact/todo/incident/note/recipe/workout/page_component), and the edge-predicate vocabulary used by the ontology worker. The doc is itself a stored entry (`rust_rag_mcp_usage_guide_v1` in `knowledge`) so anyone can refine the conventions via `update_item` / `store_entry` — no redeploy. Use `list_schemas` to inspect schema details before storing typed entries.\n\
\n\
NAMESPACES (`source_id`): short lowercase buckets — e.g. `knowledge` (durable facts/architecture), `memory` (per-agent notes), `agent_notes`, or `project:<name>:knowledge` / `project:<name>:todos` for project-scoped work.\n\
\n\
TYPICAL FLOW:\n\
1. `search_entries` to load prior context.\n\
2. Do work.\n\
3. MANDATORY: `store_entry` (stable id, descriptive metadata.tags + author) to persist every significant outcome. \"If it isn't in the RAG, it didn't happen.\"\n\
4. `send_message` to hand off, citing the stored entry id.";

/// Stable id of the live, editable usage guide stored in the rust-rag instance
/// itself. `list_memory_conventions` returns this entry's text when present;
/// `build_memory_conventions` is the compiled-in fallback used otherwise.
const MEMORY_CONVENTIONS_ENTRY_ID: &str = "rust_rag_mcp_usage_guide_v1";

/// JSON document returned by the `list_memory_conventions` tool. Authoritative
/// reference for how agents should structure stored memory.
fn build_memory_conventions() -> serde_json::Value {
    serde_json::json!({
        "stable_id": {
            "format": "descriptive snake_case, optional version suffix (e.g. `_v2`).",
            "rules": [
                "Reusing an existing `id` in `store_entry` replaces the entry (upsert).",
                "Never use UUIDs or timestamps for `id` — those are auto-generated when omitted.",
                "Tie versioned successors together via the `decision` schema (`supersedes` / `superseded_by`) or a manual edge."
            ]
        },
        "source_id_taxonomy": {
            "reserved": {
                "knowledge": "Durable cross-project facts, architecture, evergreen reference material.",
                "memory": "Per-agent scratch notes that should survive sessions.",
                "agent_notes": "Hand-off context between agents working a shared task."
            },
            "project_scoped": {
                "pattern": "project:<slug>:knowledge | project:<slug>:todos",
                "example": "project:rust-rag:knowledge",
                "rules": [
                    "`<slug>` is short, lowercase, kebab-case if needed.",
                    "Open todos go in `project:<slug>:todos` with metadata `status` and `priority`."
                ]
            }
        },
        "required_metadata": {
            "always": ["author", "tags"],
            "for_todos": ["status", "priority"],
            "optional_but_recommended": ["doc_type"]
        },
        "typed_entries": {
            "how": "Call `store_entry` with `type` set to one of the registered schemas and `data` carrying the structured payload. Validation is enforced server-side via JSON Schema. Discover schemas with `list_schemas` / `get_schema`.",
            "default_schemas": {
                "decision": "ADR-style record: context, decision, consequences, status (proposed/accepted/superseded/rejected).",
                "harness_doc": "Governing repository document (ADR, SPEC, INVARIANT, OVERVIEW, ARCHITECTURE, GUIDE). Fields: repo, repo_path, doc_type, title, version, status, summary, sections, invariants, source_files, parent_doc_id, supersedes. Use `ingest_doc` tool for automatic ingestion & linking.",
                "fact": "Atomic claim with `source` and `confidence` (0-1). Optional `expires_at` for staleness.",
                "todo": "Single task with `status` (open/in_progress/done/cancelled) and optional `priority`/`due`.",
                "incident": "Operational incident: timeline, severity, root_cause, resolution.",
                "note": "Lightweight titled prose with optional `tags` / `links` — fallback when no better schema fits."
            },
            "repo_documentation": {
                "tool": "ingest_doc",
                "id_format": "doc:<repo>:<doc_type_lower>:<slug>",
                "linking": [
                    "`(:harness_repo)-[:contains]->(:harness_doc)` links repo root to doc.",
                    "`(:harness_doc {ADR/spec})-[:part_of]->(:harness_doc {overview})` links component specs to overview.",
                    "`(:harness_doc {overview})-[:contains]->(:harness_doc {ADR/spec})` enables tree nesting in Grill cockpit.",
                    "`(:harness_doc)-[:supersedes]->(:harness_doc)` tracks architectural evolution."
                ]
            },
            "tip": "If your content fits a schema, prefer typed storage — it composes with `search_entries.type` filtering and the analyze pipeline."
        },
        "edge_vocabulary": {
            "tool": "create_manual_edge",
            "canonical_predicates": crate::db::default_ontology_predicates().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            "note": "Predicates are now dynamic and project-specific. The list above contains the system defaults. Call `list_ontology_predicates` to see active ones for your context."
        }
    })
}

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
pub struct AppendRequest {
    /// ID of the entry to append to.
    pub id: String,
    /// Text to append to the end of the entry.
    pub text: String,
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
    /// Optional wiki path. Pass an empty string to clear, omit to keep
    /// the existing value, or a slash-separated path to set/replace.
    #[serde(default)]
    pub path: Option<String>,
    /// Optional structured-data type name. References a registered schema.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// Typed payload validated against the schema for `type`. Supply only
    /// when updating; omit to leave existing payload unchanged.
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub data: Option<serde_json::Value>,
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
pub struct WaitForMessageParams {
    /// Channel to listen on. Required.
    pub channel: String,
    /// Optional sender filter (exact match).
    #[serde(default)]
    pub sender: Option<String>,
    /// Optional kind filter (exact match).
    #[serde(default)]
    pub kind: Option<String>,
    /// Substring match against message text. Case-sensitive.
    #[serde(default)]
    pub contains: Option<String>,
    /// Subset match against metadata: every key/value pair in the supplied
    /// object must appear (and equal) in the incoming message metadata.
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata_match: Option<serde_json::Value>,
    /// Inclusive lower bound on `created_at` (ms). Buffered messages newer
    /// than this that match filters are returned synchronously without
    /// waiting. Defaults to "now" (only future messages match).
    #[serde(default)]
    pub since: Option<i64>,
    /// Seconds to block. Default 60, max 600.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WaitForMessageResponse {
    pub matched: bool,
    pub message: Option<MessagePayload>,
}

#[tool_router(router = tool_router)]
impl RustRagMcpServer {
    #[tool(description = "Return rust-rag service health and embedder readiness.")]
    async fn health_status(&self) -> Result<Json<HealthResponse>, String> {
        let (_, body) = self.state.embedder.health();
        Ok(Json(body.0))
    }

    #[tool(
        description = "Return the canonical conventions for storing memory in this rust-rag instance: stable-id pattern, `source_id` taxonomy (reserved + `project:<slug>:*`), required metadata fields, default typed schemas (decision/fact/todo/incident/note + bundled schemas) with one-line purposes, and the edge-predicate vocabulary used by the ontology worker. \
LIVE-EDITABLE: the actual document is the entry `rust_rag_mcp_usage_guide_v1` in source `knowledge` — `update_item` (or `store_entry` with the same id) to iterate without redeploying. Falls back to a built-in JSON default when the entry is missing. \
Run this once at the start of a session before storing anything — it tells you whether your content should be a typed entry (call `list_schemas` to inspect the schema) or free-text, and which predicates to pass to `create_manual_edge`."
    )]
    async fn list_memory_conventions(&self) -> Result<CallToolResult, String> {
        let store = self.state.store.clone();
        let entry =
            tokio::task::spawn_blocking(move || store.get_item(MEMORY_CONVENTIONS_ENTRY_ID))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
        let text = match entry {
            Some(record) => record.text,
            None => {
                let doc = build_memory_conventions();
                serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?
            }
        };
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "Persist knowledge, decisions, summaries, or cross-agent context. \
BEFORE STORING: if you have not yet, call `list_memory_conventions` (for taxonomy + required fields) and `list_schemas` (for typed-entry options) — most agents skip this and store mush. \
SCHEMA-FIRST: if your content fits a registered schema (decision/fact/todo/incident/note/recipe/workout/page_component or any other in `list_schemas`), pass `type` + `data` for server-side JSON Schema validation and structured retrieval (`search_entries.type`, `list_items.type`). Use free-text only when no schema fits. \
STABLE ID: pass a descriptive `id` like `rust_rag_auth_redesign_v2`. Reusing an existing `id` REPLACES the entry (upsert) — use this for evolving notes; bump a `_vN` suffix when the change is breaking enough that callers should distinguish. Omit `id` only for ephemeral or strictly append-only content. \
SOURCE_ID: pick from the reserved buckets (`knowledge` / `memory` / `agent_notes`) or use `project:<slug>:knowledge` / `project:<slug>:todos`. Free-form values are allowed but won't compose with other agents' searches — see `list_memory_conventions` first. \
METADATA: always include `author` and `tags`. For `*:todos` source_ids also include `status` and `priority`. \
PATH: optional slash-separated wiki path (`team/handbook`) groups the entry in the tree under its source_id; orthogonal to `type`."
    )]
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
        description = "Append text to an existing entry. This is more token-efficient than `update_item` for adding notes or extending documents. Re-embeds the entire content after appending."
    )]
    async fn append_to_entry(
        &self,
        Parameters(params): Parameters<AppendRequest>,
    ) -> Result<Json<StoreResponse>, String> {
        crate::api::append_to_entry_core(&self.state, params.id, params.text)
            .await
            .map(Json)
            .map_err(stringify_api_error)
    }

    #[tool(
        description = "Semantic search across stored entries — use FIRST when starting any task to load prior context and avoid duplicating another agent's work. Omit `source_id` for global cross-agent search; pass it to scope to one namespace (see `list_memory_conventions` for the canonical taxonomy). Pass `type` to scope to a single typed-entry schema, or `type_names` to scope to several at once (e.g. the harness node types) — results are merged per type before the top-K cut. Returns ranked vector hits plus `related` items manually linked from the top hit (not just vector-similar). Cross-encoder reranking is ON by default for MCP callers (better top-K relevance at small latency cost); pass `rerank: false` to skip when latency matters or the server has no reranker loaded."
    )]
    async fn search_entries(
        &self,
        Parameters(mut request): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, String> {
        request.rerank = request.rerank.or(Some(true));
        let query = request.query.clone();
        let response = search_core(&self.state, request, None)
            .await
            .map_err(stringify_api_error)?;
        Ok(format_search_result(&response, &query))
    }

    #[tool(
        description = "Dry-run LLM analysis of a candidate entry: embeds it, retrieves top-K semantically similar neighbors, then asks an OpenAI-compatible chat backend to classify the candidate vs each neighbor (agrees/refines/supersedes/contradicts/duplicates/unrelated) and extract cluster_hint, tags, title, summary, doc_type, freshness, quality, suggested_edges. Returns the analysis JSON without writing anything. Useful for the entry-view re-run button or for previewing what `store_entry` would auto-tag. Server must be configured with RAG_ANALYSIS_ENABLED + model."
    )]
    async fn analyze_entry(
        &self,
        Parameters(params): Parameters<crate::api::AnalyzeEntryParams>,
    ) -> Result<Json<crate::api::StoreAnalysis>, String> {
        crate::api::run_analysis(
            &self.state,
            &params.text,
            params.source_id.as_deref(),
            params.exclude_id.as_deref(),
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Fetch full text + metadata of a single entry by id. Also returns 'neighbors' (id + title of similar entries) for contextual expansion."
    )]
    async fn get_entry(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<CallToolResult, String> {
        let store = self.state.store.clone();
        let target_id = id.clone();
        let (item, analysis, neighborhood) = tokio::task::spawn_blocking(move || {
            let item = store.get_item(&target_id)?;
            let analysis = store.get_item_analysis(&target_id).ok().flatten();
            let neighborhood = store.graph_neighborhood(&target_id, 1, 10, None).ok();
            Ok::<
                (
                    Option<ItemRecord>,
                    Option<crate::db::ItemAnalysisRecord>,
                    Option<GraphNeighborhood>,
                ),
                anyhow::Error,
            >((item, analysis, neighborhood))
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;

        let item = item.ok_or_else(|| "item not found".to_owned())?;
        let mut payload: AdminItemPayload = item.into();

        if let Some(a) = analysis {
            payload.analysis = Some(a.analysis);
            payload.analysis_at = Some(a.analysis_at);
            payload.analysis_model = Some(a.analysis_model);
        }

        let mut text = format_item_markdown(&payload);

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
                            status == Some("confirmed")
                                || (status.is_none() && confidence >= 0.7)
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
                .map(|(n, best_edge)| {
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
                        relationship: best_edge.relation.clone(),
                        source_type: n
                            .metadata
                            .get("source_type")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_owned()),
                        thumbnail: None,
                        repo,
                    }
                })
                .collect();
            if !neighbors.is_empty() {
                let _ = writeln!(text, "\n## Contextual Neighbors");
                for neighbor in neighbors {
                    let title = neighbor.title.as_deref().unwrap_or("untitled");
                    let rel = neighbor.relationship.as_deref().unwrap_or("similar");
                    let repo_str = neighbor
                        .repo
                        .as_deref()
                        .map(|r| format!(" [repo: {r}]"))
                        .unwrap_or_default();
                    let _ = writeln!(text, "- `{}` ({}) — [{rel}]{repo_str}", neighbor.id, title);
                }
            }
        }

        let value = serde_json::to_value(&payload).unwrap_or_else(|_| serde_json::json!({}));
        let mut result = CallToolResult::success(vec![Content::text(text)]);
        result.structured_content = Some(value);
        Ok(result)
    }

    #[tool(
        description = "List all `source_id` categories and their item counts. Reserved/canonical buckets and the `project:<slug>:*` pattern are documented in `list_memory_conventions` — call that first if you are about to invent a new source_id."
    )]
    async fn list_categories(&self) -> Result<String, String> {
        let store = self.state.store.clone();
        let categories = tokio::task::spawn_blocking(move || store.list_categories())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(format_categories_markdown(&categories))
    }

    #[tool(
        description = "List items, optionally filtered by `source_id` and/or `path_prefix` for wiki-style hierarchical browsing."
    )]
    async fn list_items(
        &self,
        Parameters(query): Parameters<ListItemsQuery>,
    ) -> Result<CallToolResult, String> {
        let store = self.state.store.clone();
        let limit = query.limit.unwrap_or(50);
        let offset = query.offset.unwrap_or(0);
        let path_prefix = match query.path_prefix.as_deref() {
            Some(p) => crate::db::normalize_path(p).map_err(|e| e.to_string())?,
            None => None,
        };
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
        };
        let (items, total) = tokio::task::spawn_blocking(move || store.list_items(request))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;

        let text = format_item_list_markdown(&items, total, limit, offset);
        let value = serde_json::to_value(&items).unwrap_or_else(|_| serde_json::json!([]));
        let mut result = CallToolResult::success(vec![Content::text(text)]);
        result.structured_content = Some(value);
        Ok(result)
    }

    #[tool(
        description = "Update an existing item by id. Pass `path` to set or clear the wiki path; omit to leave it untouched."
    )]
    async fn update_item(
        &self,
        Parameters(params): Parameters<UpdateItemParams>,
    ) -> Result<Json<AdminItemPayload>, String> {
        let id = params.id.clone();
        let path_override: Option<Option<String>> = match params.path.as_deref() {
            Some(p) => Some(crate::db::normalize_path(p).map_err(|e| e.to_string())?),
            None => None,
        };
        if let Some(ref type_name) = params.type_name {
            if let Some(data) = params.data.clone() {
                let cache = self.state.schema_cache.clone();
                let store = self.state.store.clone();
                let tn = type_name.clone();
                tokio::task::spawn_blocking(move || cache.validate(&tn, &data, store.as_ref()))
                    .await
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
            }
        } else if params.data.is_some() {
            return Err("`data` is only valid when `type` is set".to_string());
        }
        let request = UpdateItemRequest {
            text: params.text,
            metadata: params.metadata,
            source_id: params.source_id,
            path: params.path,
            type_name: params.type_name.clone(),
            data: params.data.clone(),
        };
        let type_override = params.type_name;
        let data_override = params.data;
        let embedder = self
            .state
            .embedder
            .get_ready()
            .map_err(stringify_api_error)?;
        let store = self.state.store.clone();

        let record = tokio::task::spawn_blocking(move || -> anyhow::Result<ItemRecord> {
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
                updated_at: now_ms(),
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
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        crate::api::invalidate_cms_nodes(&self.state, [record.id.clone()])
            .await
            .map_err(stringify_api_error)?;
        Ok(Json(record.into()))
    }

    #[tool(description = "Delete an item by id.")]
    async fn delete_item(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        crate::api::invalidate_cms_nodes(&self.state, [id.clone()])
            .await
            .map_err(stringify_api_error)?;
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
        self.state.publish_message(&record);
        Ok(Json(record.into()))
    }

    #[tool(
        description = "Block until a message arrives in `channel` matching the supplied filters (sender, kind, contains substring, metadata_match subset), then return it. Hands-off rendezvous: external systems post a message when an event happens (deploy completed, daemon ready, task finished) and the waiter wakes synchronously instead of polling. `since` (ms) lets the caller catch messages it might have missed between previous calls — buffered matches are returned immediately without waiting. Timeout default 60s, max 600s. When the timeout elapses with no match, returns `{ matched: false }`."
    )]
    async fn wait_for_message(
        &self,
        Parameters(params): Parameters<WaitForMessageParams>,
    ) -> Result<Json<WaitForMessageResponse>, String> {
        let channel = params.channel.trim().to_owned();
        if channel.is_empty() {
            return Err("channel cannot be empty".into());
        }
        let timeout = Duration::from_secs(params.timeout_secs.unwrap_or(60).clamp(1, 600));
        let since = params.since.unwrap_or_else(now_ms);
        let metadata_match = match params.metadata_match {
            Some(serde_json::Value::Object(m)) => Some(m),
            Some(serde_json::Value::Null) | None => None,
            Some(_) => return Err("metadata_match must be a JSON object".into()),
        };

        // Subscribe BEFORE the catch-up query to avoid a TOCTOU gap where a
        // message lands between the query and the subscription.
        let mut rx = self.state.message_broadcast.subscribe();

        // Catch-up: any matching record already buffered with created_at > since.
        let messages = self.state.messages.clone();
        let query = MessageQuery {
            channel: Some(channel.clone()),
            sender: params.sender.clone(),
            kind: params.kind.clone(),
            min_created_at: Some(since),
            max_created_at: None,
            limit: Some(50),
            offset: None,
            sort_order: SortOrder::Asc,
        };
        let (existing, _) = tokio::task::spawn_blocking(move || messages.list_messages(query))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        for record in existing {
            if message_matches(
                &record,
                &channel,
                params.sender.as_deref(),
                params.kind.as_deref(),
                params.contains.as_deref(),
                metadata_match.as_ref(),
            ) {
                return Ok(Json(WaitForMessageResponse {
                    matched: true,
                    message: Some(record.into()),
                }));
            }
        }

        // Block on broadcast until match or timeout.
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(Json(WaitForMessageResponse {
                    matched: false,
                    message: None,
                }));
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(record)) => {
                    if message_matches(
                        &record,
                        &channel,
                        params.sender.as_deref(),
                        params.kind.as_deref(),
                        params.contains.as_deref(),
                        metadata_match.as_ref(),
                    ) {
                        return Ok(Json(WaitForMessageResponse {
                            matched: true,
                            message: Some(record.into()),
                        }));
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(_)) | Err(_) => {
                    return Ok(Json(WaitForMessageResponse {
                        matched: false,
                        message: None,
                    }));
                }
            }
        }
    }

    #[tool(
        description = "Read messages from a channel — use on agent startup to pick up hand-offs directed at you. Filters: channel, sender, kind, since, limit. When `channel` is provided, the response also includes presence (active_users)."
    )]
    async fn list_messages(
        &self,
        Parameters(params): Parameters<ListMessagesParams>,
    ) -> Result<String, String> {
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

        let payloads: Vec<MessagePayload> = rows.into_iter().map(Into::into).collect();
        Ok(format_messages_markdown(
            &payloads,
            total,
            params.channel.as_deref(),
        ))
    }

    #[tool(
        description = "List all known channels with message counts and last activity timestamp."
    )]
    async fn list_channels(&self) -> Result<String, String> {
        let messages = self.state.messages.clone();
        let channels = tokio::task::spawn_blocking(move || messages.list_channels())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(format_channels_markdown(&channels))
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

    #[tool(
        description = "Assemble the agent-harness graph: all typed harness nodes (harness_doc/plan/sprint/todo/agent/stream/audit) plus the manual edges between them, with the latest AUDITED verdict and status badge (green/yellow/red) resolved per node. Use to navigate Plan→Sprint→Todo hierarchies and find invariant collisions. Pass `source_id` to scope to one namespace."
    )]
    async fn harness_tree(
        &self,
        Parameters(query): Parameters<crate::api::harness::HarnessTreeQuery>,
    ) -> Result<Json<crate::api::harness::HarnessTreeResponse>, String> {
        crate::api::harness::harness_tree_core(&self.state, query.source_id)
            .await
            .map(Json)
            .map_err(stringify_api_error)
    }

    #[tool(
        description = "Automatically ingest and structure a repository document (Architecture Overview, ADR, Spec, Invariant, or Guide) into the harness graph. Automatically provisions the harness_repo root node if missing, extracts markdown sections and summaries, enforces the harness_doc schema, and creates the appropriate manual graph edges (repo contains doc, doc part_of parent, parent contains doc, doc supersedes previous)."
    )]
    async fn ingest_doc(
        &self,
        Parameters(req): Parameters<crate::api::harness::IngestDocRequest>,
    ) -> Result<Json<crate::api::harness::IngestDocResponse>, String> {
        crate::api::harness::ingest_doc_core(&self.state, req)
            .await
            .map(Json)
            .map_err(stringify_api_error)
    }

    #[tool(
        description = "Attach a remote file (HTTP/HTTPS) to an existing entry. Server fetches the URL with SSRF guards (private-IP block, size + time caps, redirect re-check). Returns the new attachment id and a /assets/* URL."
    )]
    async fn attach_url(
        &self,
        Parameters(request): Parameters<crate::api::attachments::AttachUrlRequest>,
    ) -> Result<Json<crate::api::attachments::AttachmentSummary>, String> {
        crate::api::attachments::attach_from_url_core(&self.state, request)
            .await
            .map(Json)
            .map_err(stringify_api_error)
    }

    #[tool(description = "List every file attached to an entry, newest first.")]
    async fn list_attachments(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<crate::api::attachments::AttachmentsResponse>, String> {
        let store = self.state.store.clone();
        let target = id.clone();
        let records = tokio::task::spawn_blocking(move || store.list_attachments_for_item(&target))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(Json(crate::api::attachments::AttachmentsResponse {
            attachments: records.into_iter().map(Into::into).collect(),
        }))
    }

    #[tool(
        description = "Delete an attachment by id. Removes both the database row and the on-disk file."
    )]
    async fn delete_attachment(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        crate::api::attachments::delete_attachment_core(&self.state, &id)
            .await
            .map_err(stringify_api_error)?;
        Ok(Json(DeleteResponse { id, deleted: true }))
    }

    #[tool(
        description = "Browse entries hierarchically by wiki path. Returns direct child path segments under `prefix` (or top-level when omitted) plus any leaf entries whose path equals `prefix`. Always scoped by `source_id`."
    )]
    async fn list_entry_tree(
        &self,
        Parameters(query): Parameters<crate::api::attachments::EntriesTreeQuery>,
    ) -> Result<Json<crate::api::attachments::EntriesTreeResponse>, String> {
        crate::api::attachments::entries_tree_core(&self.state, query)
            .await
            .map(Json)
            .map_err(stringify_api_error)
    }

    #[tool(
        description = "List every registered typed-entry schema. Each row carries the type_name, the JSON Schema, optional title/description, and item_count (how many entries are currently typed). Use to discover what mini-app types are available before calling store_entry with a `type`."
    )]
    async fn list_schemas(&self) -> Result<Json<crate::api::schemas::SchemaListResponse>, String> {
        let store = self.state.store.clone();
        let pairs = tokio::task::spawn_blocking(
            move || -> anyhow::Result<Vec<(crate::db::SchemaRecord, i64)>> {
                let records = store.list_schemas()?;
                let mut out = Vec::with_capacity(records.len());
                for record in records {
                    let count = store.count_items_by_type(&record.type_name).unwrap_or(0);
                    out.push((record, count));
                }
                Ok(out)
            },
        )
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        let schemas = pairs
            .into_iter()
            .map(|(record, count)| {
                crate::api::schemas::SchemaPayload::from_record_pub(record, Some(count))
            })
            .collect();
        Ok(Json(crate::api::schemas::SchemaListResponse { schemas }))
    }

    #[tool(
        description = "Fetch one typed-entry schema by `type_name`. Returns the JSON Schema plus metadata. Returns an error when the type is not registered."
    )]
    async fn get_schema(
        &self,
        Parameters(params): Parameters<SchemaTypeParams>,
    ) -> Result<Json<crate::api::schemas::SchemaPayload>, String> {
        let store = self.state.store.clone();
        let tn = params.type_name.clone();
        let pair = tokio::task::spawn_blocking(
            move || -> anyhow::Result<Option<(crate::db::SchemaRecord, i64)>> {
                let Some(record) = store.get_schema(&tn)? else {
                    return Ok(None);
                };
                let count = store.count_items_by_type(&record.type_name).unwrap_or(0);
                Ok(Some((record, count)))
            },
        )
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("schema `{}` not found", params.type_name))?;
        Ok(Json(crate::api::schemas::SchemaPayload::from_record_pub(
            pair.0,
            Some(pair.1),
        )))
    }

    #[tool(
        description = "Register or update a typed-entry schema. Supply `type_name`, the `json_schema` (Draft 2020-12 / Draft-07 compatible), and optional `title` / `description`. The schema itself is validated as a JSON Schema before storage. The compiled validator cache for that type is invalidated; subsequent store_entry / update_item calls revalidate against the new schema."
    )]
    async fn upsert_schema(
        &self,
        Parameters(params): Parameters<UpsertSchemaParams>,
    ) -> Result<Json<crate::api::schemas::SchemaPayload>, String> {
        crate::validation::validate_meta_schema(&params.json_schema).map_err(|e| e.to_string())?;
        let record = crate::db::SchemaRecord {
            type_name: params.type_name.clone(),
            json_schema: params.json_schema,
            title: params.title,
            description: params.description,
            created_at: 0,
            updated_at: 0,
        };
        let store = self.state.store.clone();
        let to_store = record.clone();
        let tn = params.type_name.clone();
        let count = tokio::task::spawn_blocking(move || -> anyhow::Result<i64> {
            store.upsert_schema(to_store)?;
            Ok(store.count_items_by_type(&tn).unwrap_or(0))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        self.state.schema_cache.invalidate(&params.type_name);
        Ok(Json(crate::api::schemas::SchemaPayload::from_record_pub(
            record,
            Some(count),
        )))
    }

    #[tool(
        description = "Delete a typed-entry schema. Refuses (returns an error) when items still reference the type, unless `force=true` — in which case those items have their `type` and `data` cleared. Returns deleted=true and items_unset count."
    )]
    async fn delete_schema(
        &self,
        Parameters(params): Parameters<DeleteSchemaParams>,
    ) -> Result<Json<crate::api::schemas::DeleteSchemaResponse>, String> {
        let force = params.force.unwrap_or(false);
        let store = self.state.store.clone();
        let tn = params.type_name.clone();
        let (deleted, unset) = tokio::task::spawn_blocking(move || store.delete_schema(&tn, force))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        if !deleted {
            return Err(format!("schema `{}` not found", params.type_name));
        }
        self.state.schema_cache.invalidate(&params.type_name);
        Ok(Json(crate::api::schemas::DeleteSchemaResponse {
            type_name: params.type_name,
            deleted,
            items_unset: unset,
        }))
    }

    #[tool(
        description = "Manually trigger a dreaming round to consolidate 'memory' entries. Moves durable facts to 'knowledge', merges duplicates, and prunes transient notes. Returns accepted status immediately; work continues in background."
    )]
    async fn dream(&self) -> Result<String, String> {
        if !self.state.analysis.is_configured() {
            return Err("dreaming requires analysis (LLM) to be configured".to_owned());
        }
        let state = self.state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::api::process_dreaming_round(&state).await {
                tracing::error!("mcp dream error: {e}");
            }
        });
        Ok("Dreaming round started in background.".to_owned())
    }

    // --- Google integrations: kept for the HTTP API, not listed as MCP tools. ---

    /// Implemented but intentionally not exposed as an MCP tool; Google
    /// integrations stay reachable via the HTTP API (`/api/integrations/google/*`).
    #[allow(dead_code)]
    async fn drive_search(
        &self,
        Parameters(params): Parameters<DriveSearchParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<crate::integrations::google::drive::SearchResult>, String> {
        let subject = extract_subject(&ctx)?;
        let client = crate::integrations::google::GoogleClient::for_subject(&self.state, &subject)
            .await
            .map_err(|e| e.to_string())?;
        crate::integrations::google::drive::search(
            &client,
            &params.query,
            params.page_size.unwrap_or(20),
            params.mime_type.as_deref(),
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    /// Implemented but intentionally not exposed as an MCP tool; Google
    /// integrations stay reachable via the HTTP API (`/api/integrations/google/*`).
    #[allow(dead_code)]
    async fn drive_fetch(
        &self,
        Parameters(params): Parameters<DriveFetchParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<crate::integrations::google::drive::FetchedDoc>, String> {
        let subject = extract_subject(&ctx)?;
        let client = crate::integrations::google::GoogleClient::for_subject(&self.state, &subject)
            .await
            .map_err(|e| e.to_string())?;
        crate::integrations::google::drive::fetch(&client, &params.file_id)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Implemented but intentionally not exposed as an MCP tool; Google
    /// integrations stay reachable via the HTTP API (`/api/integrations/google/*`).
    #[allow(dead_code)]
    async fn gmail_search(
        &self,
        Parameters(params): Parameters<GmailSearchParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<crate::integrations::google::gmail::SearchResult>, String> {
        let subject = extract_subject(&ctx)?;
        let client = crate::integrations::google::GoogleClient::for_subject(&self.state, &subject)
            .await
            .map_err(|e| e.to_string())?;
        crate::integrations::google::gmail::search(
            &client,
            &params.query,
            params.page_size.unwrap_or(20),
            params.page_token.as_deref(),
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    /// Implemented but intentionally not exposed as an MCP tool; Google
    /// integrations stay reachable via the HTTP API (`/api/integrations/google/*`).
    #[allow(dead_code)]
    async fn gmail_get_thread(
        &self,
        Parameters(params): Parameters<GmailGetThreadParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<crate::integrations::google::gmail::FetchedThread>, String> {
        let subject = extract_subject(&ctx)?;
        let client = crate::integrations::google::GoogleClient::for_subject(&self.state, &subject)
            .await
            .map_err(|e| e.to_string())?;
        crate::integrations::google::gmail::get_thread(&client, &params.thread_id)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Send a Web Push notification to a user's registered devices. Defaults to the caller's subject if `subject` is omitted — that's the right shape for self-reminders. Pass `subject` explicitly to notify a different user (the call still needs the appropriate auth). Use `tag` to deduplicate noisy notifications (re-using a tag replaces the earlier one); `url` is the deep-link the SW opens on click. Returns per-subscription delivery stats, including how many dead subscriptions were pruned. Requires the user to have subscribed via /settings/integrations."
    )]
    async fn notify_user(
        &self,
        Parameters(params): Parameters<NotifyUserParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<crate::notify::SendResult>, String> {
        let caller = extract_subject(&ctx).ok();
        let target = params
            .subject
            .clone()
            .or(caller)
            .ok_or_else(|| "no target subject (omit only when authenticated)".to_owned())?;
        let payload = crate::notify::NotificationPayload {
            title: params.title,
            body: params.body,
            url: params.url,
            tag: params.tag,
            ttl_secs: params.ttl_secs,
            urgency: params.urgency,
            data: params.data,
        };
        crate::notify::send(&self.state, &target, &payload)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}


#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DriveSearchParams {
    /// Free-text query passed to Drive's `fullText contains` operator.
    pub query: String,
    /// 1-100, default 20.
    #[serde(default)]
    pub page_size: Option<u32>,
    /// Optional MIME-type filter, e.g. `application/vnd.google-apps.document`.
    #[serde(default)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DriveFetchParams {
    /// Drive file id (the `id` field from `drive_search` results).
    pub file_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GmailSearchParams {
    /// Gmail query, same operators as the Gmail search bar.
    pub query: String,
    /// 1-100, default 20.
    #[serde(default)]
    pub page_size: Option<u32>,
    /// `nextPageToken` from a previous response to paginate.
    #[serde(default)]
    pub page_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GmailGetThreadParams {
    /// Thread id (the `thread_id` field from `gmail_search` results).
    pub thread_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NotifyUserParams {
    /// Target subject. Omit to send to the calling subject (self-reminder).
    #[serde(default)]
    pub subject: Option<String>,
    pub title: String,
    pub body: String,
    /// Click-through URL the service worker opens when the notification
    /// is clicked.
    #[serde(default)]
    pub url: Option<String>,
    /// Tag for replacing earlier notifications with the same tag.
    #[serde(default)]
    pub tag: Option<String>,
    /// TTL in seconds the push service holds the message if the device
    /// is offline. Default 3600.
    #[serde(default)]
    pub ttl_secs: Option<u32>,
    /// "very-low" | "low" | "normal" (default) | "high".
    #[serde(default)]
    pub urgency: Option<String>,
    /// Optional structured data forwarded verbatim to the service worker.
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub data: Option<serde_json::Value>,
}

/// Pull the authenticated subject out of the MCP request context. The HTTP
/// transport injects `http::request::Parts` into the request extensions, and
/// `require_api_key` middleware upstream sets `SessionSubject` on the
/// underlying axum request. Returns a user-facing error string when no
/// subject is present (anonymous caller or missing extension).
fn extract_subject(ctx: &RequestContext<RoleServer>) -> Result<String, String> {
    let parts = ctx
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| "no http request context".to_owned())?;
    let subject = parts
        .extensions
        .get::<crate::api::SessionSubject>()
        .and_then(|s| s.0.clone())
        .ok_or_else(|| "google integration requires an authenticated subject".to_owned())?;
    Ok(subject)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SchemaTypeParams {
    pub type_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpsertSchemaParams {
    pub type_name: String,
    #[schemars(schema_with = "metadata_schema")]
    pub json_schema: serde_json::Value,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}


#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeleteSchemaParams {
    pub type_name: String,
    #[serde(default)]
    pub force: Option<bool>,
}

/// Filter predicate for `wait_for_message`. All supplied filters must match.
fn message_matches(
    record: &crate::db::MessageRecord,
    channel: &str,
    sender: Option<&str>,
    kind: Option<&str>,
    contains: Option<&str>,
    metadata_match: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if record.channel != channel {
        return false;
    }
    if let Some(s) = sender {
        if record.sender != s {
            return false;
        }
    }
    if let Some(k) = kind {
        if record.kind != k {
            return false;
        }
    }
    if let Some(needle) = contains {
        if !record.text.contains(needle) {
            return false;
        }
    }
    if let Some(expected) = metadata_match {
        let actual = match record.metadata.as_object() {
            Some(m) => m,
            None => return false,
        };
        for (k, v) in expected {
            if actual.get(k) != Some(v) {
                return false;
            }
        }
    }
    true
}

pub(crate) fn stringify_api_error(error: crate::api::ApiError) -> String {
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
                updated_at: related.created_at, // Related hit might not have updated_at in its payload yet
                distance: related.distance,
                chunk_context: None,
                section_path: Vec::new(),
                retrievers: Vec::new(),
                path: None,
                analysis: None,
                type_name: None,
                repo: related.repo.clone(),
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
    let repo_str = hit
        .repo
        .as_deref()
        .map(|r| format!(" [repo: {r}]"))
        .unwrap_or_default();
    let path_str = hit
        .path
        .as_deref()
        .map(|p| format!(" (path: {p})"))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "\n### {index}. `{id}` — {relevance}% [{source}]{repo_str}{path_str}{suffix}",
        id = hit.id,
        source = hit.source_id,
    );
    let _ = writeln!(
        out,
        "> Created: {} | Updated: {}",
        format_ms(hit.created_at),
        format_ms(hit.updated_at)
    );
    let _ = writeln!(out, "\n{}", hit.text.trim());
}

fn format_ms(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_item_markdown(item: &AdminItemPayload) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Entry: `{}`", item.id);
    let _ = writeln!(out, "\n- **Source**: `{}`", item.source_id);
    if let Some(path) = &item.path {
        let _ = writeln!(out, "- **Path**: `{}`", path);
    }
    if let Some(type_name) = &item.type_name {
        let _ = writeln!(out, "- **Type**: `{}`", type_name);
    }
    let _ = writeln!(out, "- **Created**: {}", format_ms(item.created_at));
    let _ = writeln!(out, "- **Updated**: {}", format_ms(item.updated_at));

    let tags: Vec<&str> = item
        .metadata
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    if !tags.is_empty() {
        let _ = writeln!(out, "- **Tags**: {}", tags.join(", "));
    }

    if let Some(data) = &item.data {
        let _ = writeln!(out, "\n## Data (JSON)");
        let _ = writeln!(
            out,
            "```json\n{}\n```",
            serde_json::to_string_pretty(data).unwrap_or_default()
        );
    }

    let _ = writeln!(out, "\n## Content\n\n{}", item.text.trim());

    if let Some(analysis) = &item.analysis {
        let _ = writeln!(out, "\n## Intelligence Analysis");
        if let Some(model) = &item.analysis_model {
            let _ = writeln!(out, "> Analyzed by `{}`", model);
        }
        let _ = writeln!(
            out,
            "\n```json\n{}\n```",
            serde_json::to_string_pretty(analysis).unwrap_or_default()
        );
    }

    out
}

fn format_item_list_markdown(
    items: &[ItemRecord],
    total: i64,
    _limit: usize,
    offset: usize,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Browse Entries (Total: {})", total);
    let _ = writeln!(
        out,
        "> Showing results {}-{} of {}",
        offset + 1,
        offset + items.len(),
        total
    );

    if items.is_empty() {
        let _ = writeln!(out, "\nNo entries found.");
        return out;
    }

    let _ = writeln!(out, "\n| ID | Path | Type | Updated |");
    let _ = writeln!(out, "|----|------|------|---------|");

    for item in items {
        let path = item.path.as_deref().unwrap_or("-");
        let type_name = item.type_name.as_deref().unwrap_or("-");
        let updated = format_ms(item.updated_at);
        let _ = writeln!(
            out,
            "| `{}` | `{}` | `{}` | {} |",
            item.id, path, type_name, updated
        );
    }

    let _ = writeln!(
        out,
        "\n*Use `get_entry(id)` to see full content and metadata for a specific item.*"
    );

    out
}

fn format_messages_markdown(
    messages: &[MessagePayload],
    total: i64,
    channel: Option<&str>,
) -> String {
    let mut out = String::new();
    let chan_suffix = channel.map(|c| format!(" in `{}`", c)).unwrap_or_default();
    let _ = writeln!(out, "# Messages{} (Total: {})", chan_suffix, total);

    if messages.is_empty() {
        let _ = writeln!(out, "\nNo messages found.");
        return out;
    }

    for m in messages {
        let ts = format_ms(m.created_at);
        let _ = writeln!(out, "\n---");
        let _ = writeln!(out, "**{}** [{:?}] ({})", m.sender, m.sender_kind, ts);
        let _ = writeln!(out, "\n{}", m.text.trim());
    }

    out
}

fn format_channels_markdown(channels: &[crate::db::ChannelSummary]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Channels");

    if channels.is_empty() {
        let _ = writeln!(out, "\nNo channels found.");
        return out;
    }

    let _ = writeln!(out, "\n| Channel | Messages | Last Activity |");
    let _ = writeln!(out, "|---------|----------|---------------|");

    for c in channels {
        let last = format_ms(c.last_message_at);
        let _ = writeln!(out, "| `{}` | {} | {} |", c.channel, c.message_count, last);
    }

    out
}

fn format_categories_markdown(categories: &[crate::db::CategorySummary]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Categories (Sources)");

    if categories.is_empty() {
        let _ = writeln!(out, "\nNo categories found.");
        return out;
    }

    let _ = writeln!(out, "\n| Source ID | Item Count |");
    let _ = writeln!(out, "|-----------|------------|");

    for c in categories {
        let _ = writeln!(out, "| `{}` | {} |", c.source_id, c.item_count);
    }

    out
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
        .with_sse_keep_alive(Some(Duration::from_secs(15)))
        .with_stateful_mode(false)
        .with_json_response(true);
    StreamableHttpService::new(
        move || Ok(RustRagMcpServer::new(factory_state.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}
