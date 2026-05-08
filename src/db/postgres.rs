use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
use serde_json::Value;
use std::str::FromStr;
use tokio::runtime::Handle;
use tokio_postgres::{Config as PgConfig, NoTls};
use tracing::info;

use super::{
    AuthStore, CategorySummary, ChannelSummary, DeviceAuthRecord, DeviceAuthStatus, DocChunk,
    GraphConfig, GraphEdgeRecord, GraphEdgeType, GraphNeighborhood, GraphStatus, ItemRecord,
    ListItemsRequest, ManualEdgeInput, McpTokenRecord, MessageQuery, MessageRecord,
    MessageSenderKind, MessageStore, MessageUpdate, NewDeviceAuth, NewMcpToken, NewMessage,
    NewUserEvent, SearchHit, SortOrder, UserMemoryStore, UserProfile, VectorStore,
};

pub use deadpool_postgres::Pool as PgPool;

const EMBEDDING_MODEL: &str = "bge-m3";
const EMBEDDING_VERSION: i32 = 1;
/// bge-m3 sparse output is one weight per vocab token. Vocab size is fixed.
const SPARSE_DIM: i32 = 250_002;

/// Convert our `(vocab_id, weight)` pairs into a `pgvector::SparseVector`.
/// Returns `None` when the input is empty so the column is bound as NULL.
fn build_sparsevec(pairs: &[(u32, f32)]) -> Option<pgvector::SparseVector> {
    if pairs.is_empty() {
        return None;
    }
    let mapped: Vec<(i32, f32)> = pairs
        .iter()
        .filter(|(_, w)| *w != 0.0)
        .map(|(idx, w)| (*idx as i32, *w))
        .collect();
    if mapped.is_empty() {
        return None;
    }
    Some(pgvector::SparseVector::from_map(
        mapped.iter().map(|(i, v)| (i, v)),
        SPARSE_DIM,
    ))
}

/// Connect to Postgres, run pending migrations from the embedded SQL files,
/// and return a deadpool pool.
pub async fn connect(database_url: &str, max_size: usize) -> Result<PgPool> {
    let pg_config = PgConfig::from_str(database_url).context("parsing postgres URL")?;

    let mut cfg = Config::new();
    cfg.host = pg_config.get_hosts().first().and_then(|h| match h {
        tokio_postgres::config::Host::Tcp(s) => Some(s.clone()),
        _ => None,
    });
    cfg.port = pg_config.get_ports().first().copied();
    cfg.user = pg_config.get_user().map(str::to_owned);
    cfg.password = pg_config
        .get_password()
        .map(|b| String::from_utf8_lossy(b).into_owned());
    cfg.dbname = pg_config.get_dbname().map(str::to_owned);
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    let mut pool_cfg = deadpool_postgres::PoolConfig::default();
    pool_cfg.max_size = max_size;
    cfg.pool = Some(pool_cfg);

    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .context("creating postgres pool")?;

    {
        let client = pool.get().await.context("acquiring postgres connection")?;
        run_migrations(&client).await?;
    }

    Ok(pool)
}

/// Embedded migrations. Discovered at compile time via `include_str!` so the
/// binary is self-contained — no external migration tool required.
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_documents_chunks",
        include_str!("../../migrations/0001_documents_chunks.sql"),
    ),
    (
        "0002_auxiliary_tables",
        include_str!("../../migrations/0002_auxiliary_tables.sql"),
    ),
    (
        "0003_sparse_embeddings",
        include_str!("../../migrations/0003_sparse_embeddings.sql"),
    ),
];

async fn run_migrations(client: &tokio_postgres::Client) -> Result<()> {
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (\
                 name TEXT PRIMARY KEY, \
                 applied_at TIMESTAMPTZ NOT NULL DEFAULT now())",
        )
        .await
        .context("creating schema_migrations table")?;

    for (name, sql) in MIGRATIONS {
        let already = client
            .query_opt(
                "SELECT 1 FROM schema_migrations WHERE name = $1",
                &[name],
            )
            .await
            .context("checking schema_migrations")?;
        if already.is_some() {
            continue;
        }
        info!("postgres: applying migration {name}");
        client
            .batch_execute(sql)
            .await
            .with_context(|| format!("applying migration {name}"))?;
        client
            .execute(
                "INSERT INTO schema_migrations (name) VALUES ($1)",
                &[name],
            )
            .await
            .context("recording applied migration")?;
    }

    Ok(())
}

/// Postgres-backed implementation of `VectorStore`. Maps the legacy flat
/// `ItemRecord` onto the new `documents` + `chunks` schema: each `upsert_item`
/// writes one document and replaces its single chunk at position 0.
///
/// Phase 1: dense-only. Hybrid search falls back to dense; sparse + reranker
/// land in phases 2/3. Graph operations are stubbed out (this store doesn't
/// have a `graph_edges` table yet — port pending).
pub struct PostgresVectorStore {
    pool: PgPool,
    runtime: Handle,
    graph_config: GraphConfig,
}

impl PostgresVectorStore {
    pub fn new(pool: PgPool, runtime: Handle, graph_config: GraphConfig) -> Self {
        Self {
            pool,
            runtime,
            graph_config,
        }
    }

    /// Bridge sync trait method → async tokio-postgres call. Safe to invoke
    /// from `spawn_blocking` threads (which is how the API layer calls
    /// `VectorStore` methods); not safe from a runtime worker.
    fn block<F, T>(&self, fut: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.runtime.block_on(fut)
    }
}

fn ms_to_ts(ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_else(Utc::now)
}

fn ts_to_ms(ts: DateTime<Utc>) -> i64 {
    ts.timestamp_millis()
}

fn current_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn row_to_graph_edge(row: &tokio_postgres::Row) -> Result<GraphEdgeRecord> {
    let edge_type_str: String = row.try_get("edge_type")?;
    Ok(GraphEdgeRecord {
        id: row.try_get("id")?,
        from_item_id: row.try_get("from_item_id")?,
        to_item_id: row.try_get("to_item_id")?,
        edge_type: GraphEdgeType::from_str(&edge_type_str)?,
        relation: row.try_get("relation")?,
        weight: row.try_get("weight")?,
        directed: row.try_get("directed")?,
        metadata: row.try_get::<_, Value>("metadata")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Per-document MIN cosine distance for an unordered set of doc ids. Mirrors
/// the SQLite `list_pairwise_distances_for_ids` shape — only canonical pairs
/// (a < b) are returned.
fn pairwise_doc_distances(
    store: &PostgresVectorStore,
    ids: &[String],
) -> Result<Vec<super::GraphNodeDistance>> {
    if ids.len() < 2 {
        return Ok(Vec::new());
    }
    let pool = store.pool.clone();
    let ids = ids.to_vec();
    store.block(async move {
        let client = pool.get().await.context("acquiring postgres connection")?;
        let rows = client
            .query(
                "SELECT ca.document_id AS from_id, cb.document_id AS to_id, \
                        MIN(ca.dense_embedding <=> cb.dense_embedding)::REAL AS distance \
                 FROM chunks ca \
                 JOIN chunks cb ON cb.document_id > ca.document_id \
                 WHERE ca.document_id = ANY($1::TEXT[]) \
                   AND cb.document_id = ANY($1::TEXT[]) \
                   AND ca.dense_embedding IS NOT NULL \
                   AND cb.dense_embedding IS NOT NULL \
                 GROUP BY ca.document_id, cb.document_id \
                 ORDER BY ca.document_id ASC, cb.document_id ASC",
                &[&ids],
            )
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(super::GraphNodeDistance {
                from_item_id: row.try_get("from_id")?,
                to_item_id: row.try_get("to_id")?,
                distance: row.try_get("distance")?,
            });
        }
        Ok(out)
    })
}

fn row_to_item(row: &tokio_postgres::Row) -> Result<ItemRecord> {
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    Ok(ItemRecord {
        id: row.try_get("id")?,
        text: row.try_get("content")?,
        metadata: row.try_get::<_, Value>("metadata")?,
        source_id: row.try_get("source_id")?,
        created_at: ts_to_ms(created_at),
    })
}

impl VectorStore for PostgresVectorStore {
    fn upsert_item(&self, item: ItemRecord, embedding: &[f32]) -> Result<()> {
        if embedding.is_empty() {
            anyhow::bail!("embedding cannot be empty");
        }
        if item.id.trim().is_empty() {
            anyhow::bail!("item id cannot be empty");
        }
        let pool = self.pool.clone();
        let vector = pgvector::Vector::from(embedding.to_vec());
        let created_at = ms_to_ts(item.created_at);
        let author = item
            .metadata
            .get("author")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let tags: Vec<String> = item
            .metadata
            .get("tags")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let status = item
            .metadata
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned);

        self.block(async move {
            let mut client = pool.get().await?;
            let tx = client.transaction().await?;
            tx.execute(
                "INSERT INTO documents (id, source_id, kind, author, content, metadata, tags, status, created_at, updated_at) \
                 VALUES ($1, $2, 'text', $3, $4, $5, $6, $7, $8, now()) \
                 ON CONFLICT (id) DO UPDATE SET \
                     source_id = EXCLUDED.source_id, \
                     author = EXCLUDED.author, \
                     content = EXCLUDED.content, \
                     metadata = EXCLUDED.metadata, \
                     tags = EXCLUDED.tags, \
                     status = EXCLUDED.status, \
                     updated_at = now()",
                &[
                    &item.id,
                    &item.source_id,
                    &author,
                    &item.text,
                    &item.metadata,
                    &tags,
                    &status,
                    &created_at,
                ],
            )
            .await?;

            tx.execute("DELETE FROM chunks WHERE document_id = $1", &[&item.id])
                .await?;
            tx.execute(
                "INSERT INTO chunks (document_id, position, content, dense_embedding, embedding_model, embedding_version) \
                 VALUES ($1, 0, $2, $3, $4, $5)",
                &[
                    &item.id,
                    &item.text,
                    &vector,
                    &EMBEDDING_MODEL,
                    &EMBEDDING_VERSION,
                ],
            )
            .await?;
            tx.commit().await?;
            Ok(())
        })
    }

    fn upsert_document(&self, item: ItemRecord, chunks: Vec<DocChunk>) -> Result<()> {
        if chunks.is_empty() {
            anyhow::bail!("upsert_document called with no chunks");
        }
        if item.id.trim().is_empty() {
            anyhow::bail!("item id cannot be empty");
        }
        let pool = self.pool.clone();
        let created_at = ms_to_ts(item.created_at);
        let author = item
            .metadata
            .get("author")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let tags: Vec<String> = item
            .metadata
            .get("tags")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let status = item
            .metadata
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned);

        self.block(async move {
            let mut client = pool.get().await?;
            let tx = client.transaction().await?;
            tx.execute(
                "INSERT INTO documents (id, source_id, kind, author, content, metadata, tags, status, created_at, updated_at) \
                 VALUES ($1, $2, 'text', $3, $4, $5, $6, $7, $8, now()) \
                 ON CONFLICT (id) DO UPDATE SET \
                     source_id = EXCLUDED.source_id, \
                     author = EXCLUDED.author, \
                     content = EXCLUDED.content, \
                     metadata = EXCLUDED.metadata, \
                     tags = EXCLUDED.tags, \
                     status = EXCLUDED.status, \
                     updated_at = now()",
                &[
                    &item.id,
                    &item.source_id,
                    &author,
                    &item.text,
                    &item.metadata,
                    &tags,
                    &status,
                    &created_at,
                ],
            )
            .await?;
            tx.execute("DELETE FROM chunks WHERE document_id = $1", &[&item.id])
                .await?;
            for chunk in &chunks {
                let vector = pgvector::Vector::from(chunk.embedding.clone());
                let section_path: Option<&[String]> = if chunk.section_path.is_empty() {
                    None
                } else {
                    Some(&chunk.section_path)
                };
                let sparse = chunk
                    .sparse
                    .as_deref()
                    .and_then(build_sparsevec);
                tx.execute(
                    "INSERT INTO chunks (document_id, position, content, section_path, dense_embedding, sparse_embedding, embedding_model, embedding_version) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[
                        &item.id,
                        &chunk.position,
                        &chunk.content,
                        &section_path,
                        &vector,
                        &sparse,
                        &EMBEDDING_MODEL,
                        &EMBEDDING_VERSION,
                    ],
                )
                .await?;
            }
            tx.commit().await?;
            Ok(())
        })
    }

    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        if query_embedding.is_empty() {
            anyhow::bail!("query embedding cannot be empty");
        }
        let pool = self.pool.clone();
        let vector = pgvector::Vector::from(query_embedding.to_vec());
        let limit = top_k as i64;
        let source = source_id.map(str::to_owned);

        self.block(async move {
            let client = pool.get().await?;
            // Per-document min distance over chunks. The DISTINCT ON pattern
            // pulls the section_path of the closest chunk (parent-section
            // breadcrumb for the UI / LLM) in the same query.
            let rows = client
                .query(
                    "SELECT d.id, d.content, d.metadata, d.source_id, d.created_at, \
                            ranked.distance, ranked.section_path \
                     FROM ( \
                         SELECT DISTINCT ON (c.document_id) \
                                c.document_id, \
                                (c.dense_embedding <=> $1::vector) AS distance, \
                                c.section_path \
                         FROM chunks c \
                         JOIN documents d ON d.id = c.document_id \
                         WHERE c.dense_embedding IS NOT NULL \
                           AND ($2::text IS NULL OR d.source_id = $2) \
                         ORDER BY c.document_id, c.dense_embedding <=> $1::vector \
                     ) ranked \
                     JOIN documents d ON d.id = ranked.document_id \
                     ORDER BY ranked.distance ASC \
                     LIMIT $3",
                    &[&vector, &source, &limit],
                )
                .await?;

            rows.iter()
                .map(|row| {
                    let item = row_to_item(row)?;
                    let distance: f64 = row.try_get("distance")?;
                    let section_path: Option<Vec<String>> = row.try_get("section_path")?;
                    Ok(SearchHit {
                        id: item.id.clone(),
                        text: item.text.clone(),
                        metadata: item.metadata.clone(),
                        source_id: item.source_id.clone(),
                        created_at: item.created_at,
                        distance: distance as f32,
                        section_path: section_path.unwrap_or_default(),
                        retrievers: vec!["dense".to_owned()],
                    })
                })
                .collect()
        })
    }

    fn search_hybrid(
        &self,
        _query_text: &str,
        query_embedding: &[f32],
        query_sparse: &[(u32, f32)],
        top_k: usize,
        source_id: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        if query_embedding.is_empty() {
            anyhow::bail!("query embedding cannot be empty");
        }
        // Empty sparse query collapses to dense-only RRF: useful when the
        // embedder couldn't produce a sparse output (legacy export, etc.)
        // or when callers explicitly want dense.
        if query_sparse.is_empty() {
            return self.search(query_embedding, top_k, source_id);
        }

        let pool = self.pool.clone();
        let dense_vec = pgvector::Vector::from(query_embedding.to_vec());
        let sparse_vec = match build_sparsevec(query_sparse) {
            Some(v) => v,
            None => return self.search(query_embedding, top_k, source_id),
        };
        let limit = top_k as i64;
        let source = source_id.map(str::to_owned);
        let half_life_days: f64 = std::env::var("RAG_RECENCY_HALF_LIFE_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f64| *v > 0.0)
            .unwrap_or(365.0);

        // Reciprocal Rank Fusion over per-document scores.
        //
        // For each retriever (dense, sparse) we rank all chunks by similarity,
        // sum 1/(60 + rank) per document so a document with multiple
        // mid-rank chunks beats a single best-chunk doc, and full-outer-join
        // the per-doc scores. Final ordering applies an exponential recency
        // decay using `documents.updated_at`.
        //
        // The 60 constant is the canonical RRF k. We pull 200 chunks per
        // retriever — enough to form a stable per-doc fusion at top_k≤50,
        // overkill above that; tune via $4 if needed.
        const RRF_CANDIDATE_LIMIT: i64 = 200;
        const RRF_K: f64 = 60.0;

        self.block(async move {
            let client = pool.get().await?;
            // SQL is verbose because we materialize ranks once and reuse
            // them across CTEs. Hand-formatted for readability — tokio-postgres
            // can't bind into a CTE inside a `WITH RECURSIVE` rewrite.
            // CTEs:
            //   dense / sparse — per-chunk rank within each retriever (top-N)
            //   *_doc          — per-document RRF score from chunk ranks
            //   *_best_chunk   — section_path of the highest-ranked chunk per doc
            //   fused          — full outer join of the per-doc scores
            // The final SELECT picks section_path from the dense best chunk
            // when available, otherwise the sparse one — both come from real
            // chunks of the same document.
            let sql = "
                WITH dense AS (
                    SELECT c.document_id,
                           c.section_path,
                           ROW_NUMBER() OVER (ORDER BY c.dense_embedding <=> $1::vector) AS rank
                    FROM chunks c
                    JOIN documents d ON d.id = c.document_id
                    WHERE c.dense_embedding IS NOT NULL
                      AND ($3::text IS NULL OR d.source_id = $3)
                    ORDER BY c.dense_embedding <=> $1::vector
                    LIMIT $5
                ),
                sparse AS (
                    SELECT c.document_id,
                           c.section_path,
                           ROW_NUMBER() OVER (ORDER BY c.sparse_embedding <=> $2::sparsevec) AS rank
                    FROM chunks c
                    JOIN documents d ON d.id = c.document_id
                    WHERE c.sparse_embedding IS NOT NULL
                      AND ($3::text IS NULL OR d.source_id = $3)
                    ORDER BY c.sparse_embedding <=> $2::sparsevec
                    LIMIT $5
                ),
                dense_doc AS (
                    SELECT document_id, SUM(1.0 / ($6::float + rank)) AS score
                    FROM dense GROUP BY document_id
                ),
                sparse_doc AS (
                    SELECT document_id, SUM(1.0 / ($6::float + rank)) AS score
                    FROM sparse GROUP BY document_id
                ),
                dense_best AS (
                    SELECT DISTINCT ON (document_id) document_id, section_path
                    FROM dense ORDER BY document_id, rank ASC
                ),
                sparse_best AS (
                    SELECT DISTINCT ON (document_id) document_id, section_path
                    FROM sparse ORDER BY document_id, rank ASC
                ),
                fused AS (
                    SELECT COALESCE(d.document_id, s.document_id) AS document_id,
                           COALESCE(d.score, 0.0) + COALESCE(s.score, 0.0) AS rrf_score,
                           (d.score IS NOT NULL) AS matched_dense,
                           (s.score IS NOT NULL) AS matched_sparse
                    FROM dense_doc d FULL OUTER JOIN sparse_doc s USING (document_id)
                )
                SELECT d.id, d.content, d.metadata, d.source_id, d.created_at,
                       (f.rrf_score
                          * exp(- EXTRACT(EPOCH FROM (now() - d.updated_at))
                                / (86400.0 * $7::float))
                       )::FLOAT8 AS final_score,
                       f.matched_dense,
                       f.matched_sparse,
                       COALESCE(db.section_path, sb.section_path) AS section_path
                FROM fused f
                JOIN documents d ON d.id = f.document_id
                LEFT JOIN dense_best  db ON db.document_id = f.document_id
                LEFT JOIN sparse_best sb ON sb.document_id = f.document_id
                ORDER BY final_score DESC
                LIMIT $4
            ";
            let rows = client
                .query(
                    sql,
                    &[
                        &dense_vec,                  // $1
                        &sparse_vec,                 // $2
                        &source,                     // $3
                        &limit,                      // $4
                        &RRF_CANDIDATE_LIMIT,        // $5
                        &RRF_K,                      // $6
                        &half_life_days,             // $7
                    ],
                )
                .await?;

            rows.iter()
                .map(|row| {
                    let item = row_to_item(row)?;
                    let final_score: f64 = row.try_get("final_score")?;
                    let matched_dense: bool = row.try_get("matched_dense")?;
                    let matched_sparse: bool = row.try_get("matched_sparse")?;
                    let section_path: Option<Vec<String>> = row.try_get("section_path")?;
                    // SearchHit.distance is "lower is better" on the dense
                    // path (cosine distance, 0..2). RRF scores are
                    // unbounded positive, so map them onto the same shape
                    // via 1 - tanh(score * 10):
                    //   score=0    -> distance=1.0  (no signal)
                    //   score=0.05 -> distance≈0.54
                    //   score=0.3  -> distance≈0.005
                    // Keeps frontend percentage bars (computed as
                    // `1 - distance`) sensible and monotonic in RRF score.
                    let pseudo = 1.0 - (final_score * 10.0).tanh();
                    let mut retrievers = Vec::with_capacity(2);
                    if matched_dense  { retrievers.push("dense".to_owned());  }
                    if matched_sparse { retrievers.push("sparse".to_owned()); }
                    Ok(SearchHit {
                        id: item.id.clone(),
                        text: item.text.clone(),
                        metadata: item.metadata.clone(),
                        source_id: item.source_id.clone(),
                        created_at: item.created_at,
                        distance: pseudo as f32,
                        section_path: section_path.unwrap_or_default(),
                        retrievers,
                    })
                })
                .collect()
        })
    }

    fn list_categories(&self) -> Result<Vec<CategorySummary>> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await?;
            let rows = client
                .query(
                    "SELECT source_id, count(*)::bigint AS n FROM documents \
                     GROUP BY source_id ORDER BY n DESC, source_id ASC",
                    &[],
                )
                .await?;
            Ok(rows
                .into_iter()
                .map(|row| CategorySummary {
                    source_id: row.get::<_, String>("source_id"),
                    item_count: row.get::<_, i64>("n"),
                })
                .collect())
        })
    }

    fn list_items(&self, request: ListItemsRequest) -> Result<(Vec<ItemRecord>, i64)> {
        let pool = self.pool.clone();
        let limit = request.limit.unwrap_or(50) as i64;
        let offset = request.offset.unwrap_or(0) as i64;
        let source = request.source_id.clone();
        let order = match request.sort_order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        };
        let min_created = request.min_created_at.map(ms_to_ts);
        let max_created = request.max_created_at.map(ms_to_ts);

        self.block(async move {
            let client = pool.get().await?;

            let count_sql = "SELECT count(*)::bigint FROM documents \
                 WHERE ($1::text IS NULL OR source_id = $1) \
                   AND ($2::timestamptz IS NULL OR created_at >= $2) \
                   AND ($3::timestamptz IS NULL OR created_at <= $3)";
            let total: i64 = client
                .query_one(count_sql, &[&source, &min_created, &max_created])
                .await?
                .get(0);

            let sql = format!(
                "SELECT id, content, metadata, source_id, created_at FROM documents \
                 WHERE ($1::text IS NULL OR source_id = $1) \
                   AND ($2::timestamptz IS NULL OR created_at >= $2) \
                   AND ($3::timestamptz IS NULL OR created_at <= $3) \
                 ORDER BY created_at {order} \
                 LIMIT $4 OFFSET $5"
            );
            let rows = client
                .query(
                    &sql,
                    &[&source, &min_created, &max_created, &limit, &offset],
                )
                .await?;
            let items: Vec<ItemRecord> = rows.iter().map(row_to_item).collect::<Result<_>>()?;
            Ok((items, total))
        })
    }

    fn list_large_items(
        &self,
        min_chars: usize,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<ItemRecord>, i64)> {
        let pool = self.pool.clone();
        let min_chars = min_chars as i32;
        let limit = limit as i64;
        let offset = offset as i64;

        self.block(async move {
            let client = pool.get().await?;
            let total: i64 = client
                .query_one(
                    "SELECT count(*)::bigint FROM documents WHERE char_length(content) >= $1",
                    &[&min_chars],
                )
                .await?
                .get(0);
            let rows = client
                .query(
                    "SELECT id, content, metadata, source_id, created_at FROM documents \
                     WHERE char_length(content) >= $1 \
                     ORDER BY char_length(content) DESC \
                     LIMIT $2 OFFSET $3",
                    &[&min_chars, &limit, &offset],
                )
                .await?;
            let items: Vec<ItemRecord> = rows.iter().map(row_to_item).collect::<Result<_>>()?;
            Ok((items, total))
        })
    }

    fn get_item(&self, id: &str) -> Result<Option<ItemRecord>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let row = client
                .query_opt(
                    "SELECT id, content, metadata, source_id, created_at FROM documents WHERE id = $1",
                    &[&id],
                )
                .await?;
            row.map(|r| row_to_item(&r)).transpose()
        })
    }

    fn delete_item(&self, id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let n = client
                .execute("DELETE FROM documents WHERE id = $1", &[&id])
                .await?;
            Ok(n > 0)
        })
    }

    fn distances_for_ids(
        &self,
        query_embedding: &[f32],
        ids: &[String],
    ) -> Result<Vec<SearchHit>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let pool = self.pool.clone();
        let vector = pgvector::Vector::from(query_embedding.to_vec());
        let ids = ids.to_vec();

        self.block(async move {
            let client = pool.get().await?;
            let rows = client
                .query(
                    "SELECT d.id, d.content, d.metadata, d.source_id, d.created_at, \
                            MIN(c.dense_embedding <=> $1::vector) AS distance \
                     FROM chunks c JOIN documents d ON d.id = c.document_id \
                     WHERE d.id = ANY($2) \
                     GROUP BY d.id",
                    &[&vector, &ids],
                )
                .await?;
            rows.iter()
                .map(|row| {
                    let item = row_to_item(row)?;
                    let distance: f64 = row.try_get("distance")?;
                    Ok(SearchHit {
                        id: item.id.clone(),
                        text: item.text.clone(),
                        metadata: item.metadata.clone(),
                        source_id: item.source_id.clone(),
                        created_at: item.created_at,
                        distance: distance as f32,
                        section_path: Vec::new(),
                        retrievers: vec!["dense".to_owned()],
                    })
                })
                .collect()
        })
    }

    // ── Graph methods. Edges live at the document level; per-document
    // similarity uses MIN(chunk-pair cosine distance) which matches the
    // search aggregation. Cosine via pgvector's `<=>` operator (range 0..2;
    // for L2-normalized vectors that's effectively 1 - cosine_similarity).

    fn graph_status(&self) -> Result<GraphStatus> {
        let pool = self.pool.clone();
        let cfg = self.graph_config;
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let item_count: i64 = client
                .query_one("SELECT count(*)::BIGINT FROM documents", &[])
                .await?
                .try_get(0)?;
            let edge_count: i64 = client
                .query_one("SELECT count(*)::BIGINT FROM graph_edges", &[])
                .await?
                .try_get(0)?;
            let similarity_edge_count: i64 = client
                .query_one(
                    "SELECT count(*)::BIGINT FROM graph_edges WHERE edge_type = 'similarity'",
                    &[],
                )
                .await?
                .try_get(0)?;
            let manual_edge_count: i64 = client
                .query_one(
                    "SELECT count(*)::BIGINT FROM graph_edges WHERE edge_type = 'manual'",
                    &[],
                )
                .await?
                .try_get(0)?;
            Ok(GraphStatus {
                enabled: cfg.enabled,
                build_on_startup: cfg.build_on_startup,
                similarity_top_k: cfg.similarity_top_k,
                similarity_max_distance: cfg.similarity_max_distance,
                cross_source: cfg.cross_source,
                item_count,
                edge_count,
                similarity_edge_count,
                manual_edge_count,
            })
        })
    }

    fn graph_neighborhood(
        &self,
        center_id: &str,
        depth: usize,
        limit: usize,
        edge_type: Option<GraphEdgeType>,
    ) -> Result<GraphNeighborhood> {
        if !self.graph_config.enabled {
            anyhow::bail!("graph features are disabled");
        }
        if limit == 0 {
            anyhow::bail!("limit must be greater than zero");
        }
        if self.get_item(center_id)?.is_none() {
            anyhow::bail!("item {center_id} not found");
        }

        use std::collections::{HashMap, HashSet, VecDeque};
        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut ordered_node_ids: Vec<String> = Vec::new();
        let mut edge_map: HashMap<String, GraphEdgeRecord> = HashMap::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        visited_nodes.insert(center_id.to_owned());
        ordered_node_ids.push(center_id.to_owned());
        queue.push_back((center_id.to_owned(), 0usize));

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }
            for edge in self.list_graph_edges(Some(&current_id), edge_type)? {
                edge_map.entry(edge.id.clone()).or_insert_with(|| edge.clone());
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
            if let Some(item) = self.get_item(node_id)? {
                nodes.push(item);
            }
        }

        let mut edges: Vec<GraphEdgeRecord> = edge_map
            .into_values()
            .filter(|e| {
                visited_nodes.contains(&e.from_item_id)
                    && visited_nodes.contains(&e.to_item_id)
            })
            .collect();
        edges.sort_by(|a, b| a.id.cmp(&b.id));

        let pairwise_distances = pairwise_doc_distances(self, &ordered_node_ids)?;

        Ok(GraphNeighborhood {
            center_id: center_id.to_owned(),
            nodes,
            edges,
            pairwise_distances,
        })
    }

    fn list_graph_edges(
        &self,
        item_id: Option<&str>,
        edge_type: Option<GraphEdgeType>,
    ) -> Result<Vec<GraphEdgeRecord>> {
        let pool = self.pool.clone();
        let item_id = item_id.map(str::to_owned);
        let edge_type_str = edge_type.map(GraphEdgeType::as_str).map(str::to_owned);
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    "SELECT id, from_item_id, to_item_id, edge_type, relation, weight, \
                            directed, metadata, created_at, updated_at \
                     FROM graph_edges \
                     WHERE ($1::TEXT IS NULL OR from_item_id = $1 OR to_item_id = $1) \
                       AND ($2::TEXT IS NULL OR edge_type = $2) \
                     ORDER BY updated_at DESC, id ASC",
                    &[&item_id, &edge_type_str],
                )
                .await?;
            rows.iter().map(row_to_graph_edge).collect()
        })
    }

    fn rebuild_similarity_graph(&self) -> Result<usize> {
        if !self.graph_config.enabled {
            anyhow::bail!("graph features are disabled");
        }
        let pool = self.pool.clone();
        let cfg = self.graph_config;
        let now = current_ms();
        self.block(async move {
            let mut client = pool.get().await.context("acquiring postgres connection")?;
            let tx = client.transaction().await?;
            tx.execute(
                "DELETE FROM graph_edges WHERE edge_type = 'similarity'",
                &[],
            )
            .await?;

            // Per-document MIN cosine distance across chunk pairs, then
            // top-k per origin doc filtered by max_distance. Symmetric edges
            // dedupe via canonical (a < b) ordering already enforced by the
            // cross-join condition.
            let source_filter = if cfg.cross_source {
                ""
            } else {
                "AND da.source_id = db.source_id"
            };
            // Hybrid similarity:
            //   dense_pairs / sparse_pairs — per-(da, db) MIN cosine distance
            //   per retriever, canonical ordering enforced by `db.id > da.id`.
            //   ranked_*  — per-retriever ranks within each origin / target.
            //   fused     — UNION + per-pair RRF score across retrievers; a pair
            //   that scores in only one retriever still appears (single-side
            //   contribution), but pairs that score in both retrievers win.
            // The dense_distance fallback is reported in metadata so the UI
            // can keep showing a meaningful number; pairs that only matched
            // via sparse get NULL dense_distance (and `retrievers='sparse'`).
            let sql = format!(
                "WITH dense_pairs AS ( \
                     SELECT da.id AS from_id, db.id AS to_id, \
                            MIN(ca.dense_embedding <=> cb.dense_embedding)::REAL AS distance \
                     FROM chunks ca \
                     JOIN documents da ON da.id = ca.document_id \
                     JOIN chunks cb ON cb.document_id > ca.document_id \
                     JOIN documents db ON db.id = cb.document_id \
                     WHERE ca.dense_embedding IS NOT NULL \
                       AND cb.dense_embedding IS NOT NULL \
                       {source_filter} \
                     GROUP BY da.id, db.id \
                 ), \
                 sparse_pairs AS ( \
                     SELECT da.id AS from_id, db.id AS to_id, \
                            MIN(ca.sparse_embedding <=> cb.sparse_embedding)::REAL AS distance \
                     FROM chunks ca \
                     JOIN documents da ON da.id = ca.document_id \
                     JOIN chunks cb ON cb.document_id > ca.document_id \
                     JOIN documents db ON db.id = cb.document_id \
                     WHERE ca.sparse_embedding IS NOT NULL \
                       AND cb.sparse_embedding IS NOT NULL \
                       {source_filter} \
                     GROUP BY da.id, db.id \
                 ), \
                 ranked_dense AS ( \
                     SELECT from_id, to_id, distance, \
                            ROW_NUMBER() OVER (PARTITION BY from_id ORDER BY distance ASC, to_id ASC) AS rk_a, \
                            ROW_NUMBER() OVER (PARTITION BY to_id   ORDER BY distance ASC, from_id ASC) AS rk_b \
                     FROM dense_pairs WHERE distance <= $1 \
                 ), \
                 ranked_sparse AS ( \
                     SELECT from_id, to_id, distance, \
                            ROW_NUMBER() OVER (PARTITION BY from_id ORDER BY distance ASC, to_id ASC) AS rk_a, \
                            ROW_NUMBER() OVER (PARTITION BY to_id   ORDER BY distance ASC, from_id ASC) AS rk_b \
                     FROM sparse_pairs \
                 ), \
                 fused AS ( \
                     SELECT COALESCE(d.from_id, s.from_id) AS from_id, \
                            COALESCE(d.to_id,   s.to_id)   AS to_id, \
                            d.distance AS dense_distance, \
                            s.distance AS sparse_distance, \
                            (COALESCE(1.0/(60 + LEAST(d.rk_a, d.rk_b)), 0.0) \
                              + COALESCE(1.0/(60 + LEAST(s.rk_a, s.rk_b)), 0.0))::FLOAT8 AS rrf_score, \
                            (d.distance IS NOT NULL) AS matched_dense, \
                            (s.distance IS NOT NULL) AS matched_sparse \
                     FROM ranked_dense d FULL OUTER JOIN ranked_sparse s \
                       ON s.from_id = d.from_id AND s.to_id = d.to_id \
                 ), \
                 ranked AS ( \
                     SELECT *, \
                            ROW_NUMBER() OVER (PARTITION BY from_id ORDER BY rrf_score DESC) AS rk_a, \
                            ROW_NUMBER() OVER (PARTITION BY to_id   ORDER BY rrf_score DESC) AS rk_b \
                     FROM fused \
                 ) \
                 SELECT from_id, to_id, dense_distance, sparse_distance, rrf_score, \
                        matched_dense, matched_sparse \
                 FROM ranked \
                 WHERE LEAST(rk_a, rk_b) <= $2 \
                 ORDER BY from_id, rrf_score DESC, to_id"
            );
            let max_distance = cfg.similarity_max_distance;
            let top_k = cfg.similarity_top_k as i64;
            let rows = tx
                .query(&sql, &[&max_distance, &top_k])
                .await?;

            let mut inserted = 0_usize;
            for row in rows {
                let from_id: String = row.try_get("from_id")?;
                let to_id: String = row.try_get("to_id")?;
                let dense_distance: Option<f32> = row.try_get("dense_distance")?;
                let sparse_distance: Option<f32> = row.try_get("sparse_distance")?;
                let rrf_score: f64 = row.try_get("rrf_score")?;
                let matched_dense: bool = row.try_get("matched_dense")?;
                let matched_sparse: bool = row.try_get("matched_sparse")?;
                // weight ∈ (0, 1]; rrf_score is bounded by 2/(60+1) ≈ 0.0328 in
                // the limit (rank 1 from both retrievers), so scale to make the
                // weight column carry the signal cleanly.
                let weight = (rrf_score * 30.0).min(1.0).max(0.0) as f32;
                let mut retrievers: Vec<&'static str> = Vec::with_capacity(2);
                if matched_dense  { retrievers.push("dense");  }
                if matched_sparse { retrievers.push("sparse"); }
                let metadata = serde_json::json!({
                    "dense_distance": dense_distance,
                    "sparse_distance": sparse_distance,
                    "rrf_score": rrf_score,
                    "retrievers": retrievers,
                });
                let edge_id = format!("similarity:{from_id}:{to_id}");
                tx.execute(
                    "INSERT INTO graph_edges \
                         (id, from_item_id, to_item_id, edge_type, relation, weight, \
                          directed, metadata, created_at, updated_at) \
                     VALUES ($1, $2, $3, 'similarity', NULL, $4, FALSE, $5, $6, $6)",
                    &[
                        &edge_id,
                        &from_id,
                        &to_id,
                        &weight,
                        &metadata,
                        &now,
                    ],
                )
                .await?;
                inserted += 1;
            }
            tx.commit().await?;
            Ok(inserted)
        })
    }

    fn add_manual_edge(&self, mut input: ManualEdgeInput) -> Result<GraphEdgeRecord> {
        if !self.graph_config.enabled {
            anyhow::bail!("graph features are disabled");
        }
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
        let timestamp = current_ms();
        let edge_id = format!(
            "manual:{}:{}:{}",
            timestamp, input.from_item_id, input.to_item_id
        );
        let pool = self.pool.clone();
        let record = GraphEdgeRecord {
            id: edge_id.clone(),
            from_item_id: input.from_item_id.clone(),
            to_item_id: input.to_item_id.clone(),
            edge_type: GraphEdgeType::Manual,
            relation: input.relation.clone(),
            weight: input.weight,
            directed: input.directed,
            metadata: input.metadata.clone(),
            created_at: timestamp,
            updated_at: timestamp,
        };
        self.block(async move {
            let mut client = pool.get().await.context("acquiring postgres connection")?;
            let tx = client.transaction().await?;
            // FK in the schema enforces both endpoints exist; surface a
            // friendlier error if the lookup fails.
            for endpoint in [&input.from_item_id, &input.to_item_id] {
                let exists = tx
                    .query_opt("SELECT 1 FROM documents WHERE id = $1", &[endpoint])
                    .await?;
                if exists.is_none() {
                    anyhow::bail!("item {endpoint} not found");
                }
            }
            tx.execute(
                "INSERT INTO graph_edges \
                     (id, from_item_id, to_item_id, edge_type, relation, weight, \
                      directed, metadata, created_at, updated_at) \
                 VALUES ($1, $2, $3, 'manual', $4, $5, $6, $7, $8, $8)",
                &[
                    &edge_id,
                    &input.from_item_id,
                    &input.to_item_id,
                    &input.relation,
                    &input.weight,
                    &input.directed,
                    &input.metadata,
                    &timestamp,
                ],
            )
            .await?;
            tx.commit().await?;
            Ok(())
        })?;
        Ok(record)
    }

    fn delete_graph_edge(&self, id: &str) -> Result<bool> {
        if !self.graph_config.enabled {
            anyhow::bail!("graph features are disabled");
        }
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt("SELECT edge_type FROM graph_edges WHERE id = $1", &[&id])
                .await?;
            let Some(row) = row else { return Ok(false) };
            let edge_type: String = row.try_get(0)?;
            if edge_type == "similarity" {
                anyhow::bail!("similarity edges must be rebuilt, not deleted manually");
            }
            let n = client
                .execute("DELETE FROM graph_edges WHERE id = $1", &[&id])
                .await?;
            Ok(n > 0)
        })
    }

    fn get_items_pending_ontology(&self, _limit: usize) -> Result<Vec<ItemRecord>> {
        // Ontology worker is disabled in the Postgres backend until the
        // status column is ported. Returning empty stops the worker from
        // consuming cycles.
        Ok(Vec::new())
    }

    fn mark_ontology_status(&self, _id: &str, _status: &str) -> Result<()> {
        Ok(())
    }
}

fn row_to_message(row: &tokio_postgres::Row) -> Result<MessageRecord> {
    Ok(MessageRecord {
        id: row.try_get("id")?,
        channel: row.try_get("channel")?,
        sender: row.try_get("sender")?,
        sender_kind: MessageSenderKind::from_str(row.try_get::<_, &str>("sender_kind")?)?,
        text: row.try_get("text")?,
        kind: row.try_get("kind")?,
        metadata: row.try_get::<_, Value>("metadata")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

const MESSAGE_COLUMNS: &str =
    "id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at";

impl MessageStore for PostgresVectorStore {
    fn get_message(&self, id: &str) -> Result<Option<MessageRecord>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    &format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = $1"),
                    &[&id],
                )
                .await?;
            row.as_ref().map(row_to_message).transpose()
        })
    }

    fn send_message(&self, message: NewMessage) -> Result<MessageRecord> {
        if message.channel.trim().is_empty() {
            anyhow::bail!("channel cannot be empty");
        }
        if message.sender.trim().is_empty() {
            anyhow::bail!("sender cannot be empty");
        }
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

        let pool = self.pool.clone();
        let record = MessageRecord {
            id: message.id.clone(),
            channel: message.channel.clone(),
            sender: message.sender.clone(),
            sender_kind: message.sender_kind,
            text: message.text.clone(),
            kind: message.kind.clone(),
            metadata: message.metadata.clone(),
            created_at: message.created_at,
            updated_at: message.created_at,
        };
        let sender_kind = message.sender_kind.as_serialized();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "INSERT INTO messages (id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
                    &[
                        &message.id,
                        &message.channel,
                        &message.sender,
                        &sender_kind,
                        &message.text,
                        &message.kind,
                        &message.metadata,
                        &message.created_at,
                    ],
                )
                .await?;
            Ok(())
        })?;
        Ok(record)
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
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let mut client = pool.get().await.context("acquiring postgres connection")?;
            let tx = client.transaction().await?;
            let row = tx
                .query_opt(
                    &format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = $1 FOR UPDATE"),
                    &[&id],
                )
                .await?;
            let Some(row) = row else { return Ok(None) };
            let mut record = row_to_message(&row)?;

            if let Some(new_text) = update.text {
                if update.append_text {
                    record.text.push_str(&new_text);
                } else {
                    record.text = new_text;
                }
            }
            if let Some(new_metadata) = update.metadata {
                record.metadata = new_metadata;
            }
            record.updated_at = now;

            tx.execute(
                "UPDATE messages SET text = $1, metadata = $2, updated_at = $3 WHERE id = $4",
                &[&record.text, &record.metadata, &now, &id],
            )
            .await?;
            tx.commit().await?;
            Ok(Some(record))
        })
    }

    fn delete_message(&self, id: &str) -> Result<Option<MessageRecord>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    &format!(
                        "DELETE FROM messages WHERE id = $1 RETURNING {MESSAGE_COLUMNS}"
                    ),
                    &[&id],
                )
                .await?;
            row.as_ref().map(row_to_message).transpose()
        })
    }

    fn find_permission_request(&self, request_id: &str) -> Result<Vec<MessageRecord>> {
        let pool = self.pool.clone();
        let request_id = request_id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {MESSAGE_COLUMNS} FROM messages \
                         WHERE kind = 'permission_request' \
                           AND metadata->>'request_id' = $1"
                    ),
                    &[&request_id],
                )
                .await?;
            rows.iter().map(row_to_message).collect()
        })
    }

    fn list_channel_messages(&self, channel: &str) -> Result<Vec<MessageRecord>> {
        let pool = self.pool.clone();
        let channel = channel.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {MESSAGE_COLUMNS} FROM messages \
                         WHERE channel = $1 ORDER BY created_at ASC, id ASC"
                    ),
                    &[&channel],
                )
                .await?;
            rows.iter().map(row_to_message).collect()
        })
    }

    fn clear_channel(&self, channel: &str) -> Result<Vec<MessageRecord>> {
        let pool = self.pool.clone();
        let channel = channel.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    &format!(
                        "DELETE FROM messages WHERE channel = $1 RETURNING {MESSAGE_COLUMNS}"
                    ),
                    &[&channel],
                )
                .await?;
            rows.iter().map(row_to_message).collect()
        })
    }

    fn list_messages(&self, query: MessageQuery) -> Result<(Vec<MessageRecord>, i64)> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;

            let mut where_clauses: Vec<String> = Vec::new();
            let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();

            if let Some(channel) = query.channel {
                params.push(Box::new(channel));
                where_clauses.push(format!("channel = ${}", params.len()));
            }
            if let Some(sender) = query.sender {
                params.push(Box::new(sender));
                where_clauses.push(format!("sender = ${}", params.len()));
            }
            if let Some(kind) = query.kind {
                params.push(Box::new(kind));
                where_clauses.push(format!("kind = ${}", params.len()));
            }
            if let Some(min_at) = query.min_created_at {
                params.push(Box::new(min_at));
                let idx = params.len();
                where_clauses
                    .push(format!("(created_at >= ${idx} OR updated_at >= ${idx})"));
            }
            if let Some(max_at) = query.max_created_at {
                params.push(Box::new(max_at));
                where_clauses.push(format!("created_at <= ${}", params.len()));
            }

            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", where_clauses.join(" AND "))
            };

            let limit: i64 = query.limit.unwrap_or(100) as i64;
            let offset: i64 = query.offset.unwrap_or(0) as i64;
            let sort_order = match query.sort_order {
                SortOrder::Asc => "ASC",
                SortOrder::Desc => "DESC",
            };

            let count_sql = format!("SELECT COUNT(*) FROM messages {where_sql}");
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
                .iter()
                .map(|b| b.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();
            let total: i64 = client
                .query_one(&count_sql, &param_refs[..])
                .await?
                .try_get(0)?;

            let limit_idx = params.len() + 1;
            let offset_idx = params.len() + 2;
            let sql = format!(
                "SELECT {MESSAGE_COLUMNS} FROM messages {where_sql} \
                 ORDER BY created_at {sort_order}, id ASC \
                 LIMIT ${limit_idx} OFFSET ${offset_idx}"
            );
            let mut all_params = param_refs;
            all_params.push(&limit);
            all_params.push(&offset);
            let rows = client.query(&sql, &all_params[..]).await?;
            let messages: Vec<MessageRecord> =
                rows.iter().map(row_to_message).collect::<Result<_>>()?;
            Ok((messages, total))
        })
    }

    fn list_channels(&self) -> Result<Vec<ChannelSummary>> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    "SELECT channel, COUNT(*)::BIGINT AS message_count, \
                            MAX(created_at)::BIGINT AS last_at \
                     FROM messages \
                     GROUP BY channel \
                     ORDER BY last_at DESC",
                    &[],
                )
                .await?;
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                out.push(ChannelSummary {
                    channel: row.try_get("channel")?,
                    message_count: row.try_get("message_count")?,
                    last_message_at: row.try_get::<_, Option<i64>>("last_at")?.unwrap_or(0),
                });
            }
            Ok(out)
        })
    }
}

fn row_to_mcp_token(row: &tokio_postgres::Row) -> Result<McpTokenRecord> {
    Ok(McpTokenRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        subject: row.try_get("subject")?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

fn row_to_device_auth(row: &tokio_postgres::Row) -> Result<DeviceAuthRecord> {
    Ok(DeviceAuthRecord {
        device_code: row.try_get("device_code")?,
        user_code: row.try_get("user_code")?,
        status: DeviceAuthStatus::from_str(row.try_get::<_, &str>("status")?)?,
        token_id: row.try_get("token_id")?,
        subject: row.try_get("subject")?,
        client_name: row.try_get("client_name")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        interval_secs: row.try_get("interval_secs")?,
        last_polled_at: row.try_get("last_polled_at")?,
    })
}

const MCP_TOKEN_COLUMNS: &str = "id, name, subject, created_at, last_used_at, expires_at";
const DEVICE_AUTH_COLUMNS: &str = "device_code, user_code, status, token_id, subject, \
                                   client_name, created_at, expires_at, interval_secs, \
                                   last_polled_at";

impl AuthStore for PostgresVectorStore {
    fn create_mcp_token(&self, token: NewMcpToken) -> Result<McpTokenRecord> {
        let pool = self.pool.clone();
        let record = McpTokenRecord {
            id: token.id.clone(),
            name: token.name.clone(),
            subject: token.subject.clone(),
            created_at: token.created_at,
            last_used_at: None,
            expires_at: token.expires_at,
        };
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "INSERT INTO mcp_tokens (id, token_hash, name, subject, created_at, expires_at) \
                     VALUES ($1, $2, $3, $4, $5, $6)",
                    &[
                        &token.id,
                        &token.token_hash,
                        &token.name,
                        &token.subject,
                        &token.created_at,
                        &token.expires_at,
                    ],
                )
                .await?;
            Ok(())
        })?;
        Ok(record)
    }

    fn find_mcp_token_by_hash(&self, hash: &str) -> Result<Option<McpTokenRecord>> {
        let pool = self.pool.clone();
        let hash = hash.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {MCP_TOKEN_COLUMNS} FROM mcp_tokens WHERE token_hash = $1"
                    ),
                    &[&hash],
                )
                .await?;
            row.as_ref().map(row_to_mcp_token).transpose()
        })
    }

    fn touch_mcp_token(&self, id: &str, now: i64) -> Result<()> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "UPDATE mcp_tokens SET last_used_at = $1 WHERE id = $2",
                    &[&now, &id],
                )
                .await?;
            Ok(())
        })
    }

    fn list_mcp_tokens(&self, subject: Option<&str>) -> Result<Vec<McpTokenRecord>> {
        let pool = self.pool.clone();
        let subject = subject.map(str::to_owned);
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {MCP_TOKEN_COLUMNS} FROM mcp_tokens \
                         WHERE ($1::TEXT IS NULL OR subject = $1) \
                         ORDER BY created_at DESC"
                    ),
                    &[&subject],
                )
                .await?;
            rows.iter().map(row_to_mcp_token).collect()
        })
    }

    fn delete_mcp_token(&self, id: &str, subject: Option<&str>) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        let subject = subject.map(str::to_owned);
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "DELETE FROM mcp_tokens WHERE id = $1 AND ($2::TEXT IS NULL OR subject = $2)",
                    &[&id, &subject],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn create_device_auth(&self, request: NewDeviceAuth) -> Result<DeviceAuthRecord> {
        let pool = self.pool.clone();
        let record = DeviceAuthRecord {
            device_code: request.device_code.clone(),
            user_code: request.user_code.clone(),
            status: DeviceAuthStatus::Pending,
            token_id: None,
            subject: None,
            client_name: request.client_name.clone(),
            created_at: request.created_at,
            expires_at: request.expires_at,
            interval_secs: request.interval_secs,
            last_polled_at: None,
        };
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "INSERT INTO device_auth_requests \
                         (device_code, user_code, status, client_name, created_at, expires_at, interval_secs) \
                     VALUES ($1, $2, 'pending', $3, $4, $5, $6)",
                    &[
                        &request.device_code,
                        &request.user_code,
                        &request.client_name,
                        &request.created_at,
                        &request.expires_at,
                        &request.interval_secs,
                    ],
                )
                .await?;
            Ok(())
        })?;
        Ok(record)
    }

    fn find_device_auth_by_device_code(
        &self,
        device_code: &str,
    ) -> Result<Option<DeviceAuthRecord>> {
        let pool = self.pool.clone();
        let device_code = device_code.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {DEVICE_AUTH_COLUMNS} FROM device_auth_requests \
                         WHERE device_code = $1"
                    ),
                    &[&device_code],
                )
                .await?;
            row.as_ref().map(row_to_device_auth).transpose()
        })
    }

    fn find_device_auth_by_user_code(&self, user_code: &str) -> Result<Option<DeviceAuthRecord>> {
        let pool = self.pool.clone();
        let user_code = user_code.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {DEVICE_AUTH_COLUMNS} FROM device_auth_requests \
                         WHERE user_code = $1"
                    ),
                    &[&user_code],
                )
                .await?;
            row.as_ref().map(row_to_device_auth).transpose()
        })
    }

    fn approve_device_auth(
        &self,
        user_code: &str,
        token_id: &str,
        subject: Option<&str>,
        now: i64,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let user_code = user_code.to_owned();
        let token_id = token_id.to_owned();
        let subject = subject.map(str::to_owned);
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "UPDATE device_auth_requests \
                     SET status = 'approved', token_id = $1, subject = $2 \
                     WHERE user_code = $3 AND status = 'pending' AND expires_at > $4",
                    &[&token_id, &subject, &user_code, &now],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn touch_device_poll(&self, device_code: &str, now: i64) -> Result<()> {
        let pool = self.pool.clone();
        let device_code = device_code.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "UPDATE device_auth_requests SET last_polled_at = $1 WHERE device_code = $2",
                    &[&now, &device_code],
                )
                .await?;
            Ok(())
        })
    }

    fn expire_device_auths(&self, now: i64) -> Result<usize> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "UPDATE device_auth_requests SET status = 'expired' \
                     WHERE status = 'pending' AND expires_at <= $1",
                    &[&now],
                )
                .await?;
            Ok(n as usize)
        })
    }
}

impl UserMemoryStore for PostgresVectorStore {
    fn log_user_event(&self, event: NewUserEvent) -> Result<()> {
        let pool = self.pool.clone();
        let event_type = event.event_type.as_str();
        let item_ids_json: Value = serde_json::to_value(&event.item_ids)?;
        let query_vector = event
            .query_embedding
            .as_ref()
            .map(|v| pgvector::Vector::from(v.clone()));
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "INSERT INTO user_events \
                         (id, subject, event_type, query, query_embedding, item_ids, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                    &[
                        &event.id,
                        &event.subject,
                        &event_type,
                        &event.query,
                        &query_vector,
                        &item_ids_json,
                        &event.created_at,
                    ],
                )
                .await?;
            Ok(())
        })
    }

    fn touch_item_accesses(&self, _item_ids: &[String], _now: i64) -> Result<()> {
        // Postgres `documents` schema doesn't track per-item access_count /
        // last_accessed (those columns weren't ported in 0001 because they
        // weren't load-bearing for retrieval). No-op here; reintroduce as a
        // separate concern if popularity boost ever becomes a feature.
        Ok(())
    }

    fn get_user_profile(&self, subject: &str) -> Result<Option<UserProfile>> {
        let pool = self.pool.clone();
        let subject = subject.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    "SELECT subject, interest_embedding, event_horizon, updated_at \
                     FROM user_profiles WHERE subject = $1",
                    &[&subject],
                )
                .await?;
            let Some(row) = row else { return Ok(None) };
            let vector: Option<pgvector::Vector> = row.try_get("interest_embedding")?;
            Ok(Some(UserProfile {
                subject: row.try_get("subject")?,
                interest_embedding: vector.map(|v| v.to_vec()),
                event_horizon: row.try_get("event_horizon")?,
                updated_at: row.try_get("updated_at")?,
            }))
        })
    }

    fn upsert_user_profile(&self, profile: UserProfile) -> Result<()> {
        let pool = self.pool.clone();
        let vector = profile
            .interest_embedding
            .as_ref()
            .map(|v| pgvector::Vector::from(v.clone()));
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "INSERT INTO user_profiles (subject, interest_embedding, event_horizon, updated_at) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT (subject) DO UPDATE SET \
                         interest_embedding = EXCLUDED.interest_embedding, \
                         event_horizon = EXCLUDED.event_horizon, \
                         updated_at = EXCLUDED.updated_at",
                    &[
                        &profile.subject,
                        &vector,
                        &profile.event_horizon,
                        &profile.updated_at,
                    ],
                )
                .await?;
            Ok(())
        })
    }

    fn get_recent_query_embeddings(
        &self,
        subject: &str,
        limit: usize,
    ) -> Result<Vec<Vec<f32>>> {
        let pool = self.pool.clone();
        let subject = subject.to_owned();
        let limit = limit as i64;
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    "SELECT query_embedding FROM user_events \
                     WHERE subject = $1 AND event_type = 'search' AND query_embedding IS NOT NULL \
                     ORDER BY created_at DESC LIMIT $2",
                    &[&subject, &limit],
                )
                .await?;
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let v: pgvector::Vector = row.try_get(0)?;
                out.push(v.to_vec());
            }
            Ok(out)
        })
    }

    fn count_events_since(&self, subject: &str, horizon: i64) -> Result<i64> {
        let pool = self.pool.clone();
        let subject = subject.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_one(
                    "SELECT COUNT(*)::BIGINT FROM user_events \
                     WHERE subject = $1 AND event_type = 'search' AND created_at > $2",
                    &[&subject, &horizon],
                )
                .await?;
            Ok(row.try_get(0)?)
        })
    }
}
