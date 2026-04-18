use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlite_vec::sqlite3_vec_init;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Mutex, Once},
    time::{SystemTime, UNIX_EPOCH},
};

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

#[derive(Debug, Clone)]
pub struct ManualEdgeInput {
    pub from_item_id: String,
    pub to_item_id: String,
    pub relation: Option<String>,
    pub weight: f32,
    pub directed: bool,
    pub metadata: Value,
}

pub trait VectorStore: Send + Sync {
    fn upsert_item(&self, item: ItemRecord, embedding: &[f32]) -> Result<()>;
    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>>;
    fn list_categories(&self) -> Result<Vec<CategorySummary>>;
    fn list_items(&self, source_id: Option<&str>) -> Result<Vec<ItemRecord>>;
    fn get_item(&self, id: &str) -> Result<Option<ItemRecord>>;
    fn delete_item(&self, id: &str) -> Result<bool>;
    fn distances_for_ids(
        &self,
        query_embedding: &[f32],
        ids: &[String],
    ) -> Result<Vec<SearchHit>>;
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
        })
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
                created_at = excluded.created_at
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
            rebuild_similarity_graph_locked(connection, self.graph_config)?;
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

    fn distances_for_ids(
        &self,
        query_embedding: &[f32],
        ids: &[String],
    ) -> Result<Vec<SearchHit>> {
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
        let rows =
            statement.query_map(rusqlite::params_from_iter(params_vec), map_search_row)?;
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

    fn list_items(&self, source_id: Option<&str>) -> Result<Vec<ItemRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        list_items_internal(connection, source_id)
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
            rebuild_similarity_graph_locked(connection, self.graph_config)?;
        }

        Ok(deleted > 0)
    }

    fn graph_status(&self) -> Result<GraphStatus> {
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
        rebuild_similarity_graph_locked(connection, self.graph_config)
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

fn initialize_schema(connection: &Connection, embedding_dimension: usize) -> Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            text TEXT NOT NULL,
            metadata TEXT NOT NULL CHECK (json_valid(metadata)),
            source_id TEXT NOT NULL DEFAULT 'default',
            created_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_items_source_id ON items(source_id);
        CREATE INDEX IF NOT EXISTS idx_items_created_at ON items(created_at DESC);

        CREATE TABLE IF NOT EXISTS graph_edges (
            id TEXT PRIMARY KEY,
            from_item_id TEXT NOT NULL,
            to_item_id TEXT NOT NULL,
            edge_type TEXT NOT NULL CHECK (edge_type IN ('similarity', 'manual')),
            relation TEXT,
            weight REAL NOT NULL,
            directed INTEGER NOT NULL DEFAULT 0 CHECK (directed IN (0, 1)),
            metadata TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata)),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(from_item_id) REFERENCES items(id) ON DELETE CASCADE,
            FOREIGN KEY(to_item_id) REFERENCES items(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_graph_edges_from ON graph_edges(from_item_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_to ON graph_edges(to_item_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_type ON graph_edges(edge_type);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_similarity_pair
            ON graph_edges(from_item_id, to_item_id, edge_type)
            WHERE edge_type = 'similarity';
        ",
    )?;
    ensure_column_exists(
        connection,
        "items",
        "source_id",
        "TEXT NOT NULL DEFAULT 'default'",
    )?;
    ensure_column_exists(
        connection,
        "items",
        "created_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    connection.execute_batch(&format!(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(
            id TEXT PRIMARY KEY,
            embedding FLOAT[{embedding_dimension}]
        );
        "
    ))?;

    Ok(())
}

fn list_items_internal(
    connection: &Connection,
    source_id: Option<&str>,
) -> Result<Vec<ItemRecord>> {
    let mut items = Vec::new();
    if let Some(source_id) = source_id {
        let mut statement = connection.prepare(
            "
            SELECT id, text, metadata, source_id, created_at
            FROM items
            WHERE source_id = ?1
            ORDER BY created_at DESC, id ASC
            ",
        )?;
        let rows = statement.query_map(params![source_id], map_item_row)?;
        for row in rows {
            items.push(row?);
        }
    } else {
        let mut statement = connection.prepare(
            "
            SELECT id, text, metadata, source_id, created_at
            FROM items
            ORDER BY created_at DESC, id ASC
            ",
        )?;
        let rows = statement.query_map([], map_item_row)?;
        for row in rows {
            items.push(row?);
        }
    }

    Ok(items)
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

fn list_graph_edges_internal(
    connection: &Connection,
    item_id: Option<&str>,
    edge_type: Option<GraphEdgeType>,
) -> Result<Vec<GraphEdgeRecord>> {
    let edge_type = edge_type.map(GraphEdgeType::as_str);
    let mut statement = connection.prepare(
        "
        SELECT
            id,
            from_item_id,
            to_item_id,
            edge_type,
            relation,
            weight,
            directed,
            metadata,
            created_at,
            updated_at
        FROM graph_edges
        WHERE (?1 IS NULL OR from_item_id = ?1 OR to_item_id = ?1)
          AND (?2 IS NULL OR edge_type = ?2)
        ORDER BY updated_at DESC, id ASC
        ",
    )?;
    let rows = statement.query_map(params![item_id, edge_type], map_graph_edge_row)?;

    let mut edges = Vec::new();
    for row in rows {
        edges.push(row?);
    }
    Ok(edges)
}

fn list_pairwise_distances_for_ids(
    connection: &Connection,
    ids: &[String],
) -> Result<Vec<GraphNodeDistance>> {
    if ids.len() < 2 {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "
        SELECT
            left_vec.id,
            right_vec.id,
            CAST(vec_distance_L2(left_vec.embedding, right_vec.embedding) AS REAL) AS distance
        FROM vec_items AS left_vec
        JOIN vec_items AS right_vec ON left_vec.id < right_vec.id
        WHERE left_vec.id IN ({placeholders})
          AND right_vec.id IN ({placeholders})
        ORDER BY left_vec.id ASC, right_vec.id ASC
        "
    );

    let mut statement = connection.prepare(&sql)?;
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() * 2);
    for id in ids {
        params_vec.push(id);
    }
    for id in ids {
        params_vec.push(id);
    }

    let rows = statement.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok(GraphNodeDistance {
            from_item_id: row.get(0)?,
            to_item_id: row.get(1)?,
            distance: row.get(2)?,
        })
    })?;

    let mut distances = Vec::new();
    for row in rows {
        distances.push(row?);
    }
    Ok(distances)
}

fn rebuild_similarity_graph_locked(
    connection: &mut Connection,
    graph_config: GraphConfig,
) -> Result<usize> {
    if !graph_config.enabled {
        return Ok(0);
    }

    let mut item_statement = connection.prepare(
        "
        SELECT items.id
        FROM items
        JOIN vec_items ON vec_items.id = items.id
        ORDER BY items.id ASC
        ",
    )?;
    let item_ids = item_statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(item_statement);

    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM graph_edges WHERE edge_type = 'similarity'", [])?;

    let mut inserted_pairs = HashSet::new();
    let timestamp = current_timestamp_millis()?;
    let mut inserted = 0usize;

    for item_id in item_ids {
        let candidates = {
            let mut statement = transaction.prepare(
                "
                SELECT
                    other.id,
                    CAST(vec_distance_L2(base.embedding, other_vec.embedding) AS REAL) AS distance
                FROM vec_items AS base
                JOIN items AS base_item ON base_item.id = base.id
                JOIN vec_items AS other_vec ON other_vec.id != base.id
                JOIN items AS other ON other.id = other_vec.id
                WHERE base.id = ?1
                  AND (?2 = 1 OR other.source_id = base_item.source_id)
                ORDER BY distance ASC, other.id ASC
                LIMIT ?3
                ",
            )?;
            let rows = statement.query_map(
                params![
                    item_id,
                    bool_to_sqlite(graph_config.cross_source),
                    graph_config.similarity_top_k as i64
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?)),
            )?;

            let mut candidates = Vec::new();
            for row in rows {
                candidates.push(row?);
            }
            candidates
        };

        for (other_id, distance) in candidates {
            if distance > graph_config.similarity_max_distance {
                continue;
            }
            let (from_item_id, to_item_id) = canonical_edge_pair(&item_id, &other_id);
            if !inserted_pairs.insert((from_item_id.clone(), to_item_id.clone())) {
                continue;
            }

            let weight = 1.0 / (1.0 + distance);
            let metadata = serde_json::json!({ "distance": distance });
            transaction.execute(
                "
                INSERT INTO graph_edges (
                    id, from_item_id, to_item_id, edge_type, relation, weight, directed, metadata, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, 'similarity', NULL, ?4, 0, ?5, ?6, ?6)
                ",
                params![
                    format!("similarity:{from_item_id}:{to_item_id}"),
                    from_item_id,
                    to_item_id,
                    weight,
                    serde_json::to_string(&metadata)?,
                    timestamp
                ],
            )?;
            inserted += 1;
        }
    }

    transaction.commit()?;
    Ok(inserted)
}

fn ensure_column_exists(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    if table_has_column(connection, table, column)? {
        return Ok(());
    }

    connection.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;

    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }

    Ok(false)
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

fn map_graph_edge_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GraphEdgeRecord> {
    let edge_type_raw = row.get::<_, String>(3)?;
    let edge_type = GraphEdgeType::from_str(&edge_type_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;

    Ok(GraphEdgeRecord {
        id: row.get(0)?,
        from_item_id: row.get(1)?,
        to_item_id: row.get(2)?,
        edge_type,
        relation: row.get(4)?,
        weight: row.get(5)?,
        directed: row.get::<_, i64>(6)? != 0,
        metadata: parse_json_column(row.get::<_, String>(7)?, 7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
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

fn register_sqlite_vec() {
    static SQLITE_VEC_INIT: Once = Once::new();

    SQLITE_VEC_INIT.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
    });
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

fn canonical_edge_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
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

        let knowledge_items = store.list_items(Some("knowledge")).unwrap();
        assert_eq!(knowledge_items.len(), 1);
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
