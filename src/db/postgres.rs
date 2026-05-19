use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
use serde_json::Value;
use std::str::FromStr;
use tokio::runtime::Handle;
use tokio_postgres::{Config as PgConfig, NoTls};
use tracing::info;

use super::{
    AttachmentRecord, AuthStore, CategorySummary, ChannelSummary, DeviceAuthRecord,
    DeviceAuthStatus, DocChunk, GraphConfig, GraphEdgeRecord, GraphEdgeType, GraphNeighborhood,
    GraphStatus, ItemAnalysisRecord, ItemRecord, ListItemsRequest, ManualEdgeInput, McpTokenRecord, MessageQuery,
    MessageRecord, MessageSenderKind, MessageStore, MessageUpdate, NewDeviceAuth, NewMcpToken,
    NewMessage, NewOAuthAuthCode, NewUserEvent, OAuthAuthCodeRecord, OAuthCredentialsRecord,
    OAuthCredsStore, OntologyPredicateRecord, PathChild, PathRow, PushStore,
    PushSubscriptionRecord, SchemaRecord, SearchHit, SortOrder, UpsertOAuthCredentials,
    UpsertPushSubscription, UserMemoryStore, UserProfile, VectorStore,
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
        seed_default_ontology_predicates(&client).await?;
    }

    Ok(pool)
}

/// Mirror the SQLite seed path: insert the canonical predicate vocabulary on
/// first boot so the ontology worker has a schema to filter against.
/// Without this, every edge the LLM emits gets dropped with
/// `predicate not in schema`. Idempotent — bails when the table already has rows.
async fn seed_default_ontology_predicates(client: &tokio_postgres::Client) -> Result<()> {
    let row = client
        .query_one("SELECT count(*) FROM ontology_predicates", &[])
        .await
        .context("counting ontology_predicates")?;
    let count: i64 = row.get(0);
    if count > 0 {
        return Ok(());
    }

    use crate::db::default_ontology_predicates;
    let predicates = default_ontology_predicates();
    info!("postgres: seeding {} default ontology predicates", predicates.len());
    for p in predicates {
        // Stored as source_id='*' so the worker's `WHERE source_id=$1 OR source_id='*'`
        // query picks them up regardless of which namespace the item lives in.
        client
            .execute(
                "INSERT INTO ontology_predicates \
                 (name, source_id, description, direction, example_from, example_to) \
                 VALUES ($1, '*', $2, $3, $4, $5) \
                 ON CONFLICT (name, source_id) DO NOTHING",
                &[
                    &p.name,
                    &p.description,
                    &p.direction,
                    &p.example_from,
                    &p.example_to,
                ],
            )
            .await
            .with_context(|| format!("seeding predicate {}", p.name))?;
    }
    Ok(())
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
    (
        "0004_oauth_authorization_codes",
        include_str!("../../migrations/0004_oauth_authorization_codes.sql"),
    ),
    (
        "0005_entry_path",
        include_str!("../../migrations/0005_entry_path.sql"),
    ),
    (
        "0006_attachments",
        include_str!("../../migrations/0006_attachments.sql"),
    ),
    (
        "0007_document_analysis",
        include_str!("../../migrations/0007_document_analysis.sql"),
    ),
    (
        "0008_typed_entries",
        include_str!("../../migrations/0008_typed_entries.sql"),
    ),
    (
        "0009_ontology_predicates",
        include_str!("../../migrations/0009_ontology_predicates.sql"),
    ),
    (
        "0010_documents_ontology_status",
        include_str!("../../migrations/0010_documents_ontology_status.sql"),
    ),
    (
        "0011_ontology_edge_dedup",
        include_str!("../../migrations/0011_ontology_edge_dedup.sql"),
    ),
    (
        "0012_ontology_directed_pair_unique",
        include_str!("../../migrations/0012_ontology_directed_pair_unique.sql"),
    ),
    (
        "0013_user_oauth_credentials",
        include_str!("../../migrations/0013_user_oauth_credentials.sql"),
    ),
    (
        "0014_push_subscriptions",
        include_str!("../../migrations/0014_push_subscriptions.sql"),
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

fn pg_row_to_attachment(row: &tokio_postgres::Row) -> AttachmentRecord {
    let created_at: DateTime<Utc> = row.get("created_at");
    AttachmentRecord {
        id: row.get("id"),
        item_id: row.get("document_id"),
        filename: row.get("filename"),
        stored_name: row.get("stored_name"),
        mime: row.get("mime"),
        size: row.get("size"),
        sha256: row.get("sha256"),
        created_at: ts_to_ms(created_at),
    }
}

fn row_to_item(row: &tokio_postgres::Row) -> Result<ItemRecord> {
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    Ok(ItemRecord {
        id: row.try_get("id")?,
        text: row.try_get("content")?,
        metadata: row.try_get::<_, Value>("metadata")?,
        source_id: row.try_get("source_id")?,
        created_at: ts_to_ms(created_at),
        updated_at: ts_to_ms(updated_at),
        path: row.try_get("path").ok(),
        type_name: row.try_get::<_, Option<String>>("type").ok().flatten(),
        data: row.try_get::<_, Option<Value>>("data").ok().flatten(),
        analysis: row.try_get::<_, Option<Value>>("analysis_json").ok().flatten(),
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
                "INSERT INTO documents (id, source_id, kind, author, content, metadata, tags, status, created_at, updated_at, path, type, data) \
                 VALUES ($1, $2, 'text', $3, $4, $5, $6, $7, $8, now(), $9, $10, $11) \
                 ON CONFLICT (id) DO UPDATE SET \
                     source_id = EXCLUDED.source_id, \
                     author = EXCLUDED.author, \
                     content = EXCLUDED.content, \
                     metadata = EXCLUDED.metadata, \
                     tags = EXCLUDED.tags, \
                     status = EXCLUDED.status, \
                     path = EXCLUDED.path, \
                     type = EXCLUDED.type, \
                     data = EXCLUDED.data, \
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
                    &item.path,
                    &item.type_name,
                    &item.data,
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
                "INSERT INTO documents (id, source_id, kind, author, content, metadata, tags, status, created_at, updated_at, path, type, data) \
                 VALUES ($1, $2, 'text', $3, $4, $5, $6, $7, $8, now(), $9, $10, $11) \
                 ON CONFLICT (id) DO UPDATE SET \
                     source_id = EXCLUDED.source_id, \
                     author = EXCLUDED.author, \
                     content = EXCLUDED.content, \
                     metadata = EXCLUDED.metadata, \
                     tags = EXCLUDED.tags, \
                     status = EXCLUDED.status, \
                     path = EXCLUDED.path, \
                     type = EXCLUDED.type, \
                     data = EXCLUDED.data, \
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
                    &item.path,
                    &item.type_name,
                    &item.data,
                ],
            )
            .await?;
            tx.execute("DELETE FROM chunks WHERE document_id = $1", &[&item.id])
                .await?;

            use tokio_postgres::types::ToSql;

            // Batch up to 1000 chunks per INSERT to avoid query size limits
            for chunk_batch in chunks.chunks(1000) {
                let mut query = String::from(
                    "INSERT INTO chunks (document_id, position, content, section_path, dense_embedding, sparse_embedding, embedding_model, embedding_version) VALUES "
                );
                let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(chunk_batch.len() * 8);

                // We need to store these locally in parallel vecs so they live long enough to be referenced in `params`
                let mut vectors = Vec::with_capacity(chunk_batch.len());
                let mut sparses = Vec::with_capacity(chunk_batch.len());
                let mut section_paths = Vec::with_capacity(chunk_batch.len());

                for chunk in chunk_batch {
                    vectors.push(pgvector::Vector::from(chunk.embedding.clone()));
                    sparses.push(chunk.sparse.as_deref().and_then(build_sparsevec));
                    section_paths.push(if chunk.section_path.is_empty() {
                        None
                    } else {
                        Some(chunk.section_path.as_slice())
                    });
                }

                for (i, chunk) in chunk_batch.iter().enumerate() {
                    if i > 0 {
                        query.push_str(", ");
                    }
                    let base = i * 8;
                    use std::fmt::Write;
                    write!(&mut query, "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                        base + 1, base + 2, base + 3, base + 4,
                        base + 5, base + 6, base + 7, base + 8
                    ).unwrap();

                    params.push(&item.id);
                    params.push(&chunk.position);
                    params.push(&chunk.content);
                    params.push(&section_paths[i]);
                    params.push(&vectors[i]);
                    params.push(&sparses[i]);
                    params.push(&EMBEDDING_MODEL);
                    params.push(&EMBEDDING_VERSION);
                }

                tx.execute(&query, &params).await?;
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
        type_name: Option<&str>,
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
                    "SELECT d.id, d.content, d.metadata, d.source_id, d.created_at, d.updated_at, \
                            ranked.distance, ranked.section_path, ranked.chunk_content, \
                            d.type, d.tags, d.analysis_json \
                     FROM ( \
                         SELECT DISTINCT ON (c.document_id) \
                                c.document_id, \
                                (c.dense_embedding <=> $1::vector) AS distance, \
                                c.section_path, \
                                c.content AS chunk_content \
                         FROM chunks c \
                         JOIN documents d ON d.id = c.document_id \
                         WHERE c.dense_embedding IS NOT NULL \
                           AND ($2::text IS NULL OR d.source_id = $2) \
                           AND ($4::text IS NULL OR d.type = $4) \
                         ORDER BY c.document_id, c.dense_embedding <=> $1::vector \
                     ) ranked \
                     JOIN documents d ON d.id = ranked.document_id \
                     ORDER BY ranked.distance ASC \
                     LIMIT $3",
                    &[&vector, &source, &limit, &type_name],
                )
                .await?;

            rows.iter()
                .map(|row| {
                    let item = row_to_item(row)?;
                    let distance: f64 = row.try_get("distance")?;
                    let section_path: Option<Vec<String>> = row.try_get("section_path")?;
                    let chunk_text: Option<String> = row.try_get("chunk_content")?;
                    Ok(SearchHit {
                        id: item.id.clone(),
                        text: item.text.clone(),
                        metadata: item.metadata.clone(),
                        source_id: item.source_id.clone(),
                        created_at: item.created_at,
                        updated_at: item.updated_at,
                        distance: distance as f32,
                        section_path: section_path.unwrap_or_default(),
                        retrievers: vec!["dense".to_owned()],
                        chunk_text,
                        path: item.path.clone(),
                        type_name: row.try_get::<_, Option<String>>("type")?,
                        tags: row.try_get::<_, Option<Vec<String>>>("tags")?.unwrap_or_default(),
                        analysis: item.analysis.clone(),
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
        type_name: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        if query_embedding.is_empty() {
            anyhow::bail!("query embedding cannot be empty");
        }
        // Empty sparse query collapses to dense-only RRF: useful when the
        // embedder couldn't produce a sparse output (legacy export, etc.)
        // or when callers explicitly want dense.
        if query_sparse.is_empty() {
            return self.search(query_embedding, top_k, source_id, type_name);
        }

        let pool = self.pool.clone();
        let dense_vec = pgvector::Vector::from(query_embedding.to_vec());
        let sparse_vec = match build_sparsevec(query_sparse) {
            Some(v) => v,
            None => return self.search(query_embedding, top_k, source_id, type_name),
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
                           c.content AS chunk_content,
                           ROW_NUMBER() OVER (ORDER BY c.dense_embedding <=> $1::vector) AS rank
                    FROM chunks c
                    JOIN documents d ON d.id = c.document_id
                    WHERE c.dense_embedding IS NOT NULL
                      AND ($3::text IS NULL OR d.source_id = $3)
                      AND ($8::text IS NULL OR d.type = $8)
                    ORDER BY c.dense_embedding <=> $1::vector
                    LIMIT $5
                ),
                sparse AS (
                    SELECT c.document_id,
                           c.section_path,
                           c.content AS chunk_content,
                           ROW_NUMBER() OVER (ORDER BY c.sparse_embedding <=> $2::sparsevec) AS rank
                    FROM chunks c
                    JOIN documents d ON d.id = c.document_id
                    WHERE c.sparse_embedding IS NOT NULL
                      AND ($3::text IS NULL OR d.source_id = $3)
                      AND ($8::text IS NULL OR d.type = $8)
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
                    SELECT DISTINCT ON (document_id) document_id, section_path, chunk_content
                    FROM dense ORDER BY document_id, rank ASC
                ),
                sparse_best AS (
                    SELECT DISTINCT ON (document_id) document_id, section_path, chunk_content
                    FROM sparse ORDER BY document_id, rank ASC
                ),
                fused AS (
                    SELECT COALESCE(d.document_id, s.document_id) AS document_id,
                           COALESCE(d.score, 0.0) + COALESCE(s.score, 0.0) AS rrf_score,
                           (d.score IS NOT NULL) AS matched_dense,
                           (s.score IS NOT NULL) AS matched_sparse
                    FROM dense_doc d FULL OUTER JOIN sparse_doc s USING (document_id)
                )
                SELECT d.id, d.content, d.metadata, d.source_id, d.created_at, d.updated_at,
                       d.type, d.tags, d.analysis_json,
                       (f.rrf_score
                          * exp(- EXTRACT(EPOCH FROM (now() - d.updated_at))
                                / (86400.0 * $7::float))
                       )::FLOAT8 AS final_score,
                       f.matched_dense,
                       f.matched_sparse,
                       COALESCE(db.section_path, sb.section_path) AS section_path,
                       COALESCE(db.chunk_content, sb.chunk_content) AS chunk_content
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
                        &type_name,                  // $8
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
                    let chunk_text: Option<String> = row.try_get("chunk_content")?;
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
                        updated_at: item.updated_at,
                        distance: pseudo as f32,
                        section_path: section_path.unwrap_or_default(),
                        retrievers,
                        chunk_text,
                        path: item.path.clone(),
                        type_name: row.try_get::<_, Option<String>>("type")?,
                        tags: row.try_get::<_, Option<Vec<String>>>("tags")?.unwrap_or_default(),
                        analysis: item.analysis.clone(),
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
        let path_prefix = request.path_prefix.clone().filter(|p| !p.is_empty());
        let type_filter = request.type_name.clone().filter(|t| !t.is_empty());

        self.block(async move {
            let client = pool.get().await?;

            let count_sql = "SELECT count(*)::bigint FROM documents \
                 WHERE ($1::text IS NULL OR source_id = $1) \
                   AND ($2::timestamptz IS NULL OR created_at >= $2) \
                   AND ($3::timestamptz IS NULL OR created_at <= $3) \
                   AND ($4::text IS NULL OR LOWER(path) = LOWER($4) OR LOWER(path) LIKE LOWER($4) || '/%') \
                   AND ($5::text IS NULL OR type = $5)";
            let total: i64 = client
                .query_one(count_sql, &[&source, &min_created, &max_created, &path_prefix, &type_filter])
                .await?
                .get(0);

            let sql = format!(
                "SELECT id, content, metadata, source_id, created_at, updated_at, path, type, data, analysis_json FROM documents \
                 WHERE ($1::text IS NULL OR source_id = $1) \
                   AND ($2::timestamptz IS NULL OR created_at >= $2) \
                   AND ($3::timestamptz IS NULL OR created_at <= $3) \
                   AND ($4::text IS NULL OR LOWER(path) = LOWER($4) OR LOWER(path) LIKE LOWER($4) || '/%') \
                   AND ($5::text IS NULL OR type = $5) \
                 ORDER BY created_at {order} \
                 LIMIT $6 OFFSET $7"
            );
            let rows = client
                .query(
                    &sql,
                    &[&source, &min_created, &max_created, &path_prefix, &type_filter, &limit, &offset],
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
                    "SELECT id, content, metadata, source_id, created_at, updated_at, path, type, data, analysis_json FROM documents WHERE id = $1",
                    &[&id],
                )
                .await?;
            row.map(|r| row_to_item(&r)).transpose()
        })
    }

    fn update_item_analysis(&self, id: &str, json: &str, model: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        let parsed: Value = serde_json::from_str(json).unwrap_or(Value::Null);
        let model = model.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let now = chrono::Utc::now();
            let n = client
                .execute(
                    "UPDATE documents SET analysis_json = $1, analysis_at = $2, analysis_model = $3 WHERE id = $4",
                    &[&parsed, &now, &model, &id],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn get_item_analysis(&self, id: &str) -> Result<Option<ItemAnalysisRecord>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let row = client
                .query_opt(
                    "SELECT analysis_json, analysis_at, analysis_model FROM documents WHERE id = $1",
                    &[&id],
                )
                .await?;
            let Some(row) = row else { return Ok(None) };
            let json: Option<Value> = row.get(0);
            let at: Option<DateTime<Utc>> = row.get(1);
            let model: Option<String> = row.get(2);
            match (json, at, model) {
                (Some(j), Some(a), Some(m)) if !j.is_null() => Ok(Some(ItemAnalysisRecord {
                    analysis: j,
                    analysis_at: ts_to_ms(a),
                    analysis_model: m,
                })),
                _ => Ok(None),
            }
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

    fn insert_attachment(&self, record: AttachmentRecord) -> Result<()> {
        let pool = self.pool.clone();
        let created_at = ms_to_ts(record.created_at);
        self.block(async move {
            let client = pool.get().await?;
            client
                .execute(
                    "INSERT INTO attachments (id, document_id, filename, stored_name, mime, size, sha256, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[
                        &record.id,
                        &record.item_id,
                        &record.filename,
                        &record.stored_name,
                        &record.mime,
                        &record.size,
                        &record.sha256,
                        &created_at,
                    ],
                )
                .await?;
            Ok(())
        })
    }

    fn list_attachments_for_item(&self, item_id: &str) -> Result<Vec<AttachmentRecord>> {
        let pool = self.pool.clone();
        let id = item_id.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let rows = client
                .query(
                    "SELECT id, document_id, filename, stored_name, mime, size, sha256, created_at \
                     FROM attachments WHERE document_id = $1 ORDER BY created_at DESC",
                    &[&id],
                )
                .await?;
            Ok(rows.iter().map(pg_row_to_attachment).collect())
        })
    }

    fn get_attachment(&self, id: &str) -> Result<Option<AttachmentRecord>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let row = client
                .query_opt(
                    "SELECT id, document_id, filename, stored_name, mime, size, sha256, created_at \
                     FROM attachments WHERE id = $1",
                    &[&id],
                )
                .await?;
            Ok(row.as_ref().map(pg_row_to_attachment))
        })
    }

    fn delete_attachment(&self, id: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let mut client = pool.get().await?;
            let tx = client.transaction().await?;
            let row = tx
                .query_opt(
                    "SELECT stored_name FROM attachments WHERE id = $1",
                    &[&id],
                )
                .await?;
            let stored_name: Option<String> = row.map(|r| r.get(0));
            if stored_name.is_some() {
                tx.execute("DELETE FROM attachments WHERE id = $1", &[&id])
                    .await?;
            }
            tx.commit().await?;
            Ok(stored_name)
        })
    }

    fn list_all_paths(
        &self,
        source_id_filter: Option<&str>,
    ) -> Result<Vec<PathRow>> {
        let pool = self.pool.clone();
        let filter = source_id_filter.map(|s| s.to_owned());
        self.block(async move {
            let client = pool.get().await?;
            let rows = if let Some(s) = &filter {
                client
                    .query(
                        "SELECT source_id, path, COUNT(*)::bigint AS cnt FROM documents \
                         WHERE path IS NOT NULL AND path <> '' AND source_id = $1 \
                         GROUP BY source_id, path \
                         ORDER BY source_id ASC, path ASC",
                        &[&s],
                    )
                    .await?
            } else {
                client
                    .query(
                        "SELECT source_id, path, COUNT(*)::bigint AS cnt FROM documents \
                         WHERE path IS NOT NULL AND path <> '' \
                         GROUP BY source_id, path \
                         ORDER BY source_id ASC, path ASC",
                        &[],
                    )
                    .await?
            };
            Ok(rows
                .iter()
                .map(|r| PathRow {
                    source_id: r.get::<_, String>("source_id"),
                    path: r.get::<_, String>("path"),
                    count: r.get::<_, i64>("cnt"),
                })
                .collect())
        })
    }

    fn list_path_children(
        &self,
        source_id: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<PathChild>> {
        let pool = self.pool.clone();
        let source = source_id.to_owned();
        let prefix_norm = prefix
            .map(|p| p.trim_matches('/').to_owned())
            .filter(|p| !p.is_empty());
        self.block(async move {
            let client = pool.get().await?;
            let rows = if let Some(p) = &prefix_norm {
                let sql = "WITH rels AS ( \
                    SELECT substring(path FROM length($2) + 2) AS rel \
                    FROM documents \
                    WHERE source_id = $1 \
                      AND path IS NOT NULL \
                      AND LOWER(path) LIKE LOWER($2) || '/%' \
                ), heads AS ( \
                    SELECT split_part(rel, '/', 1) AS segment, \
                           (position('/' IN rel) > 0) AS deeper \
                    FROM rels WHERE rel IS NOT NULL AND rel <> '' \
                ) \
                SELECT segment, COUNT(*)::bigint AS cnt, BOOL_OR(deeper) AS has_children \
                FROM heads GROUP BY segment ORDER BY segment ASC";
                client.query(sql, &[&source, &p]).await?
            } else {
                let sql = "WITH rels AS ( \
                    SELECT path AS rel FROM documents \
                    WHERE source_id = $1 AND path IS NOT NULL AND path <> '' \
                ), heads AS ( \
                    SELECT split_part(rel, '/', 1) AS segment, \
                           (position('/' IN rel) > 0) AS deeper \
                    FROM rels \
                ) \
                SELECT segment, COUNT(*)::bigint AS cnt, BOOL_OR(deeper) AS has_children \
                FROM heads GROUP BY segment ORDER BY segment ASC";
                client.query(sql, &[&source]).await?
            };
            Ok(rows
                .iter()
                .map(|r| PathChild {
                    segment: r.get::<_, String>("segment"),
                    count: r.get::<_, i64>("cnt"),
                    has_children: r.get::<_, bool>("has_children"),
                })
                .collect())
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
                    "SELECT d.id, d.content, d.metadata, d.source_id, d.created_at, d.updated_at, \
                            d.path, d.type, d.tags, d.analysis_json, \
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
                        updated_at: item.updated_at,
                        distance: distance as f32,
                        section_path: Vec::new(),
                        retrievers: vec!["dense".to_owned()],
                        chunk_text: None,
                        path: row.try_get("path")?,
                        type_name: row.try_get("type")?,
                        tags: row.try_get::<_, Option<Vec<String>>>("tags")?.unwrap_or_default(),
                        analysis: item.analysis.clone(),
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
            for edge in self.list_graph_edges(Some(&current_id), edge_type, None)? {
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

    fn get_graph_edge(&self, id: &str) -> Result<Option<GraphEdgeRecord>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    "SELECT id, from_item_id, to_item_id, edge_type, relation, weight, \
                            directed, metadata, created_at, updated_at \
                     FROM graph_edges WHERE id = $1",
                    &[&id],
                )
                .await?;
            row.as_ref().map(row_to_graph_edge).transpose()
        })
    }

    fn list_graph_edges(
        &self,
        item_id: Option<&str>,
        edge_type: Option<GraphEdgeType>,
        status: Option<&str>,
    ) -> Result<Vec<GraphEdgeRecord>> {
        let pool = self.pool.clone();
        let item_id = item_id.map(str::to_owned);
        let edge_type_str = edge_type.map(GraphEdgeType::as_str).map(str::to_owned);
        let status = status.map(str::to_owned);
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    "SELECT id, from_item_id, to_item_id, edge_type, relation, weight, \
                            directed, metadata, created_at, updated_at \
                     FROM graph_edges \
                     WHERE ($1::TEXT IS NULL OR from_item_id = $1 OR to_item_id = $1) \
                       AND ($2::TEXT IS NULL OR edge_type = $2) \
                       AND ($3::TEXT IS NULL OR metadata->>'status' = $3) \
                     ORDER BY updated_at DESC, id ASC",
                    &[&item_id, &edge_type_str, &status],
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
        let relation_owned = input.relation.map(|r| r.into_owned());
        // Detect ontology-worker source so we can de-dup against the partial
        // unique index from migration 0011. Other manual edges (human curator)
        // are not covered by that index and always insert fresh.
        let from_ontology = input
            .metadata
            .get("source")
            .and_then(|v| v.as_str())
            == Some("ontology_worker");
        let record = self.block(async move {
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
            // For ontology-worker edges, an existing row for the same
            // (from, to, relation) tuple is a no-op — keep the original
            // verdict (and any HITL approval/rejection state). Return the
            // existing record so callers can log it consistently.
            let inserted_row = if from_ontology {
                tx.query_opt(
                    "INSERT INTO graph_edges \
                         (id, from_item_id, to_item_id, edge_type, relation, weight, \
                          directed, metadata, created_at, updated_at) \
                     VALUES ($1, $2, $3, 'manual', $4, $5, $6, $7, $8, $8) \
                     ON CONFLICT (from_item_id, to_item_id) \
                         WHERE edge_type = 'manual' AND metadata->>'source' = 'ontology_worker' \
                     DO NOTHING \
                     RETURNING id, from_item_id, to_item_id, edge_type, relation, weight, \
                               directed, metadata, created_at, updated_at",
                    &[
                        &edge_id,
                        &input.from_item_id,
                        &input.to_item_id,
                        &relation_owned,
                        &input.weight,
                        &input.directed,
                        &input.metadata,
                        &timestamp,
                    ],
                )
                .await?
            } else {
                Some(
                    tx.query_one(
                        "INSERT INTO graph_edges \
                             (id, from_item_id, to_item_id, edge_type, relation, weight, \
                              directed, metadata, created_at, updated_at) \
                         VALUES ($1, $2, $3, 'manual', $4, $5, $6, $7, $8, $8) \
                         RETURNING id, from_item_id, to_item_id, edge_type, relation, weight, \
                                   directed, metadata, created_at, updated_at",
                        &[
                            &edge_id,
                            &input.from_item_id,
                            &input.to_item_id,
                            &relation_owned,
                            &input.weight,
                            &input.directed,
                            &input.metadata,
                            &timestamp,
                        ],
                    )
                    .await?,
                )
            };
            let final_row = match inserted_row {
                Some(row) => row,
                None => {
                    // Conflict — fetch and return the existing edge so the
                    // caller's logging stays meaningful.
                    tx.query_one(
                        "SELECT id, from_item_id, to_item_id, edge_type, relation, weight, \
                                directed, metadata, created_at, updated_at \
                         FROM graph_edges \
                         WHERE edge_type = 'manual' \
                           AND metadata->>'source' = 'ontology_worker' \
                           AND from_item_id = $1 AND to_item_id = $2",
                        &[&input.from_item_id, &input.to_item_id],
                    )
                    .await
                    .context("fetching existing ontology edge after ON CONFLICT")?
                }
            };
            let existing = row_to_graph_edge(&final_row)?;
            tx.commit().await?;
            Ok(existing)
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

    fn update_graph_edge(&self, id: &str, relation: Option<String>, metadata: Value) -> Result<GraphEdgeRecord> {
        if !self.graph_config.enabled {
            anyhow::bail!("graph features are disabled");
        }
        let pool = self.pool.clone();
        let id = id.to_owned();
        let timestamp = current_ms();
        self.block(async move {
            let mut client = pool.get().await.context("acquiring postgres connection")?;
            let tx = client.transaction().await?;
            
            let row = tx
                .query_opt("SELECT id FROM graph_edges WHERE id = $1 FOR UPDATE", &[&id])
                .await?;
            if row.is_none() {
                anyhow::bail!("edge {id} not found");
            }

            tx.execute(
                "UPDATE graph_edges SET relation = $1, metadata = $2, updated_at = $3 WHERE id = $4",
                &[&relation, &metadata, &timestamp, &id],
            )
            .await?;

            let row = tx.query_one(
                "SELECT id, from_item_id, to_item_id, edge_type, relation, weight, directed, metadata, created_at, updated_at \
                 FROM graph_edges WHERE id = $1",
                &[&id],
            ).await?;

            let edge_type_str: String = row.get(3);
            let edge = GraphEdgeRecord {
                id: row.get(0),
                from_item_id: row.get(1),
                to_item_id: row.get(2),
                edge_type: GraphEdgeType::from_str(&edge_type_str).unwrap_or(GraphEdgeType::Manual),
                relation: row.get(4),
                weight: row.get(5),
                directed: row.get(6),
                metadata: row.get(7),
                created_at: row.get(8),
                updated_at: row.get(9),
            };

            tx.commit().await?;
            Ok(edge)
        })
    }

    fn get_items_pending_ontology(&self, limit: usize) -> Result<Vec<ItemRecord>> {
        let pool = self.pool.clone();
        let limit = limit as i64;
        self.block(async move {
            let client = pool.get().await?;
            // Mirrors the SQLite path: oldest pending rows first, capped by limit.
            // Selects the same column set as row_to_item so a single helper handles both.
            let rows = client
                .query(
                    "SELECT id, content, metadata, source_id, created_at, updated_at, path, type, data, analysis_json \
                     FROM documents \
                     WHERE ontology_status = 'pending' \
                     ORDER BY created_at ASC \
                     LIMIT $1",
                    &[&limit],
                )
                .await?;
            rows.iter().map(row_to_item).collect()
        })
    }

    fn mark_ontology_status(&self, id: &str, status: &str) -> Result<()> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        let status = status.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            client
                .execute(
                    "UPDATE documents SET ontology_status = $1 WHERE id = $2",
                    &[&status, &id],
                )
                .await?;
            Ok(())
        })
    }

    fn list_ontology_predicates(&self, source_id: Option<&str>) -> Result<Vec<OntologyPredicateRecord>> {
        let pool = self.pool.clone();
        let sid = source_id.map(|s| s.to_owned()).unwrap_or_else(|| "*".to_owned());
        self.block(async move {
            let client = pool.get().await?;
            let rows = client
                .query(
                    "SELECT name, source_id, description, direction, example_from, example_to, created_at, updated_at
                     FROM ontology_predicates WHERE source_id = $1 OR source_id = '*'
                     ORDER BY name ASC, source_id DESC",
                    &[&sid],
                )
                .await?;
            let mut predicates = rows
                .into_iter()
                .map(|row| {
                    let sid_val: String = row.get("source_id");
                    let created: DateTime<Utc> = row.get("created_at");
                    let updated: DateTime<Utc> = row.get("updated_at");
                    OntologyPredicateRecord {
                        name: row.get("name"),
                        source_id: if sid_val == "*" { None } else { Some(sid_val) },
                        description: row.get("description"),
                        direction: row.get("direction"),
                        example_from: row.get("example_from"),
                        example_to: row.get("example_to"),
                        created_at: ts_to_ms(created),
                        updated_at: ts_to_ms(updated),
                    }
                })
                .collect::<Vec<_>>();
            predicates.dedup_by(|a, b| a.name == b.name);
            Ok(predicates)
        })
    }

    fn get_ontology_predicate(&self, name: &str, source_id: Option<&str>) -> Result<Option<OntologyPredicateRecord>> {
        let pool = self.pool.clone();
        let name = name.to_owned();
        let sid = source_id.map(|s| s.to_owned()).unwrap_or_else(|| "*".to_owned());
        self.block(async move {
            let client = pool.get().await?;
            let row = client
                .query_opt(
                    "SELECT name, source_id, description, direction, example_from, example_to, created_at, updated_at
                     FROM ontology_predicates WHERE name = $1 AND (source_id = $2 OR source_id = '*')
                     ORDER BY source_id DESC LIMIT 1",
                    &[&name, &sid],
                )
                .await?;
            Ok(row.map(|row| {
                let sid_val: String = row.get("source_id");
                let created: DateTime<Utc> = row.get("created_at");
                let updated: DateTime<Utc> = row.get("updated_at");
                OntologyPredicateRecord {
                    name: row.get("name"),
                    source_id: if sid_val == "*" { None } else { Some(sid_val) },
                    description: row.get("description"),
                    direction: row.get("direction"),
                    example_from: row.get("example_from"),
                    example_to: row.get("example_to"),
                    created_at: ts_to_ms(created),
                    updated_at: ts_to_ms(updated),
                }
            }))
        })
    }

    fn upsert_ontology_predicate(&self, record: OntologyPredicateRecord) -> Result<()> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await?;
            let sid = record.source_id.as_deref().unwrap_or("*");
            client
                .execute(
                    "INSERT INTO ontology_predicates (name, source_id, description, direction, example_from, example_to, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, now(), now())
                     ON CONFLICT (name, source_id) DO UPDATE SET
                         description = EXCLUDED.description,
                         direction = EXCLUDED.direction,
                         example_from = EXCLUDED.example_from,
                         example_to = EXCLUDED.example_to,
                         updated_at = now()",
                    &[
                        &record.name,
                        &sid,
                        &record.description,
                        &record.direction,
                        &record.example_from,
                        &record.example_to,
                    ],
                )
                .await?;
            Ok(())
        })
    }

    fn delete_ontology_predicate(&self, name: &str, source_id: Option<&str>) -> Result<bool> {
        let pool = self.pool.clone();
        let name = name.to_owned();
        let sid = source_id.map(|s| s.to_owned()).unwrap_or_else(|| "*".to_owned());
        self.block(async move {
            let client = pool.get().await?;
            let n = client
                .execute(
                    "DELETE FROM ontology_predicates WHERE name = $1 AND source_id = $2",
                    &[&name, &sid],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn list_schemas(&self) -> Result<Vec<SchemaRecord>> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await?;
            let rows = client
                .query(
                    "SELECT type_name, json_schema, title, description, created_at, updated_at
                     FROM schemas ORDER BY type_name ASC",
                    &[],
                )
                .await?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    let created: DateTime<Utc> = row.get("created_at");
                    let updated: DateTime<Utc> = row.get("updated_at");
                    SchemaRecord {
                        type_name: row.get("type_name"),
                        json_schema: row.get::<_, Value>("json_schema"),
                        title: row.get("title"),
                        description: row.get("description"),
                        created_at: ts_to_ms(created),
                        updated_at: ts_to_ms(updated),
                    }
                })
                .collect())
        })
    }

    fn get_schema(&self, type_name: &str) -> Result<Option<SchemaRecord>> {
        let pool = self.pool.clone();
        let type_name = type_name.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let row = client
                .query_opt(
                    "SELECT type_name, json_schema, title, description, created_at, updated_at
                     FROM schemas WHERE type_name = $1",
                    &[&type_name],
                )
                .await?;
            Ok(row.map(|row| {
                let created: DateTime<Utc> = row.get("created_at");
                let updated: DateTime<Utc> = row.get("updated_at");
                SchemaRecord {
                    type_name: row.get("type_name"),
                    json_schema: row.get::<_, Value>("json_schema"),
                    title: row.get("title"),
                    description: row.get("description"),
                    created_at: ts_to_ms(created),
                    updated_at: ts_to_ms(updated),
                }
            }))
        })
    }

    fn upsert_schema(&self, record: SchemaRecord) -> Result<()> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await?;
            client
                .execute(
                    "INSERT INTO schemas (type_name, json_schema, title, description, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, now(), now())
                     ON CONFLICT (type_name) DO UPDATE SET
                         json_schema = EXCLUDED.json_schema,
                         title = EXCLUDED.title,
                         description = EXCLUDED.description,
                         updated_at = now()",
                    &[
                        &record.type_name,
                        &record.json_schema,
                        &record.title,
                        &record.description,
                    ],
                )
                .await?;
            Ok(())
        })
    }

    fn delete_schema(&self, type_name: &str, force: bool) -> Result<(bool, usize)> {
        let pool = self.pool.clone();
        let type_name = type_name.to_owned();
        self.block(async move {
            let mut client = pool.get().await?;
            let tx = client.transaction().await?;
            let count: i64 = tx
                .query_one(
                    "SELECT count(*)::bigint FROM documents WHERE type = $1",
                    &[&type_name],
                )
                .await?
                .get(0);
            if count > 0 && !force {
                anyhow::bail!("schema {type_name} is referenced by {count} items");
            }
            let unset = if count > 0 {
                tx.execute(
                    "UPDATE documents SET type = NULL, data = NULL WHERE type = $1",
                    &[&type_name],
                )
                .await? as usize
            } else {
                0
            };
            let n = tx
                .execute("DELETE FROM schemas WHERE type_name = $1", &[&type_name])
                .await?;
            tx.commit().await?;
            Ok((n > 0, unset))
        })
    }

    fn count_items_by_type(&self, type_name: &str) -> Result<i64> {
        let pool = self.pool.clone();
        let type_name = type_name.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            let n: i64 = client
                .query_one(
                    "SELECT count(*)::bigint FROM documents WHERE type = $1",
                    &[&type_name],
                )
                .await?
                .get(0);
            Ok(n)
        })
    }

    fn merge_item_tags(&self, id: &str, tags: &[String]) -> Result<bool> {
        if tags.is_empty() {
            return Ok(true);
        }
        let pool = self.pool.clone();
        let id = id.to_owned();
        let new_tags: Vec<String> = tags
            .iter()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect();
        self.block(async move {
            let mut client = pool.get().await?;
            let tx = client.transaction().await?;
            let row = tx
                .query_opt(
                    "SELECT metadata, tags FROM documents WHERE id = $1 FOR UPDATE",
                    &[&id],
                )
                .await?;
            let Some(row) = row else {
                return Ok(false);
            };
            let mut metadata: Value = row.try_get("metadata").unwrap_or(Value::Object(Default::default()));
            let column_tags: Vec<String> = row.try_get("tags").unwrap_or_default();
            let obj = metadata
                .as_object_mut()
                .context("metadata is not a JSON object")?;
            let mut merged: Vec<String> = obj
                .get("tags")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            for t in &column_tags {
                if !merged.iter().any(|m| m == t) {
                    merged.push(t.clone());
                }
            }
            for t in &new_tags {
                if !merged.iter().any(|m| m == t) {
                    merged.push(t.clone());
                }
            }
            obj.insert("tags".to_owned(), serde_json::json!(merged));
            tx.execute(
                "UPDATE documents SET metadata = $1, tags = $2, updated_at = now() WHERE id = $3",
                &[&metadata, &merged, &id],
            )
            .await?;
            tx.commit().await?;
            Ok(true)
        })
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

fn row_to_auth_code(row: &tokio_postgres::Row) -> Result<OAuthAuthCodeRecord> {
    Ok(OAuthAuthCodeRecord {
        code: row.try_get("code")?,
        client_id: row.try_get("client_id")?,
        redirect_uri: row.try_get("redirect_uri")?,
        code_challenge: row.try_get("code_challenge")?,
        challenge_method: row.try_get("challenge_method")?,
        scope: row.try_get("scope")?,
        subject: row.try_get("subject")?,
        token_id: row.try_get("token_id")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        consumed_at: row.try_get("consumed_at")?,
    })
}

const MCP_TOKEN_COLUMNS: &str = "id, name, subject, created_at, last_used_at, expires_at";
const DEVICE_AUTH_COLUMNS: &str = "device_code, user_code, status, token_id, subject, \
                                   client_name, created_at, expires_at, interval_secs, \
                                   last_polled_at";
const AUTH_CODE_COLUMNS: &str = "code, client_id, redirect_uri, code_challenge, \
                                 challenge_method, scope, subject, token_id, \
                                 created_at, expires_at, consumed_at";

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

    fn create_auth_code(&self, code: NewOAuthAuthCode) -> Result<OAuthAuthCodeRecord> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "INSERT INTO oauth_authorization_codes \
                         (code, client_id, redirect_uri, code_challenge, challenge_method, \
                          scope, subject, created_at, expires_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                    &[
                        &code.code,
                        &code.client_id,
                        &code.redirect_uri,
                        &code.code_challenge,
                        &code.challenge_method,
                        &code.scope,
                        &code.subject,
                        &code.created_at,
                        &code.expires_at,
                    ],
                )
                .await?;
            Ok(OAuthAuthCodeRecord {
                code: code.code,
                client_id: code.client_id,
                redirect_uri: code.redirect_uri,
                code_challenge: code.code_challenge,
                challenge_method: code.challenge_method,
                scope: code.scope,
                subject: code.subject,
                token_id: None,
                created_at: code.created_at,
                expires_at: code.expires_at,
                consumed_at: None,
            })
        })
    }

    fn find_auth_code(&self, code: &str) -> Result<Option<OAuthAuthCodeRecord>> {
        let pool = self.pool.clone();
        let code = code.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {AUTH_CODE_COLUMNS} \
                         FROM oauth_authorization_codes WHERE code = $1"
                    ),
                    &[&code],
                )
                .await?;
            row.as_ref().map(row_to_auth_code).transpose()
        })
    }

    fn consume_auth_code(&self, code: &str, token_id: &str, now: i64) -> Result<bool> {
        let pool = self.pool.clone();
        let code = code.to_owned();
        let token_id = token_id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "UPDATE oauth_authorization_codes \
                     SET consumed_at = $1, token_id = $2 \
                     WHERE code = $3 AND consumed_at IS NULL AND expires_at > $1",
                    &[&now, &token_id, &code],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn expire_auth_codes(&self, now: i64) -> Result<usize> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "DELETE FROM oauth_authorization_codes WHERE expires_at <= $1",
                    &[&now],
                )
                .await?;
            Ok(n as usize)
        })
    }
}

const OAUTH_CREDS_COLUMNS: &str =
    "subject, provider, access_token_enc, refresh_token_enc, scopes, expires_at, account_email, \
     created_at, updated_at";

fn row_to_oauth_creds(row: &tokio_postgres::Row) -> Result<OAuthCredentialsRecord> {
    Ok(OAuthCredentialsRecord {
        subject: row.try_get("subject")?,
        provider: row.try_get("provider")?,
        access_token_enc: row.try_get("access_token_enc")?,
        refresh_token_enc: row.try_get("refresh_token_enc")?,
        scopes: row.try_get("scopes")?,
        expires_at: row.try_get("expires_at")?,
        account_email: row.try_get("account_email")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl OAuthCredsStore for PostgresVectorStore {
    fn upsert_oauth_credentials(
        &self,
        creds: UpsertOAuthCredentials,
    ) -> Result<OAuthCredentialsRecord> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            // Preserve created_at on conflict; refresh_token + account_email
            // fall back to existing values if the new one is NULL (Google
            // omits refresh_token on re-consent if granted previously).
            let row = client
                .query_one(
                    &format!(
                        "INSERT INTO user_oauth_credentials \
                             ({OAUTH_CREDS_COLUMNS}) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8) \
                         ON CONFLICT (subject, provider) DO UPDATE SET \
                             access_token_enc = EXCLUDED.access_token_enc, \
                             refresh_token_enc = COALESCE(EXCLUDED.refresh_token_enc, user_oauth_credentials.refresh_token_enc), \
                             scopes = EXCLUDED.scopes, \
                             expires_at = EXCLUDED.expires_at, \
                             account_email = COALESCE(EXCLUDED.account_email, user_oauth_credentials.account_email), \
                             updated_at = EXCLUDED.updated_at \
                         RETURNING {OAUTH_CREDS_COLUMNS}"
                    ),
                    &[
                        &creds.subject,
                        &creds.provider,
                        &creds.access_token_enc,
                        &creds.refresh_token_enc,
                        &creds.scopes,
                        &creds.expires_at,
                        &creds.account_email,
                        &creds.now,
                    ],
                )
                .await?;
            row_to_oauth_creds(&row)
        })
    }

    fn find_oauth_credentials(
        &self,
        subject: &str,
        provider: &str,
    ) -> Result<Option<OAuthCredentialsRecord>> {
        let pool = self.pool.clone();
        let subject = subject.to_owned();
        let provider = provider.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {OAUTH_CREDS_COLUMNS} FROM user_oauth_credentials \
                         WHERE subject = $1 AND provider = $2"
                    ),
                    &[&subject, &provider],
                )
                .await?;
            row.as_ref().map(row_to_oauth_creds).transpose()
        })
    }

    fn delete_oauth_credentials(&self, subject: &str, provider: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let subject = subject.to_owned();
        let provider = provider.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "DELETE FROM user_oauth_credentials WHERE subject = $1 AND provider = $2",
                    &[&subject, &provider],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn list_oauth_providers(&self, subject: &str) -> Result<Vec<OAuthCredentialsRecord>> {
        let pool = self.pool.clone();
        let subject = subject.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {OAUTH_CREDS_COLUMNS} FROM user_oauth_credentials \
                         WHERE subject = $1 ORDER BY provider"
                    ),
                    &[&subject],
                )
                .await?;
            rows.iter().map(row_to_oauth_creds).collect()
        })
    }
}

const PUSH_SUB_COLUMNS: &str =
    "id, subject, endpoint, p256dh, auth, user_agent, created_at, last_used_at";

fn row_to_push_sub(row: &tokio_postgres::Row) -> Result<PushSubscriptionRecord> {
    Ok(PushSubscriptionRecord {
        id: row.try_get("id")?,
        subject: row.try_get("subject")?,
        endpoint: row.try_get("endpoint")?,
        p256dh: row.try_get("p256dh")?,
        auth: row.try_get("auth")?,
        user_agent: row.try_get("user_agent")?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
    })
}

impl PushStore for PostgresVectorStore {
    fn upsert_push_subscription(
        &self,
        sub: UpsertPushSubscription,
    ) -> Result<PushSubscriptionRecord> {
        let pool = self.pool.clone();
        let id = uuid::Uuid::now_v7().to_string();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let row = client
                .query_one(
                    &format!(
                        "INSERT INTO push_subscriptions \
                             ({PUSH_SUB_COLUMNS}) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, NULL) \
                         ON CONFLICT (subject, endpoint) DO UPDATE SET \
                             p256dh = EXCLUDED.p256dh, \
                             auth = EXCLUDED.auth, \
                             user_agent = COALESCE(EXCLUDED.user_agent, push_subscriptions.user_agent) \
                         RETURNING {PUSH_SUB_COLUMNS}"
                    ),
                    &[
                        &id,
                        &sub.subject,
                        &sub.endpoint,
                        &sub.p256dh,
                        &sub.auth,
                        &sub.user_agent,
                        &sub.now,
                    ],
                )
                .await?;
            row_to_push_sub(&row)
        })
    }

    fn list_push_subscriptions(&self, subject: &str) -> Result<Vec<PushSubscriptionRecord>> {
        let pool = self.pool.clone();
        let subject = subject.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {PUSH_SUB_COLUMNS} FROM push_subscriptions \
                         WHERE subject = $1 ORDER BY created_at DESC"
                    ),
                    &[&subject],
                )
                .await?;
            rows.iter().map(row_to_push_sub).collect()
        })
    }

    fn delete_push_subscription(&self, id: &str, subject: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        let subject = subject.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "DELETE FROM push_subscriptions WHERE id = $1 AND subject = $2",
                    &[&id, &subject],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn delete_push_subscription_by_endpoint(&self, endpoint: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let endpoint = endpoint.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            let n = client
                .execute(
                    "DELETE FROM push_subscriptions WHERE endpoint = $1",
                    &[&endpoint],
                )
                .await?;
            Ok(n > 0)
        })
    }

    fn touch_push_subscription(&self, id: &str, now: i64) -> Result<()> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        self.block(async move {
            let client = pool.get().await.context("acquiring postgres connection")?;
            client
                .execute(
                    "UPDATE push_subscriptions SET last_used_at = $1 WHERE id = $2",
                    &[&now, &id],
                )
                .await?;
            Ok(())
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
