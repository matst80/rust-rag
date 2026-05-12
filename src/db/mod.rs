mod auth;
mod graph;
pub mod postgres;
mod schema;

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use graph::{
    list_graph_edges_internal, list_pairwise_distances_for_ids, rebuild_similarity_graph_locked,
};
use schema::{initialize_schema, register_sqlite_vec};

/// Reciprocal Rank Fusion ranking constant. 60 is the value from the original
/// RRF paper (Cormack et al.) and is the de-facto default across search systems.
const RRF_K: f32 = 60.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ItemRecord {
    pub id: String,
    pub text: String,
    pub metadata: Value,
    pub source_id: String,
    pub created_at: i64,
    /// Wiki-style hierarchical path (slash-separated, e.g. `team/handbook`).
    /// User-asserted, optional. Distinct from chunk-level `section_path`
    /// which is derived from markdown headers by the chunker.
    /// Normalized via `normalize_path` before persistence.
    pub path: Option<String>,
    /// Optional structured-data type name. References `schemas.type_name`
    /// when set. NULL = untyped legacy entry.
    pub type_name: Option<String>,
    /// Typed payload validated against the schema for `type_name`.
    pub data: Option<Value>,
}

/// Registered JSON Schema entry. Used to validate typed entries' `data`
/// payloads on store/update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SchemaRecord {
    pub type_name: String,
    pub json_schema: Value,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Persisted LLM-on-store analysis for an entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemAnalysisRecord {
    pub analysis: Value,
    pub analysis_at: i64,
    pub analysis_model: String,
}

/// One (source_id, path) pair with entry count. Returned by `list_all_paths`
/// for bulk wiki-tree rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct PathRow {
    pub source_id: String,
    pub path: String,
    pub count: i64,
}

/// One direct child segment under a wiki path prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct PathChild {
    /// Segment text (the part of the path immediately after `prefix`).
    pub segment: String,
    /// Number of entries whose path equals `prefix/segment` or sits under it.
    pub count: i64,
    /// Whether any descendants exist beyond this segment (i.e. there are
    /// entries deeper than `prefix/segment`). Lets the UI know a folder has
    /// further sub-folders without a second round-trip.
    pub has_children: bool,
}

/// File bound to an entry. Disk lives under `RAG_UPLOAD_PATH/<stored_name>`.
/// Cascade-deleted with the parent item; on-disk file removal is the API
/// layer's responsibility (see `delete_item` return + `delete_attachment`).
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentRecord {
    pub id: String,
    pub item_id: String,
    pub filename: Option<String>,
    pub stored_name: String,
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub sha256: Option<String>,
    pub created_at: i64,
}

/// Normalize a wiki-style path. Trims surrounding slashes, collapses `//`,
/// rejects `..` traversal and absolute paths. Empty -> `None`.
pub fn normalize_path(input: &str) -> Result<Option<String>> {
    let trimmed = input.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut segments = Vec::new();
    for seg in trimmed.split('/') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        if seg == ".." || seg == "." {
            anyhow::bail!("path segment '{seg}' not allowed");
        }
        segments.push(seg);
    }
    if segments.is_empty() {
        Ok(None)
    } else {
        Ok(Some(segments.join("/")))
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchHit {
    pub id: String,
    pub text: String,
    pub metadata: Value,
    pub source_id: String,
    pub created_at: i64,
    pub distance: f32,
    /// Header breadcrumb of the chunk that scored best for this document
    /// (e.g. `["Architecture", "Embedding execution"]`). Empty for hits
    /// whose chunks had no preceding markdown headers, or for stores that
    /// don't track section paths.
    pub section_path: Vec<String>,
    /// Which retrievers contributed to ranking this hit. `["dense"]` for
    /// dense-only / non-hybrid search, `["dense","sparse"]` when hybrid
    /// fusion saw the hit on both sides, `["sparse"]` when only sparse
    /// matched. Helps the UI show *why* something ranked.
    pub retrievers: Vec<String>,
    /// Text of the single best-matching chunk for this document, set by
    /// stores that track chunks (postgres). Used by the reranker so it
    /// scores the actual matched passage instead of the whole document.
    /// `None` on stores that don't chunk or paths that don't surface it
    /// (e.g. `distances_for_ids`); reranker falls back to `text`.
    pub chunk_text: Option<String>,
    /// User-asserted wiki path inherited from the parent entry. `None` if
    /// unset. Distinct from `section_path` (chunker-derived from headers).
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CategorySummary {
    pub source_id: String,
    pub item_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphConfig {
    pub enabled: bool,
    pub build_on_startup: bool,
    pub similarity_top_k: usize,
    pub similarity_max_distance: f32,
    pub cross_source: bool,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            build_on_startup: false,
            similarity_top_k: 5,
            similarity_max_distance: 0.75,
            cross_source: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GraphEdgeType {
    Similarity,
    Manual,
}

impl GraphEdgeType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Similarity => "similarity",
            Self::Manual => "manual",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self> {
        match value {
            "similarity" => Ok(Self::Similarity),
            "manual" => Ok(Self::Manual),
            other => anyhow::bail!("unsupported graph edge type {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdgeRecord {
    pub id: String,
    pub from_item_id: String,
    pub to_item_id: String,
    pub edge_type: GraphEdgeType,
    pub relation: Option<String>,
    pub weight: f32,
    pub directed: bool,
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNeighborhood {
    pub center_id: String,
    pub nodes: Vec<ItemRecord>,
    pub edges: Vec<GraphEdgeRecord>,
    pub pairwise_distances: Vec<GraphNodeDistance>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNodeDistance {
    pub from_item_id: String,
    pub to_item_id: String,
    pub distance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphStatus {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTokenRecord {
    pub id: String,
    pub name: String,
    pub subject: Option<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMcpToken {
    pub id: String,
    pub token_hash: String,
    pub name: String,
    pub subject: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAuthStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

impl DeviceAuthStatus {
    pub(crate) fn from_str(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "denied" => Ok(Self::Denied),
            "expired" => Ok(Self::Expired),
            other => anyhow::bail!("unsupported device auth status {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAuthRecord {
    pub device_code: String,
    pub user_code: String,
    pub status: DeviceAuthStatus,
    pub token_id: Option<String>,
    pub subject: Option<String>,
    pub client_name: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub interval_secs: i64,
    pub last_polled_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDeviceAuth {
    pub device_code: String,
    pub user_code: String,
    pub client_name: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub interval_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOAuthAuthCode {
    pub code: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub challenge_method: String,
    pub scope: Option<String>,
    pub subject: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAuthCodeRecord {
    pub code: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub challenge_method: String,
    pub scope: Option<String>,
    pub subject: Option<String>,
    pub token_id: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
}

pub trait AuthStore: Send + Sync {
    fn create_mcp_token(&self, token: NewMcpToken) -> Result<McpTokenRecord>;
    fn find_mcp_token_by_hash(&self, hash: &str) -> Result<Option<McpTokenRecord>>;
    fn touch_mcp_token(&self, id: &str, now: i64) -> Result<()>;
    fn list_mcp_tokens(&self, subject: Option<&str>) -> Result<Vec<McpTokenRecord>>;
    fn delete_mcp_token(&self, id: &str, subject: Option<&str>) -> Result<bool>;

    fn create_device_auth(&self, request: NewDeviceAuth) -> Result<DeviceAuthRecord>;
    fn find_device_auth_by_device_code(
        &self,
        device_code: &str,
    ) -> Result<Option<DeviceAuthRecord>>;
    fn find_device_auth_by_user_code(&self, user_code: &str) -> Result<Option<DeviceAuthRecord>>;
    fn approve_device_auth(
        &self,
        user_code: &str,
        token_id: &str,
        subject: Option<&str>,
        now: i64,
    ) -> Result<bool>;
    fn touch_device_poll(&self, device_code: &str, now: i64) -> Result<()>;
    fn expire_device_auths(&self, now: i64) -> Result<usize>;

    fn create_auth_code(&self, code: NewOAuthAuthCode) -> Result<OAuthAuthCodeRecord>;
    fn find_auth_code(&self, code: &str) -> Result<Option<OAuthAuthCodeRecord>>;
    /// Atomically mark an authorization code consumed and bind a minted
    /// `token_id` to it. Returns `true` if the row was updated (still
    /// pending), `false` if already consumed (replay attempt).
    fn consume_auth_code(&self, code: &str, token_id: &str, now: i64) -> Result<bool>;
    fn expire_auth_codes(&self, now: i64) -> Result<usize>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self::Desc
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListItemsRequest {
    pub source_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_order: SortOrder,
    pub metadata_filter: HashMap<String, String>,
    pub min_created_at: Option<i64>,
    pub max_created_at: Option<i64>,
    /// Restrict to entries whose `path` equals this prefix or sits under it
    /// (`prefix` itself or `prefix/...`). Already-normalized when set; empty
    /// is treated as `None`. Comparison is case-insensitive.
    pub path_prefix: Option<String>,
    /// Restrict to entries whose `type` equals this value.
    pub type_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManualEdgeInput {
    pub from_item_id: String,
    pub to_item_id: String,
    pub relation: Option<String>,
    pub weight: f32,
    pub directed: bool,
    pub metadata: Value,
}

/// How many recent search events to average when rebuilding an interest profile.
pub const PROFILE_EVENTS_WINDOW: usize = 30;
/// Rebuild the interest profile after this many new search events.
pub const PROFILE_REFRESH_AFTER: i64 = 5;

#[derive(Debug, Clone)]
pub struct NewUserEvent {
    pub id: String,
    pub subject: String,
    pub event_type: UserEventType,
    pub query: Option<String>,
    pub query_embedding: Option<Vec<f32>>,
    pub item_ids: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserEventType {
    Search,
    Store,
    Chat,
}

impl UserEventType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Store => "store",
            Self::Chat => "chat",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub subject: String,
    pub interest_embedding: Option<Vec<f32>>,
    pub event_horizon: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MessageSenderKind {
    Human,
    Agent,
    System,
}

impl Default for MessageSenderKind {
    fn default() -> Self {
        Self::Human
    }
}

impl MessageSenderKind {
    pub fn as_serialized(self) -> &'static str {
        self.as_str()
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::System => "system",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self> {
        match value {
            "human" => Ok(Self::Human),
            "agent" => Ok(Self::Agent),
            "system" => Ok(Self::System),
            other => anyhow::bail!("unsupported sender_kind {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageRecord {
    pub id: String,
    pub channel: String,
    pub sender: String,
    pub sender_kind: MessageSenderKind,
    pub text: String,
    pub kind: String,
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct MessageUpdate {
    pub text: Option<String>,
    pub metadata: Option<Value>,
    /// Append `text` to the existing body instead of replacing it. Useful for
    /// streaming agent chunks into a single message row.
    pub append_text: bool,
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub id: String,
    pub channel: String,
    pub sender: String,
    pub sender_kind: MessageSenderKind,
    pub text: String,
    pub kind: String,
    pub metadata: Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct MessageQuery {
    pub channel: Option<String>,
    pub sender: Option<String>,
    pub kind: Option<String>,
    pub min_created_at: Option<i64>,
    pub max_created_at: Option<i64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_order: SortOrder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSummary {
    pub channel: String,
    pub message_count: i64,
    pub last_message_at: i64,
}

pub trait MessageStore: Send + Sync {
    fn get_message(&self, id: &str) -> Result<Option<MessageRecord>>;
    fn send_message(&self, message: NewMessage) -> Result<MessageRecord>;
    fn update_message(
        &self,
        id: &str,
        update: MessageUpdate,
        now: i64,
    ) -> Result<Option<MessageRecord>>;
    fn delete_message(&self, id: &str) -> Result<Option<MessageRecord>>;
    /// Find permission_request rows whose metadata.request_id == `request_id`,
    /// returning the matching rows so the caller can delete + emit tombstones.
    fn find_permission_request(&self, request_id: &str) -> Result<Vec<MessageRecord>>;
    fn list_channel_messages(&self, channel: &str) -> Result<Vec<MessageRecord>>;
    /// Delete every message in `channel`. Returns the wiped rows so the caller
    /// can emit tombstones for in-flight long-poll listeners.
    fn clear_channel(&self, channel: &str) -> Result<Vec<MessageRecord>>;
    fn list_messages(&self, query: MessageQuery) -> Result<(Vec<MessageRecord>, i64)>;
    fn list_channels(&self) -> Result<Vec<ChannelSummary>>;
}

pub trait UserMemoryStore: Send + Sync {
    fn log_user_event(&self, event: NewUserEvent) -> Result<()>;
    fn touch_item_accesses(&self, item_ids: &[String], now: i64) -> Result<()>;
    fn get_user_profile(&self, subject: &str) -> Result<Option<UserProfile>>;
    fn upsert_user_profile(&self, profile: UserProfile) -> Result<()>;
    fn get_recent_query_embeddings(
        &self,
        subject: &str,
        limit: usize,
    ) -> Result<Vec<Vec<f32>>>;
    fn count_events_since(&self, subject: &str, horizon: i64) -> Result<i64>;
}

/// One indexed chunk of a parent document. Used by `upsert_document` so the
/// store can write a single document row with N chunk rows attached.
#[derive(Debug, Clone)]
pub struct DocChunk {
    pub position: i32,
    pub content: String,
    pub embedding: Vec<f32>,
    /// Header breadcrumb in effect at the chunk's start (e.g.
    /// `["Architecture", "Embedding execution"]`). Empty for non-markdown
    /// or pre-heading content. Maps to `chunks.section_path TEXT[]`.
    pub section_path: Vec<String>,
    /// bge-m3 sparse output as `(vocab_id, weight)` pairs after per-token
    /// aggregation. `None` when the backend doesn't produce sparse (slice 1
    /// keeps this `None` everywhere; slice 2 wires the sparse head).
    pub sparse: Option<Vec<(u32, f32)>>,
}

pub trait VectorStore: Send + Sync {
    fn upsert_item(&self, item: ItemRecord, embedding: &[f32]) -> Result<()>;

    /// Write `item` plus N chunks. Default impl falls back to single-chunk
    /// `upsert_item` using the first chunk's embedding — adequate for stores
    /// that don't have a chunks table (e.g. SQLite). Postgres overrides to
    /// write all chunks.
    fn upsert_document(&self, item: ItemRecord, chunks: Vec<DocChunk>) -> Result<()> {
        let first = chunks
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("upsert_document called with no chunks"))?;
        self.upsert_item(item, &first.embedding)
    }

    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>>;
    fn search_hybrid(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        query_sparse: &[(u32, f32)],
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>>;
    /// Persist analysis JSON + model name onto an existing item. Best-effort;
    /// non-existent ids should return `Ok(false)`.
    fn update_item_analysis(&self, _id: &str, _json: &str, _model: &str) -> Result<bool> {
        anyhow::bail!("update_item_analysis not supported by this store")
    }
    /// Fetch persisted analysis for an item. `Ok(None)` when item lacks one.
    fn get_item_analysis(&self, _id: &str) -> Result<Option<ItemAnalysisRecord>> {
        Ok(None)
    }
    fn list_categories(&self) -> Result<Vec<CategorySummary>>;
    fn list_items(&self, request: ListItemsRequest) -> Result<(Vec<ItemRecord>, i64)>;
    fn get_item(&self, id: &str) -> Result<Option<ItemRecord>>;
    fn delete_item(&self, id: &str) -> Result<bool>;

    /// Insert an attachment row. Caller persists the file to disk first.
    fn insert_attachment(&self, _record: AttachmentRecord) -> Result<()> {
        anyhow::bail!("attachments not supported by this store")
    }
    /// Fetch every attachment bound to `item_id`, newest first.
    fn list_attachments_for_item(&self, _item_id: &str) -> Result<Vec<AttachmentRecord>> {
        anyhow::bail!("attachments not supported by this store")
    }
    /// Look up a single attachment by id.
    fn get_attachment(&self, _id: &str) -> Result<Option<AttachmentRecord>> {
        anyhow::bail!("attachments not supported by this store")
    }
    /// Remove an attachment row. Returns the stored on-disk filename so the
    /// API layer can `fs::remove_file` after the row is gone. `Ok(None)` when
    /// the id was not found.
    fn delete_attachment(&self, _id: &str) -> Result<Option<String>> {
        anyhow::bail!("attachments not supported by this store")
    }
    /// Direct child path segments for tree navigation. Each row is the next
    /// path segment after `prefix` plus a count of entries that share the
    /// prefix (recursive). When `prefix` is `None`, top-level segments are
    /// returned. Always scoped by `source_id`. Comparison is case-insensitive.
    fn list_path_children(
        &self,
        _source_id: &str,
        _prefix: Option<&str>,
    ) -> Result<Vec<PathChild>> {
        anyhow::bail!("path tree not supported by this store")
    }

    /// Every distinct (source_id, path) with entry count. Lets the wiki
    /// sidebar render the full tree from a single round-trip. When
    /// `source_id_filter` is `Some`, scoped to one namespace.
    fn list_all_paths(
        &self,
        _source_id_filter: Option<&str>,
    ) -> Result<Vec<PathRow>> {
        anyhow::bail!("path tree not supported by this store")
    }
    fn distances_for_ids(&self, query_embedding: &[f32], ids: &[String]) -> Result<Vec<SearchHit>>;
    fn graph_status(&self) -> Result<GraphStatus>;
    fn graph_neighborhood(
        &self,
        center_id: &str,
        depth: usize,
        limit: usize,
        edge_type: Option<GraphEdgeType>,
    ) -> Result<GraphNeighborhood>;
    fn list_graph_edges(
        &self,
        item_id: Option<&str>,
        edge_type: Option<GraphEdgeType>,
    ) -> Result<Vec<GraphEdgeRecord>>;
    fn rebuild_similarity_graph(&self) -> Result<usize>;
    fn add_manual_edge(&self, input: ManualEdgeInput) -> Result<GraphEdgeRecord>;
    fn delete_graph_edge(&self, id: &str) -> Result<bool>;
    fn get_items_pending_ontology(&self, limit: usize) -> Result<Vec<ItemRecord>>;
    fn mark_ontology_status(&self, id: &str, status: &str) -> Result<()>;

    /// Typed-entry schemas. CRUD on the `schemas` table holding JSON Schema
    /// definitions referenced by `items.type`.
    fn list_schemas(&self) -> Result<Vec<SchemaRecord>> {
        anyhow::bail!("schemas not supported by this store")
    }
    fn get_schema(&self, _type_name: &str) -> Result<Option<SchemaRecord>> {
        anyhow::bail!("schemas not supported by this store")
    }
    fn upsert_schema(&self, _record: SchemaRecord) -> Result<()> {
        anyhow::bail!("schemas not supported by this store")
    }
    /// Returns `(deleted, items_unset)`. If items reference the type and
    /// `force=false`, returns an error. With `force=true` the schema is
    /// removed and referencing items have their `type` set to NULL.
    fn delete_schema(&self, _type_name: &str, _force: bool) -> Result<(bool, usize)> {
        anyhow::bail!("schemas not supported by this store")
    }
    /// Count of items whose `type` equals `type_name`. Used by delete to
    /// detect references.
    fn count_items_by_type(&self, _type_name: &str) -> Result<i64> {
        Ok(0)
    }

    /// Merge a set of tags into the item's `metadata.tags` array (union,
    /// dedupe, preserve order). Best-effort; intended for analysis output
    /// promotion. Does not re-embed. Returns true when the item exists.
    fn merge_item_tags(&self, _id: &str, _tags: &[String]) -> Result<bool> {
        Ok(false)
    }
}

pub struct SqliteVectorStore {
    connection: Mutex<Option<Connection>>,
    graph_config: GraphConfig,
    /// Set on every write that invalidates the similarity graph. The graph is
    /// rebuilt lazily on the next graph read (or explicit rebuild call) so
    /// bulk imports don't pay O(N²) graph work per row.
    graph_dirty: AtomicBool,
}

impl SqliteVectorStore {
    pub fn connect(
        path: &str,
        embedding_dimension: usize,
        graph_config: GraphConfig,
    ) -> Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
        Self::connect_with_flags(path, embedding_dimension, flags, graph_config)
    }

    pub fn connect_uri(
        uri: &str,
        embedding_dimension: usize,
        graph_config: GraphConfig,
    ) -> Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI;
        Self::connect_with_flags(uri, embedding_dimension, flags, graph_config)
    }

    pub fn close(&self) -> Result<()> {
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let Some(connection) = guard.take() else {
            return Ok(());
        };

        connection
            .close()
            .map_err(|(_, error)| anyhow!(error))
            .context("failed to close sqlite connection")
    }

    fn connect_with_flags(
        path: &str,
        embedding_dimension: usize,
        flags: OpenFlags,
        graph_config: GraphConfig,
    ) -> Result<Self> {
        register_sqlite_vec();

        let connection = Connection::open_with_flags(path, flags)
            .with_context(|| format!("failed to open sqlite database at {path}"))?;

        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            ",
        )?;
        initialize_schema(&connection, embedding_dimension)?;

        Ok(Self {
            connection: Mutex::new(Some(connection)),
            graph_config,
            graph_dirty: AtomicBool::new(false),
        })
    }

    /// If the graph is dirty and enabled, rebuild it now. Must not be called
    /// while holding `self.connection` — takes the lock itself.
    fn ensure_graph_fresh(&self) -> Result<()> {
        if !self.graph_config.enabled {
            return Ok(());
        }
        if !self.graph_dirty.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        // Re-check under the lock — another caller may have just flushed.
        if !self.graph_dirty.load(Ordering::Acquire) {
            return Ok(());
        }
        rebuild_similarity_graph_locked(connection, self.graph_config)?;
        self.graph_dirty.store(false, Ordering::Release);
        Ok(())
    }

    fn ensure_graph_enabled(&self) -> Result<()> {
        if !self.graph_config.enabled {
            anyhow::bail!("graph support is disabled");
        }
        Ok(())
    }

}

impl VectorStore for SqliteVectorStore {
    fn upsert_item(&self, item: ItemRecord, embedding: &[f32]) -> Result<()> {
        if embedding.is_empty() {
            anyhow::bail!("embedding cannot be empty");
        }
        if item.id.trim().is_empty() {
            anyhow::bail!("item id cannot be empty");
        }

        let metadata_json = serde_json::to_string(&item.metadata)?;
        let data_json = match &item.data {
            Some(value) => Some(serde_json::to_string(value)?),
            None => None,
        };
        let embedding_json = embedding_to_json(embedding);

        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let transaction = connection.transaction()?;

        transaction.execute(
            "
            INSERT INTO items (id, text, metadata, source_id, created_at, path, type, data)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE
            SET text = excluded.text,
                metadata = excluded.metadata,
                source_id = excluded.source_id,
                created_at = excluded.created_at,
                path = excluded.path,
                type = excluded.type,
                data = excluded.data,
                ontology_status = CASE WHEN excluded.text != text THEN 'pending' ELSE ontology_status END
            ",
            params![
                item.id,
                item.text,
                metadata_json,
                item.source_id,
                item.created_at,
                item.path,
                item.type_name,
                data_json,
            ],
        )?;

        transaction.execute("DELETE FROM vec_items WHERE id = ?1", params![item.id])?;
        transaction.execute(
            "INSERT INTO vec_items (id, embedding) VALUES (?1, vec_f32(?2))",
            params![item.id, embedding_json],
        )?;

        transaction.commit()?;
        if self.graph_config.enabled {
            self.graph_dirty.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        if top_k == 0 {
            anyhow::bail!("top_k must be greater than zero");
        }

        let query_embedding_json = embedding_to_json(query_embedding);
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut results = Vec::new();
        if let Some(source_id) = source_id {
            let mut statement = connection.prepare(
                "
                SELECT
                    items.id,
                    items.text,
                    items.metadata,
                    items.source_id,
                    items.created_at,
                    CAST(vec_distance_L2(vec_items.embedding, vec_f32(?2)) AS REAL) AS distance
                FROM items
                JOIN vec_items ON vec_items.id = items.id
                WHERE items.source_id = ?1
                ORDER BY distance ASC
                LIMIT ?3
                ",
            )?;

            let rows = statement.query_map(
                params![source_id, query_embedding_json, top_k as i64],
                map_search_row,
            )?;

            for row in rows {
                results.push(row?);
            }
        } else {
            let mut statement = connection.prepare(
                "
                WITH matches AS (
                    SELECT id
                    FROM vec_items
                    WHERE embedding MATCH vec_f32(?1)
                      AND k = ?2
                )
                SELECT
                    items.id,
                    items.text,
                    items.metadata,
                    items.source_id,
                    items.created_at,
                    CAST(vec_distance_L2(vec_items.embedding, vec_f32(?1)) AS REAL) AS distance
                FROM matches
                JOIN vec_items ON vec_items.id = matches.id
                JOIN items ON items.id = matches.id
                ORDER BY distance ASC
                ",
            )?;

            let rows =
                statement.query_map(params![query_embedding_json, top_k as i64], map_search_row)?;

            for row in rows {
                results.push(row?);
            }
        }

        Ok(results)
    }

    fn search_hybrid(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        _query_sparse: &[(u32, f32)],
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        if top_k == 0 {
            anyhow::bail!("top_k must be greater than zero");
        }

        // SQLite path has no sparsevec column — sparse query is ignored;
        // dense + FTS5 keyword fusion is the legacy hybrid behavior.
        // 1. Vector Search
        let vector_hits = self.search(query_embedding, top_k * 2, source_id)?;

        // 2. Keyword Search (FTS5)
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        // FTS5 MATCH syntax preparation
        let fts_query = query_text
            .split_whitespace()
            .map(|w| format!("\"{}\"*", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");

        let mut keyword_hits = Vec::new();
        if !fts_query.is_empty() {
            let mut fts_stmt = connection.prepare(
                "
                SELECT
                    items.id,
                    items.text,
                    items.metadata,
                    items.source_id,
                    items.created_at,
                    COALESCE(bm25(items_fts), 0.0) as score
                FROM items
                JOIN items_fts ON items_fts.id = items.id
                WHERE items_fts MATCH ?1
                  AND (?2 IS NULL OR items.source_id = ?2)
                ORDER BY score ASC
                LIMIT ?3
                ",
            )?;

            let rows = fts_stmt.query_map(
                params![fts_query, source_id, (top_k * 2) as i64],
                map_search_row,
            )?;

            for row in rows {
                keyword_hits.push(row?);
            }
        }

        // 3. Reciprocal Rank Fusion (RRF)
        // score = 1 / (RRF_K + rank_vector) + 1 / (RRF_K + rank_keyword)
        let k = RRF_K;
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();
        let mut hit_map: HashMap<String, SearchHit> = HashMap::new();

        for (rank, hit) in vector_hits.into_iter().enumerate() {
            let score = 1.0 / (k + (rank + 1) as f32);
            rrf_scores.insert(hit.id.clone(), score);
            hit_map.insert(hit.id.clone(), hit);
        }

        for (rank, hit) in keyword_hits.into_iter().enumerate() {
            let score = 1.0 / (k + (rank + 1) as f32);
            *rrf_scores.entry(hit.id.clone()).or_insert(0.0) += score;
            hit_map.entry(hit.id.clone()).or_insert(hit);
        }

        let mut results: Vec<SearchHit> = rrf_scores
            .into_iter()
            .map(|(id, score)| {
                let mut hit = hit_map.remove(&id).unwrap();
                // We normalize RRF score back to a 'pseudo-distance' for UI compatibility.
                // The max possible RRF score is (1/k + 1/k) = 2/60 = 0.0333...
                // We want the result to be well within the default 0.8 filter.
                // Distance = 1.0 - (score / max_possible_score)
                let max_score = 2.0 / k;
                hit.distance = (1.0 - (score / max_score)).clamp(0.0, 1.0);
                hit
            })
            .collect();

        results.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        Ok(results)
    }

    fn distances_for_ids(&self, query_embedding: &[f32], ids: &[String]) -> Result<Vec<SearchHit>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let query_embedding_json = embedding_to_json(query_embedding);
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        let placeholders = std::iter::repeat("?")
            .take(ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "
            SELECT
                items.id,
                items.text,
                items.metadata,
                items.source_id,
                items.created_at,
                CAST(vec_distance_L2(vec_items.embedding, vec_f32(?)) AS REAL) AS distance
            FROM items
            JOIN vec_items ON vec_items.id = items.id
            WHERE items.id IN ({placeholders})
            ORDER BY distance ASC
            "
        );

        let mut statement = connection.prepare(&sql)?;
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() + 1);
        params_vec.push(&query_embedding_json);
        for id in ids {
            params_vec.push(id);
        }
        let rows = statement.query_map(rusqlite::params_from_iter(params_vec), map_search_row)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    fn get_item_analysis(&self, id: &str) -> Result<Option<ItemAnalysisRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "SELECT analysis_json, analysis_at, analysis_model FROM items WHERE id = ?1",
        )?;
        let mut rows = statement.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => {
                let json: Option<String> = row.get(0)?;
                let at: Option<i64> = row.get(1)?;
                let model: Option<String> = row.get(2)?;
                match (json, at, model) {
                    (Some(j), Some(a), Some(m)) if !j.is_empty() => {
                        let parsed: Value = serde_json::from_str(&j).unwrap_or(Value::Null);
                        Ok(Some(ItemAnalysisRecord {
                            analysis: parsed,
                            analysis_at: a,
                            analysis_model: m,
                        }))
                    }
                    _ => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    fn update_item_analysis(&self, id: &str, json: &str, model: &str) -> Result<bool> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let n = connection.execute(
            "UPDATE items SET analysis_json = ?1, analysis_at = ?2, analysis_model = ?3 WHERE id = ?4",
            rusqlite::params![json, now, model, id],
        )?;
        Ok(n > 0)
    }

    fn list_categories(&self) -> Result<Vec<CategorySummary>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "
            SELECT source_id, COUNT(*) AS item_count
            FROM items
            GROUP BY source_id
            ORDER BY source_id ASC
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(CategorySummary {
                source_id: row.get(0)?,
                item_count: row.get(1)?,
            })
        })?;

        let mut categories = Vec::new();
        for row in rows {
            categories.push(row?);
        }
        Ok(categories)
    }

    fn list_items(&self, request: ListItemsRequest) -> Result<(Vec<ItemRecord>, i64)> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        list_items_internal(connection, request)
    }


    fn get_item(&self, id: &str) -> Result<Option<ItemRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        get_item_internal(connection, id)
    }

    fn delete_item(&self, id: &str) -> Result<bool> {
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM vec_items WHERE id = ?1", params![id])?;
        let deleted = transaction.execute("DELETE FROM items WHERE id = ?1", params![id])?;
        transaction.commit()?;

        if deleted > 0 && self.graph_config.enabled {
            self.graph_dirty.store(true, Ordering::Release);
        }

        Ok(deleted > 0)
    }

    fn insert_attachment(&self, record: AttachmentRecord) -> Result<()> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "INSERT INTO attachments (id, item_id, filename, stored_name, mime, size, sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                record.item_id,
                record.filename,
                record.stored_name,
                record.mime,
                record.size,
                record.sha256,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    fn list_attachments_for_item(&self, item_id: &str) -> Result<Vec<AttachmentRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT id, item_id, filename, stored_name, mime, size, sha256, created_at
             FROM attachments WHERE item_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map(params![item_id], |row| {
                Ok(AttachmentRecord {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    filename: row.get(2)?,
                    stored_name: row.get(3)?,
                    mime: row.get(4)?,
                    size: row.get(5)?,
                    sha256: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn get_attachment(&self, id: &str) -> Result<Option<AttachmentRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT id, item_id, filename, stored_name, mime, size, sha256, created_at
             FROM attachments WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(AttachmentRecord {
                id: row.get(0)?,
                item_id: row.get(1)?,
                filename: row.get(2)?,
                stored_name: row.get(3)?,
                mime: row.get(4)?,
                size: row.get(5)?,
                sha256: row.get(6)?,
                created_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    fn delete_attachment(&self, id: &str) -> Result<Option<String>> {
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let tx = connection.transaction()?;
        let stored_name: Option<String> = tx
            .query_row(
                "SELECT stored_name FROM attachments WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if stored_name.is_some() {
            tx.execute("DELETE FROM attachments WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(stored_name)
    }

    fn list_all_paths(
        &self,
        source_id_filter: Option<&str>,
    ) -> Result<Vec<PathRow>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let (sql, has_filter) = match source_id_filter {
            Some(_) => (
                "SELECT source_id, path, COUNT(*) AS cnt FROM items
                 WHERE path IS NOT NULL AND path != '' AND source_id = ?1
                 GROUP BY source_id, path
                 ORDER BY source_id ASC, path ASC",
                true,
            ),
            None => (
                "SELECT source_id, path, COUNT(*) AS cnt FROM items
                 WHERE path IS NOT NULL AND path != ''
                 GROUP BY source_id, path
                 ORDER BY source_id ASC, path ASC",
                false,
            ),
        };
        let mut stmt = connection.prepare(sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(PathRow {
                source_id: row.get::<_, String>(0)?,
                path: row.get::<_, String>(1)?,
                count: row.get::<_, i64>(2)?,
            })
        };
        let rows: Vec<PathRow> = if has_filter {
            stmt.query_map(params![source_id_filter.unwrap()], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map([], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }

    fn list_path_children(
        &self,
        source_id: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<PathChild>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let prefix_norm = prefix
            .map(|p| p.trim_matches('/').to_owned())
            .filter(|p| !p.is_empty());

        // rel = the suffix of path past `prefix/`. For top-level (prefix
        // empty/None), rel = path itself. We then take the first segment.
        let sql = if prefix_norm.is_some() {
            "WITH rels AS (
                SELECT substr(path, length(?2) + 2) AS rel
                FROM items
                WHERE source_id = ?1
                  AND path IS NOT NULL
                  AND LOWER(path) LIKE LOWER(?2) || '/%'
            ),
            heads AS (
                SELECT
                    CASE WHEN instr(rel, '/') > 0
                         THEN substr(rel, 1, instr(rel, '/') - 1)
                         ELSE rel END AS segment,
                    CASE WHEN instr(rel, '/') > 0 THEN 1 ELSE 0 END AS deeper
                FROM rels
                WHERE rel IS NOT NULL AND rel != ''
            )
            SELECT segment, COUNT(*) AS cnt, MAX(deeper) AS has_children
            FROM heads
            GROUP BY segment
            ORDER BY segment ASC"
        } else {
            "WITH rels AS (
                SELECT path AS rel
                FROM items
                WHERE source_id = ?1
                  AND path IS NOT NULL
                  AND path != ''
            ),
            heads AS (
                SELECT
                    CASE WHEN instr(rel, '/') > 0
                         THEN substr(rel, 1, instr(rel, '/') - 1)
                         ELSE rel END AS segment,
                    CASE WHEN instr(rel, '/') > 0 THEN 1 ELSE 0 END AS deeper
                FROM rels
            )
            SELECT segment, COUNT(*) AS cnt, MAX(deeper) AS has_children
            FROM heads
            GROUP BY segment
            ORDER BY segment ASC"
        };
        let mut stmt = connection.prepare(sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(PathChild {
                segment: row.get::<_, String>(0)?,
                count: row.get::<_, i64>(1)?,
                has_children: row.get::<_, i64>(2)? > 0,
            })
        };
        let rows: Vec<PathChild> = if let Some(p) = &prefix_norm {
            stmt.query_map(params![source_id, p], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![source_id], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }

    fn graph_status(&self) -> Result<GraphStatus> {
        self.ensure_graph_fresh()?;
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        Ok(GraphStatus {
            enabled: self.graph_config.enabled,
            build_on_startup: self.graph_config.build_on_startup,
            similarity_top_k: self.graph_config.similarity_top_k,
            similarity_max_distance: self.graph_config.similarity_max_distance,
            cross_source: self.graph_config.cross_source,
            item_count: connection.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))?,
            edge_count: connection
                .query_row("SELECT COUNT(*) FROM graph_edges", [], |row| row.get(0))?,
            similarity_edge_count: connection.query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE edge_type = 'similarity'",
                [],
                |row| row.get(0),
            )?,
            manual_edge_count: connection.query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE edge_type = 'manual'",
                [],
                |row| row.get(0),
            )?,
        })
    }

    fn graph_neighborhood(
        &self,
        center_id: &str,
        depth: usize,
        limit: usize,
        edge_type: Option<GraphEdgeType>,
    ) -> Result<GraphNeighborhood> {
        self.ensure_graph_enabled()?;
        if limit == 0 {
            anyhow::bail!("limit must be greater than zero");
        }
        self.ensure_graph_fresh()?;

        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        if get_item_internal(connection, center_id)?.is_none() {
            anyhow::bail!("item {center_id} not found");
        }

        let mut visited_nodes = HashSet::new();
        let mut ordered_node_ids = Vec::new();
        let mut edge_map = HashMap::new();
        let mut queue = VecDeque::from([(center_id.to_owned(), 0usize)]);

        visited_nodes.insert(center_id.to_owned());
        ordered_node_ids.push(center_id.to_owned());

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }

            for edge in list_graph_edges_internal(connection, Some(&current_id), edge_type)? {
                edge_map
                    .entry(edge.id.clone())
                    .or_insert_with(|| edge.clone());

                for neighbor_id in [&edge.from_item_id, &edge.to_item_id] {
                    if visited_nodes.len() >= limit || visited_nodes.contains(neighbor_id) {
                        continue;
                    }
                    visited_nodes.insert(neighbor_id.clone());
                    ordered_node_ids.push(neighbor_id.clone());
                    queue.push_back((neighbor_id.clone(), current_depth + 1));
                }
            }
        }

        let mut nodes = Vec::with_capacity(ordered_node_ids.len());
        for node_id in &ordered_node_ids {
            if let Some(item) = get_item_internal(connection, node_id)? {
                nodes.push(item);
            }
        }

        let mut edges = edge_map
            .into_values()
            .filter(|edge| {
                visited_nodes.contains(&edge.from_item_id)
                    && visited_nodes.contains(&edge.to_item_id)
            })
            .collect::<Vec<_>>();
        edges.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(GraphNeighborhood {
            center_id: center_id.to_owned(),
            nodes,
            edges,
            pairwise_distances: list_pairwise_distances_for_ids(connection, &ordered_node_ids)?,
        })
    }

    fn list_graph_edges(
        &self,
        item_id: Option<&str>,
        edge_type: Option<GraphEdgeType>,
    ) -> Result<Vec<GraphEdgeRecord>> {
        self.ensure_graph_enabled()?;
        self.ensure_graph_fresh()?;

        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        list_graph_edges_internal(connection, item_id, edge_type)
    }

    fn rebuild_similarity_graph(&self) -> Result<usize> {
        self.ensure_graph_enabled()?;

        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let rebuilt = rebuild_similarity_graph_locked(connection, self.graph_config)?;
        self.graph_dirty.store(false, Ordering::Release);
        Ok(rebuilt)
    }

    fn add_manual_edge(&self, mut input: ManualEdgeInput) -> Result<GraphEdgeRecord> {
        self.ensure_graph_enabled()?;
        if input.from_item_id.trim().is_empty() || input.to_item_id.trim().is_empty() {
            anyhow::bail!("from_item_id and to_item_id must not be empty");
        }
        if input.from_item_id == input.to_item_id {
            anyhow::bail!("manual edges must connect two distinct items");
        }
        if !input.metadata.is_object() {
            anyhow::bail!("metadata must be a JSON object");
        }
        if input.weight <= 0.0 {
            anyhow::bail!("weight must be greater than zero");
        }
        if !input.directed && input.from_item_id > input.to_item_id {
            std::mem::swap(&mut input.from_item_id, &mut input.to_item_id);
        }

        let metadata_json = serde_json::to_string(&input.metadata)?;
        let timestamp = current_timestamp_millis()?;
        let edge_id = format!(
            "manual:{}:{}:{}",
            timestamp, input.from_item_id, input.to_item_id
        );

        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let transaction = connection.transaction()?;

        if get_item_internal(&transaction, &input.from_item_id)?.is_none() {
            anyhow::bail!("item {} not found", input.from_item_id);
        }
        if get_item_internal(&transaction, &input.to_item_id)?.is_none() {
            anyhow::bail!("item {} not found", input.to_item_id);
        }

        transaction.execute(
            "
            INSERT INTO graph_edges (
                id, from_item_id, to_item_id, edge_type, relation, weight, directed, metadata, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, 'manual', ?4, ?5, ?6, ?7, ?8, ?8)
            ",
            params![
                edge_id,
                input.from_item_id,
                input.to_item_id,
                input.relation,
                input.weight,
                bool_to_sqlite(input.directed),
                metadata_json,
                timestamp
            ],
        )?;
        transaction.commit()?;

        Ok(GraphEdgeRecord {
            id: edge_id,
            from_item_id: input.from_item_id,
            to_item_id: input.to_item_id,
            edge_type: GraphEdgeType::Manual,
            relation: input.relation,
            weight: input.weight,
            directed: input.directed,
            metadata: input.metadata,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    fn delete_graph_edge(&self, id: &str) -> Result<bool> {
        self.ensure_graph_enabled()?;

        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;

        let edge_type = connection
            .query_row(
                "SELECT edge_type FROM graph_edges WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        match edge_type.as_deref() {
            None => Ok(false),
            Some("similarity") => {
                anyhow::bail!("similarity edges must be rebuilt, not deleted manually")
            }
            Some(_) => {
                let deleted =
                    connection.execute("DELETE FROM graph_edges WHERE id = ?1", params![id])?;
                Ok(deleted > 0)
            }
        }
    }

    fn get_items_pending_ontology(&self, limit: usize) -> Result<Vec<ItemRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT id, text, metadata, source_id, created_at, path, type, data
             FROM items
             WHERE ontology_status = 'pending'
             ORDER BY created_at ASC
             LIMIT ?1",
        )?;
        let items = stmt
            .query_map([limit as i64], |row| {
                let metadata_str: String = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    metadata_str,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            })?
            .map(|r| {
                let (id, text, metadata_str, source_id, created_at, path, type_name, data_str) = r?;
                Ok(ItemRecord {
                    id,
                    text,
                    metadata: serde_json::from_str(&metadata_str)
                        .unwrap_or(serde_json::Value::Object(Default::default())),
                    source_id,
                    created_at,
                    path,
                    type_name,
                    data: data_str.and_then(|s| serde_json::from_str(&s).ok()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(items)
    }

    fn mark_ontology_status(&self, id: &str, status: &str) -> Result<()> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "UPDATE items SET ontology_status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    fn list_schemas(&self) -> Result<Vec<SchemaRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT type_name, json_schema, title, description, created_at, updated_at
             FROM schemas ORDER BY type_name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let schema_str: String = row.get(1)?;
            Ok(SchemaRecord {
                type_name: row.get(0)?,
                json_schema: serde_json::from_str(&schema_str)
                    .unwrap_or(Value::Object(Default::default())),
                title: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn get_schema(&self, type_name: &str) -> Result<Option<SchemaRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT type_name, json_schema, title, description, created_at, updated_at
             FROM schemas WHERE type_name = ?1",
        )?;
        let mut rows = stmt.query(params![type_name])?;
        if let Some(row) = rows.next()? {
            let schema_str: String = row.get(1)?;
            Ok(Some(SchemaRecord {
                type_name: row.get(0)?,
                json_schema: serde_json::from_str(&schema_str)
                    .unwrap_or(Value::Object(Default::default())),
                title: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    fn upsert_schema(&self, record: SchemaRecord) -> Result<()> {
        let schema_json = serde_json::to_string(&record.json_schema)?;
        let now = current_timestamp_millis()?;
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "INSERT INTO schemas (type_name, json_schema, title, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(type_name) DO UPDATE SET
                 json_schema = excluded.json_schema,
                 title = excluded.title,
                 description = excluded.description,
                 updated_at = excluded.updated_at",
            params![
                record.type_name,
                schema_json,
                record.title,
                record.description,
                if record.created_at == 0 { now } else { record.created_at },
                now,
            ],
        )?;
        Ok(())
    }

    fn delete_schema(&self, type_name: &str, force: bool) -> Result<(bool, usize)> {
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM items WHERE type = ?1",
            params![type_name],
            |row| row.get(0),
        )?;
        if count > 0 && !force {
            anyhow::bail!("schema {type_name} is referenced by {count} items");
        }
        let tx = connection.transaction()?;
        let unset = if count > 0 {
            tx.execute(
                "UPDATE items SET type = NULL, data = NULL WHERE type = ?1",
                params![type_name],
            )?
        } else {
            0
        };
        let n = tx.execute("DELETE FROM schemas WHERE type_name = ?1", params![type_name])?;
        tx.commit()?;
        Ok((n > 0, unset))
    }

    fn count_items_by_type(&self, type_name: &str) -> Result<i64> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let n: i64 = connection.query_row(
            "SELECT COUNT(*) FROM items WHERE type = ?1",
            params![type_name],
            |row| row.get(0),
        )?;
        Ok(n)
    }

    fn merge_item_tags(&self, id: &str, tags: &[String]) -> Result<bool> {
        if tags.is_empty() {
            return Ok(true);
        }
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let row: Option<String> = connection
            .query_row(
                "SELECT metadata FROM items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(metadata_str) = row else {
            return Ok(false);
        };
        let mut metadata: Value =
            serde_json::from_str(&metadata_str).unwrap_or_else(|_| Value::Object(Default::default()));
        let obj = metadata
            .as_object_mut()
            .context("metadata is not a JSON object")?;
        let mut existing: Vec<String> = obj
            .get("tags")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let mut seen: HashSet<String> = existing.iter().cloned().collect();
        for tag in tags {
            let t = tag.trim();
            if t.is_empty() || !seen.insert(t.to_owned()) {
                continue;
            }
            existing.push(t.to_owned());
        }
        obj.insert("tags".to_owned(), serde_json::json!(existing));
        let serialized = serde_json::to_string(&metadata)?;
        connection.execute(
            "UPDATE items SET metadata = ?1 WHERE id = ?2",
            params![serialized, id],
        )?;
        Ok(true)
    }
}


impl UserMemoryStore for SqliteVectorStore {
    fn log_user_event(&self, event: NewUserEvent) -> Result<()> {
        let item_ids_json = serde_json::to_string(&event.item_ids)?;
        let embedding_blob = event.query_embedding.as_deref().map(embedding_to_blob);
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "INSERT INTO user_events (id, subject, event_type, query, query_embedding, item_ids, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id,
                event.subject,
                event.event_type.as_str(),
                event.query,
                embedding_blob,
                item_ids_json,
                event.created_at
            ],
        )?;
        Ok(())
    }

    fn touch_item_accesses(&self, item_ids: &[String], now: i64) -> Result<()> {
        if item_ids.is_empty() {
            return Ok(());
        }
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let placeholders = std::iter::repeat("?")
            .take(item_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE items SET access_count = access_count + 1, last_accessed = ?1 WHERE id IN ({placeholders})"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(item_ids.len() + 1);
        params_vec.push(&now);
        for id in item_ids {
            params_vec.push(id);
        }
        connection.execute(&sql, rusqlite::params_from_iter(params_vec))?;
        Ok(())
    }

    fn get_user_profile(&self, subject: &str) -> Result<Option<UserProfile>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT subject, interest_embedding, event_horizon, updated_at
             FROM user_profiles WHERE subject = ?1",
        )?;
        let mut rows = stmt.query(params![subject])?;
        match rows.next()? {
            None => Ok(None),
            Some(row) => {
                let blob: Option<Vec<u8>> = row.get(1)?;
                Ok(Some(UserProfile {
                    subject: row.get(0)?,
                    interest_embedding: blob.map(|b| blob_to_embedding(&b)),
                    event_horizon: row.get(2)?,
                    updated_at: row.get(3)?,
                }))
            }
        }
    }

    fn upsert_user_profile(&self, profile: UserProfile) -> Result<()> {
        let blob = profile.interest_embedding.as_deref().map(embedding_to_blob);
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "INSERT INTO user_profiles (subject, interest_embedding, event_horizon, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(subject) DO UPDATE
             SET interest_embedding = excluded.interest_embedding,
                 event_horizon = excluded.event_horizon,
                 updated_at = excluded.updated_at",
            params![profile.subject, blob, profile.event_horizon, profile.updated_at],
        )?;
        Ok(())
    }

    fn get_recent_query_embeddings(&self, subject: &str, limit: usize) -> Result<Vec<Vec<f32>>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT query_embedding FROM user_events
             WHERE subject = ?1 AND event_type = 'search' AND query_embedding IS NOT NULL
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![subject, limit as i64], |row| {
            row.get::<_, Vec<u8>>(0)
        })?;
        let mut embeddings = Vec::new();
        for row in rows {
            embeddings.push(blob_to_embedding(&row?));
        }
        Ok(embeddings)
    }

    fn count_events_since(&self, subject: &str, horizon: i64) -> Result<i64> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let count = connection.query_row(
            "SELECT COUNT(*) FROM user_events
             WHERE subject = ?1 AND event_type = 'search' AND created_at > ?2",
            params![subject, horizon],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

impl MessageStore for SqliteVectorStore {
    fn get_message(&self, id: &str) -> Result<Option<MessageRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let row = connection
            .query_row(
                "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at
                 FROM messages WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, channel, sender, sender_kind_str, text, kind, metadata_str, created_at, updated_at)) = row else {
            return Ok(None);
        };
        Ok(Some(MessageRecord {
            id,
            channel,
            sender,
            sender_kind: MessageSenderKind::from_str(&sender_kind_str)?,
            text,
            kind,
            metadata: serde_json::from_str(&metadata_str)
                .unwrap_or(Value::Object(Default::default())),
            created_at,
            updated_at,
        }))
    }

    fn send_message(&self, message: NewMessage) -> Result<MessageRecord> {
        if message.channel.trim().is_empty() {
            anyhow::bail!("channel cannot be empty");
        }
        if message.sender.trim().is_empty() {
            anyhow::bail!("sender cannot be empty");
        }
        // Allow empty text when the message carries structured metadata (e.g.
        // pure permission_response rows). Reject only when both are empty.
        let metadata_empty = match &message.metadata {
            Value::Null => true,
            Value::Object(map) => map.is_empty(),
            _ => false,
        };
        if message.text.trim().is_empty() && metadata_empty {
            anyhow::bail!("text or metadata required");
        }
        if !message.metadata.is_object() && !message.metadata.is_null() {
            anyhow::bail!("metadata must be a JSON object");
        }

        let metadata_json = serde_json::to_string(&message.metadata)?;
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "INSERT INTO messages (id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                message.id,
                message.channel,
                message.sender,
                message.sender_kind.as_str(),
                message.text,
                message.kind,
                metadata_json,
                message.created_at,
            ],
        )?;

        Ok(MessageRecord {
            id: message.id,
            channel: message.channel,
            sender: message.sender,
            sender_kind: message.sender_kind,
            text: message.text,
            kind: message.kind,
            metadata: message.metadata,
            created_at: message.created_at,
            updated_at: message.created_at,
        })
    }

    fn update_message(
        &self,
        id: &str,
        update: MessageUpdate,
        now: i64,
    ) -> Result<Option<MessageRecord>> {
        if let Some(metadata) = &update.metadata {
            if !metadata.is_object() {
                anyhow::bail!("metadata must be a JSON object");
            }
        }
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let transaction = connection.transaction()?;

        let existing: Option<(String, String, String, String, String, String, String, i64, i64)> =
            transaction
                .query_row(
                    "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at
                     FROM messages WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                        ))
                    },
                )
                .optional()?;

        let Some((_, channel, sender, sender_kind_str, mut text, kind, metadata_str, created_at, _)) =
            existing
        else {
            return Ok(None);
        };

        if let Some(new_text) = &update.text {
            if update.append_text {
                text.push_str(new_text);
            } else {
                text = new_text.clone();
            }
        }

        let metadata_value: Value = if let Some(new_metadata) = &update.metadata {
            new_metadata.clone()
        } else {
            serde_json::from_str(&metadata_str).unwrap_or(Value::Object(Default::default()))
        };
        let metadata_to_store = serde_json::to_string(&metadata_value)?;

        transaction.execute(
            "UPDATE messages SET text = ?1, metadata = ?2, updated_at = ?3 WHERE id = ?4",
            params![text, metadata_to_store, now, id],
        )?;
        transaction.commit()?;

        Ok(Some(MessageRecord {
            id: id.to_owned(),
            channel,
            sender,
            sender_kind: MessageSenderKind::from_str(&sender_kind_str)?,
            text,
            kind,
            metadata: metadata_value,
            created_at,
            updated_at: now,
        }))
    }

    fn delete_message(&self, id: &str) -> Result<Option<MessageRecord>> {
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let transaction = connection.transaction()?;

        let existing: Option<(String, String, String, String, String, String, String, i64, i64)> =
            transaction
                .query_row(
                    "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at
                     FROM messages WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                        ))
                    },
                )
                .optional()?;

        let Some((id_v, channel, sender, sender_kind_str, text, kind, metadata_str, created_at, updated_at)) =
            existing
        else {
            return Ok(None);
        };

        transaction.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
        transaction.commit()?;

        Ok(Some(MessageRecord {
            id: id_v,
            channel,
            sender,
            sender_kind: MessageSenderKind::from_str(&sender_kind_str)?,
            text,
            kind,
            metadata: serde_json::from_str(&metadata_str).unwrap_or(Value::Object(Default::default())),
            created_at,
            updated_at,
        }))
    }

    fn find_permission_request(&self, request_id: &str) -> Result<Vec<MessageRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at
             FROM messages
             WHERE kind = 'permission_request'
               AND json_extract(metadata, '$.request_id') = ?1",
        )?;
        let rows = stmt.query_map(params![request_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, channel, sender, sender_kind_str, text, kind, metadata_str, created_at, updated_at) =
                row?;
            out.push(MessageRecord {
                id,
                channel,
                sender,
                sender_kind: MessageSenderKind::from_str(&sender_kind_str)?,
                text,
                kind,
                metadata: serde_json::from_str(&metadata_str)
                    .unwrap_or(Value::Object(Default::default())),
                created_at,
                updated_at,
            });
        }
        Ok(out)
    }

    fn list_channel_messages(&self, channel: &str) -> Result<Vec<MessageRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at
             FROM messages WHERE channel = ?1 ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![channel], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, channel, sender, sender_kind_str, text, kind, metadata_str, created_at, updated_at) =
                row?;
            out.push(MessageRecord {
                id,
                channel,
                sender,
                sender_kind: MessageSenderKind::from_str(&sender_kind_str)?,
                text,
                kind,
                metadata: serde_json::from_str(&metadata_str)
                    .unwrap_or(Value::Object(Default::default())),
                created_at,
                updated_at,
            });
        }
        Ok(out)
    }

    fn clear_channel(&self, channel: &str) -> Result<Vec<MessageRecord>> {
        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let transaction = connection.transaction()?;

        let rows: Vec<(String, String, String, String, String, String, String, i64, i64)> = {
            let mut stmt = transaction.prepare(
                "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at
                 FROM messages WHERE channel = ?1",
            )?;
            let mapped = stmt.query_map(params![channel], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })?;
            let mut out = Vec::new();
            for r in mapped {
                out.push(r?);
            }
            out
        };

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        transaction.execute("DELETE FROM messages WHERE channel = ?1", params![channel])?;
        transaction.commit()?;

        let mut out = Vec::with_capacity(rows.len());
        for (id, ch, sender, sender_kind_str, text, kind, metadata_str, created_at, updated_at) in rows {
            out.push(MessageRecord {
                id,
                channel: ch,
                sender,
                sender_kind: MessageSenderKind::from_str(&sender_kind_str)?,
                text,
                kind,
                metadata: serde_json::from_str(&metadata_str)
                    .unwrap_or(Value::Object(Default::default())),
                created_at,
                updated_at,
            });
        }
        Ok(out)
    }

    fn list_messages(&self, query: MessageQuery) -> Result<(Vec<MessageRecord>, i64)> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        let limit = query.limit.unwrap_or(100) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let sort_order = match query.sort_order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        };

        let mut where_clauses: Vec<&'static str> = Vec::new();
        let mut sql_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(channel) = &query.channel {
            where_clauses.push("channel = ?");
            sql_params.push(Box::new(channel.clone()));
        }
        if let Some(sender) = &query.sender {
            where_clauses.push("sender = ?");
            sql_params.push(Box::new(sender.clone()));
        }
        if let Some(kind) = &query.kind {
            where_clauses.push("kind = ?");
            sql_params.push(Box::new(kind.clone()));
        }
        if let Some(min_at) = query.min_created_at {
            // `since` cursor: includes both newly-created and recently-updated rows
            // so that long-poll consumers see streamed updates (e.g. agent_chunk
            // patches) without a separate watch endpoint.
            where_clauses.push("(created_at >= ? OR updated_at >= ?)");
            sql_params.push(Box::new(min_at));
            sql_params.push(Box::new(min_at));
        }
        if let Some(max_at) = query.max_created_at {
            where_clauses.push("created_at <= ?");
            sql_params.push(Box::new(max_at));
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM messages {}", where_sql);
        let total_count: i64 = connection.query_row(
            &count_sql,
            rusqlite::params_from_iter(sql_params.iter().map(|p| p.as_ref())),
            |row| row.get(0),
        )?;

        let sql = format!(
            "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at
             FROM messages
             {where_sql}
             ORDER BY created_at {sort_order}, id ASC
             LIMIT ? OFFSET ?"
        );

        let mut params_vec = sql_params;
        params_vec.push(Box::new(limit));
        params_vec.push(Box::new(offset));

        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(params_vec.iter().map(|p| p.as_ref())),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )?;

        let mut messages = Vec::new();
        for row in rows {
            let (id, channel, sender, sender_kind_str, text, kind, metadata_str, created_at, updated_at) =
                row?;
            messages.push(MessageRecord {
                id,
                channel,
                sender,
                sender_kind: MessageSenderKind::from_str(&sender_kind_str)?,
                text,
                kind,
                metadata: serde_json::from_str(&metadata_str)
                    .unwrap_or(Value::Object(Default::default())),
                created_at,
                updated_at,
            });
        }

        Ok((messages, total_count))
    }

    fn list_channels(&self) -> Result<Vec<ChannelSummary>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "SELECT channel, COUNT(*) AS message_count, MAX(created_at) AS last_at
             FROM messages
             GROUP BY channel
             ORDER BY last_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ChannelSummary {
                channel: row.get(0)?,
                message_count: row.get(1)?,
                last_message_at: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn list_items_internal(
    connection: &Connection,
    request: ListItemsRequest,
) -> Result<(Vec<ItemRecord>, i64)> {
    let mut items = Vec::new();
    let limit = request.limit.unwrap_or(100) as i64;
    let offset = request.offset.unwrap_or(0) as i64;
    let sort_order = match request.sort_order {
        SortOrder::Asc => "ASC",
        SortOrder::Desc => "DESC",
    };

    let mut where_clauses = Vec::new();
    let mut sql_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(source_id) = &request.source_id {
        where_clauses.push("source_id = ?".to_string());
        sql_params.push(Box::new(source_id.clone()));
    }

    if let Some(min_at) = request.min_created_at {
        where_clauses.push("created_at >= ?".to_string());
        sql_params.push(Box::new(min_at));
    }

    if let Some(max_at) = request.max_created_at {
        where_clauses.push("created_at <= ?".to_string());
        sql_params.push(Box::new(max_at));
    }

    for (key, value) in &request.metadata_filter {
        let path = format!("$.{}", key);
        where_clauses.push("json_extract(metadata, ?) = ?".to_string());
        sql_params.push(Box::new(path));
        sql_params.push(Box::new(value.clone()));
    }

    if let Some(prefix) = request.path_prefix.as_ref().filter(|p| !p.is_empty()) {
        where_clauses.push(
            "(LOWER(path) = LOWER(?) OR LOWER(path) LIKE LOWER(?) || '/%')".to_string(),
        );
        sql_params.push(Box::new(prefix.clone()));
        sql_params.push(Box::new(prefix.clone()));
    }

    if let Some(type_name) = request.type_name.as_ref().filter(|t| !t.is_empty()) {
        where_clauses.push("type = ?".to_string());
        sql_params.push(Box::new(type_name.clone()));
    }

    let where_sql = if where_clauses.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM items {}", where_sql);
    let total_count: i64 = connection.query_row(
        &count_sql,
        rusqlite::params_from_iter(sql_params.iter().map(|p| p.as_ref())),
        |row| row.get(0),
    )?;

    let sql = format!(
        "
        SELECT id, text, metadata, source_id, created_at, path, type, data
        FROM items
        {}
        ORDER BY created_at {sort_order}, id ASC
        LIMIT ? OFFSET ?
        ",
        where_sql
    );

    let mut query_params = sql_params;
    query_params.push(Box::new(limit));
    query_params.push(Box::new(offset));

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        rusqlite::params_from_iter(query_params.iter().map(|p| p.as_ref())),
        map_item_row,
    )?;
    for row in rows {
        items.push(row?);
    }

    Ok((items, total_count))
}

fn get_item_internal(connection: &Connection, id: &str) -> Result<Option<ItemRecord>> {
    let mut statement = connection.prepare(
        "
        SELECT id, text, metadata, source_id, created_at, path, type, data
        FROM items
        WHERE id = ?1
        ",
    )?;
    let mut rows = statement.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(map_item_row(row)?)),
        None => Ok(None),
    }
}

fn map_search_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit {
        id: row.get(0)?,
        text: row.get(1)?,
        metadata: parse_json_column(row.get::<_, String>(2)?, 2)?,
        source_id: row.get(3)?,
        created_at: row.get(4)?,
        distance: row.get(5)?,
        section_path: Vec::new(),
        retrievers: vec!["dense".to_owned()],
        chunk_text: None,
        path: None,
    })
}

fn map_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRecord> {
    let data_str: Option<String> = row.get(7)?;
    Ok(ItemRecord {
        id: row.get(0)?,
        text: row.get(1)?,
        metadata: parse_json_column(row.get::<_, String>(2)?, 2)?,
        source_id: row.get(3)?,
        created_at: row.get(4)?,
        path: row.get(5)?,
        type_name: row.get(6)?,
        data: match data_str {
            Some(s) => Some(parse_json_column(s, 7)?),
            None => None,
        },
    })
}

fn parse_json_column(raw: String, column_index: usize) -> rusqlite::Result<Value> {
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column_index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn embedding_to_json(embedding: &[f32]) -> String {
    let mut json = String::with_capacity(embedding.len() * 8 + 2);
    json.push('[');

    for (index, value) in embedding.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        use std::fmt::Write as _;
        let _ = write!(&mut json, "{value}");
    }

    json.push(']');
    json
}

fn current_timestamp_millis() -> Result<i64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before unix epoch")?;
    Ok(now.as_millis() as i64)
}

fn bool_to_sqlite(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_store() -> SqliteVectorStore {
        test_store_with_graph(GraphConfig::default())
    }

    fn test_store_with_graph(graph_config: GraphConfig) -> SqliteVectorStore {
        static NEXT_DB_ID: AtomicUsize = AtomicUsize::new(0);

        let db_id = NEXT_DB_ID.fetch_add(1, Ordering::Relaxed);
        let uri = format!("file:db-module-test-{db_id}?mode=memory&cache=shared");

        SqliteVectorStore::connect_uri(&uri, 3, graph_config)
            .expect("in-memory sqlite store should initialize")
    }

    #[test]
    fn stores_and_searches_embeddings_in_memory() {
        let store = test_store();

        store
            .upsert_item(
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "first".to_owned(),
                    metadata: json!({"kind": "alpha"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "doc-2".to_owned(),
                    text: "second".to_owned(),
                    metadata: json!({"kind": "beta"}),
                    source_id: "memory".to_owned(),
                    created_at: 2000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.0, 1.0, 0.0],
            )
            .unwrap();

        let results = store.search(&[0.9, 0.1, 0.0], 2, None).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "doc-1");
        assert_eq!(results[0].metadata, json!({"kind": "alpha"}));
        assert_eq!(results[0].source_id, "knowledge");
        assert_eq!(results[0].created_at, 1000);
        assert!(results[0].distance <= results[1].distance);
    }

    #[test]
    fn upsert_replaces_existing_document_payload() {
        let store = test_store();

        store
            .upsert_item(
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "old".to_owned(),
                    metadata: json!({"version": 1}),
                    source_id: "knowledge".to_owned(),
                    created_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "new".to_owned(),
                    metadata: json!({"version": 2}),
                    source_id: "memory".to_owned(),
                    created_at: 2000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.0, 1.0, 0.0],
            )
            .unwrap();

        let results = store.search(&[0.0, 1.0, 0.0], 1, None).unwrap();
        assert_eq!(results[0].text, "new");
        assert_eq!(results[0].metadata, json!({"version": 2}));
        assert_eq!(results[0].source_id, "memory");
        assert_eq!(results[0].created_at, 2000);
    }

    #[test]
    fn search_can_filter_by_source_id() {
        let store = test_store();

        store
            .upsert_item(
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "memory item".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "doc-2".to_owned(),
                    text: "knowledge item".to_owned(),
                    metadata: json!({"kind": "knowledge"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 2000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.9, 0.1, 0.0],
            )
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 5, Some("memory")).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");
        assert_eq!(results[0].source_id, "memory");
    }

    #[test]
    fn lists_categories_and_items() {
        let store = test_store();

        store
            .upsert_item(
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "memory item".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "doc-2".to_owned(),
                    text: "knowledge item".to_owned(),
                    metadata: json!({"kind": "knowledge"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 2000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.0, 1.0, 0.0],
            )
            .unwrap();

        let categories = store.list_categories().unwrap();
        assert_eq!(
            categories,
            vec![
                CategorySummary {
                    source_id: "knowledge".to_owned(),
                    item_count: 1,
                },
                CategorySummary {
                    source_id: "memory".to_owned(),
                    item_count: 1,
                }
            ]
        );

        let (knowledge_items, total) = store
            .list_items(ListItemsRequest {
                source_id: Some("knowledge".to_owned()),
                limit: None,
                offset: None,
                sort_order: SortOrder::Desc,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(knowledge_items.len(), 1);
        assert_eq!(total, 1);
        assert_eq!(knowledge_items[0].id, "doc-2");

        let fetched = store.get_item("doc-1").unwrap().unwrap();
        assert_eq!(fetched.source_id, "memory");
        assert_eq!(fetched.metadata, json!({"kind": "memory"}));
    }

    #[test]
    fn deletes_item_and_vector() {
        let store = test_store();

        store
            .upsert_item(
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "memory item".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap();

        assert!(store.delete_item("doc-1").unwrap());
        assert!(store.get_item("doc-1").unwrap().is_none());
        assert!(store.search(&[1.0, 0.0, 0.0], 5, None).unwrap().is_empty());
        assert!(!store.delete_item("doc-1").unwrap());
    }

    #[test]
    fn rejects_blank_item_id() {
        let store = test_store();

        let error = store
            .upsert_item(
                ItemRecord {
                    id: "   ".to_owned(),
                    text: "memory item".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap_err();

        assert!(error.to_string().contains("item id cannot be empty"));
    }

    #[test]
    fn rebuilds_similarity_edges_and_keeps_manual_edges() {
        let store = test_store_with_graph(GraphConfig {
            enabled: true,
            build_on_startup: false,
            similarity_top_k: 2,
            similarity_max_distance: 0.6,
            cross_source: false,
        });

        store
            .upsert_item(
                ItemRecord {
                    id: "mem-1".to_owned(),
                    text: "memory 1".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "mem-2".to_owned(),
                    text: "memory 2".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 2000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.95, 0.05, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "know-1".to_owned(),
                    text: "knowledge 1".to_owned(),
                    metadata: json!({"kind": "knowledge"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 3000,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.0, 1.0, 0.0],
            )
            .unwrap();

        let manual = store
            .add_manual_edge(ManualEdgeInput {
                from_item_id: "mem-1".to_owned(),
                to_item_id: "know-1".to_owned(),
                relation: Some("supports".to_owned()),
                weight: 1.0,
                directed: true,
                metadata: json!({"user": "mats"}),
            })
            .unwrap();

        let rebuilt = store.rebuild_similarity_graph().unwrap();
        assert_eq!(rebuilt, 1);

        let edges = store.list_graph_edges(None, None).unwrap();
        assert_eq!(edges.len(), 2);
        assert!(
            edges
                .iter()
                .any(|edge| edge.edge_type == GraphEdgeType::Similarity)
        );
        assert!(edges.iter().any(|edge| edge.id == manual.id));

        let status = store.graph_status().unwrap();
        assert_eq!(status.edge_count, 2);
        assert_eq!(status.similarity_edge_count, 1);
        assert_eq!(status.manual_edge_count, 1);
    }

    #[test]
    fn graph_neighborhood_returns_center_nodes_and_edges() {
        let store = test_store_with_graph(GraphConfig {
            enabled: true,
            build_on_startup: false,
            similarity_top_k: 2,
            similarity_max_distance: 1.0,
            cross_source: false,
        });

        store
            .upsert_item(
                ItemRecord {
                    id: "a".to_owned(),
                    text: "a".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 1,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "b".to_owned(),
                    text: "b".to_owned(),
                    metadata: json!({"kind": "memory"}),
                    source_id: "memory".to_owned(),
                    created_at: 2,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.9, 0.1, 0.0],
            )
            .unwrap();
        store
            .upsert_item(
                ItemRecord {
                    id: "c".to_owned(),
                    text: "c".to_owned(),
                    metadata: json!({"kind": "knowledge"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 3,
                    path: None,
                    type_name: None,
                    data: None,
                },
                &[0.0, 1.0, 0.0],
            )
            .unwrap();
        store
            .add_manual_edge(ManualEdgeInput {
                from_item_id: "a".to_owned(),
                to_item_id: "c".to_owned(),
                relation: Some("supports".to_owned()),
                weight: 1.0,
                directed: true,
                metadata: json!({"kind": "manual"}),
            })
            .unwrap();

        let neighborhood = store.graph_neighborhood("a", 1, 10, None).unwrap();

        assert_eq!(neighborhood.center_id, "a");
        assert_eq!(neighborhood.nodes.len(), 3);
        assert_eq!(neighborhood.edges.len(), 2);
        assert!(
            neighborhood
                .edges
                .iter()
                .any(|edge| edge.edge_type == GraphEdgeType::Manual)
        );
        assert!(
            neighborhood
                .edges
                .iter()
                .any(|edge| edge.edge_type == GraphEdgeType::Similarity)
        );
    }
}
