mod auth;
mod graph;
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub id: String,
    pub text: String,
    pub metadata: Value,
    pub source_id: String,
    pub created_at: i64,
    pub distance: f32,
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
    fn as_str(self) -> &'static str {
        match self {
            Self::Similarity => "similarity",
            Self::Manual => "manual",
        }
    }

    fn from_str(value: &str) -> Result<Self> {
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
    fn from_str(value: &str) -> Result<Self> {
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

#[derive(Debug, Clone)]
pub struct ListItemsRequest {
    pub source_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_order: SortOrder,
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
    fn as_str(self) -> &'static str {
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

pub trait VectorStore: Send + Sync {
    fn upsert_item(&self, item: ItemRecord, embedding: &[f32]) -> Result<()>;
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
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>>;
    fn list_categories(&self) -> Result<Vec<CategorySummary>>;
    fn list_items(&self, request: ListItemsRequest) -> Result<(Vec<ItemRecord>, i64)>;
    fn list_large_items(&self, min_chars: usize, limit: usize, offset: usize) -> Result<(Vec<ItemRecord>, i64)>;
    fn get_item(&self, id: &str) -> Result<Option<ItemRecord>>;
    fn delete_item(&self, id: &str) -> Result<bool>;
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

    pub fn get_items_pending_ontology(&self, limit: usize) -> Result<Vec<ItemRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut stmt = connection.prepare(
            "SELECT id, text, metadata, source_id, created_at
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
                ))
            })?
            .map(|r| {
                let (id, text, metadata_str, source_id, created_at) = r?;
                Ok(ItemRecord {
                    id,
                    text,
                    metadata: serde_json::from_str(&metadata_str)
                        .unwrap_or(serde_json::Value::Object(Default::default())),
                    source_id,
                    created_at,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn mark_ontology_status(&self, id: &str, status: &str) -> Result<()> {
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
        let embedding_json = embedding_to_json(embedding);

        let mut guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_mut()
            .context("sqlite connection has already been closed")?;
        let transaction = connection.transaction()?;

        transaction.execute(
            "
            INSERT INTO items (id, text, metadata, source_id, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE
            SET text = excluded.text,
                metadata = excluded.metadata,
                source_id = excluded.source_id,
                created_at = excluded.created_at,
                ontology_status = CASE WHEN excluded.text != text THEN 'pending' ELSE ontology_status END
            ",
            params![
                item.id,
                item.text,
                metadata_json,
                item.source_id,
                item.created_at
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
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        if top_k == 0 {
            anyhow::bail!("top_k must be greater than zero");
        }

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

    fn list_large_items(&self, min_chars: usize, limit: usize, offset: usize) -> Result<(Vec<ItemRecord>, i64)> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        let min_chars = min_chars as i64;
        let limit = limit as i64;
        let offset = offset as i64;

        let total_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM items WHERE LENGTH(text) > ?1 AND json_extract(metadata, '$._chunk') IS NULL",
            params![min_chars],
            |row| row.get(0),
        )?;

        let mut statement = connection.prepare(
            "SELECT id, text, metadata, source_id, created_at
             FROM items
             WHERE LENGTH(text) > ?1 AND json_extract(metadata, '$._chunk') IS NULL
             ORDER BY LENGTH(text) DESC
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = statement.query_map(params![min_chars, limit, offset], map_item_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }

        Ok((items, total_count))
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

    let total_count: i64 = if let Some(source_id) = &request.source_id {
        connection.query_row(
            "SELECT COUNT(*) FROM items WHERE source_id = ?1",
            params![source_id],
            |row| row.get(0),
        )?
    } else {
        connection.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))?
    };

    let sql = format!(
        "
        SELECT id, text, metadata, source_id, created_at
        FROM items
        WHERE (?1 IS NULL OR source_id = ?1)
        ORDER BY created_at {sort_order}, id ASC
        LIMIT ?2 OFFSET ?3
        "
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![request.source_id, limit, offset], map_item_row)?;
    for row in rows {
        items.push(row?);
    }

    Ok((items, total_count))
}

fn get_item_internal(connection: &Connection, id: &str) -> Result<Option<ItemRecord>> {
    let mut statement = connection.prepare(
        "
        SELECT id, text, metadata, source_id, created_at
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
    })
}

fn map_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRecord> {
    Ok(ItemRecord {
        id: row.get(0)?,
        text: row.get(1)?,
        metadata: parse_json_column(row.get::<_, String>(2)?, 2)?,
        source_id: row.get(3)?,
        created_at: row.get(4)?,
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
