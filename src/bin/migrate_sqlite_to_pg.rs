//! One-shot migration: SQLite snapshot → Postgres `documents` + `chunks`.
//!
//! Reads `items` rows, re-embeds each with the configured ONNX model
//! (bge-m3 dense head produces vector(1024)), and writes one document
//! plus one chunk per row. Vectors are NOT ported from SQLite — they
//! belong to bge-small (384d) and are incompatible with the new schema.
//!
//! Required env:
//!   RAG_DATABASE_URL        postgres://user:pass@host/db
//!   RAG_MODEL_PATH          path to bge-m3 ONNX model
//!   RAG_TOKENIZER_PATH      path to bge-m3 tokenizer.json
//!
//! Positional: path to SQLite snapshot file (e.g. /tmp/rust-rag-prod-snapshot/rag.db).
//!
//! Re-runnable: ON CONFLICT updates documents in place; chunks for the
//! same (document_id, position) are replaced.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use rust_rag::{
    chunking_md::MarkdownChunker,
    db::postgres,
    embedding::{Embedder, EmbeddingService, Pooling},
};
use tokenizers::Tokenizer;
use serde_json::Value;
use std::{env, path::PathBuf, sync::Arc, time::Instant};
use tracing::{info, warn};

struct ItemRow {
    id: String,
    text: String,
    metadata: Value,
    source_id: String,
    created_at_ms: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let sqlite_path = env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("usage: migrate_sqlite_to_pg <path-to-rag.db>"))?;
    let database_url = env::var("RAG_DATABASE_URL")
        .context("RAG_DATABASE_URL must be set")?;
    let model_path: PathBuf = env::var_os("RAG_MODEL_PATH")
        .map(PathBuf::from)
        .context("RAG_MODEL_PATH must be set")?;
    let tokenizer_path: PathBuf = env::var_os("RAG_TOKENIZER_PATH")
        .map(PathBuf::from)
        .context("RAG_TOKENIZER_PATH must be set")?;
    let intra_threads: usize = env::var("RAG_INTRA_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

    let pooling: Pooling = env::var("RAG_EMBEDDING_POOLING")
        .ok()
        .as_deref()
        .map(str::parse)
        .transpose()?
        // bge-m3's reference dense head is CLS-pooled. Migrations are usually
        // bge-m3 → fresh schema, so default to CLS here even though main.rs
        // still defaults to mean for backward-compat with the bge-small store.
        .unwrap_or(Pooling::Cls);

    info!("loading embedder from {} (pooling={pooling:?})", model_path.display());
    let embedder: Arc<dyn EmbeddingService> = Arc::new(
        Embedder::from_paths(
            &model_path,
            &tokenizer_path,
            intra_threads,
            env::var_os("RAG_ORT_DYLIB_PATH")
                .map(PathBuf::from)
                .as_deref(),
        )?
        .with_pooling(pooling),
    );

    let probe = embedder.embed("dimension probe")?;
    info!("embedder ready, output dim = {}", probe.len());
    if probe.len() != 1024 {
        warn!(
            "expected vector(1024) for bge-m3, got {} — schema mismatch likely",
            probe.len()
        );
    }
    let embedding_model = "bge-m3".to_owned();
    let embedding_version: i32 = 1;

    // Chunker uses the same tokenizer the embedder sees, so chunk size is
    // measured in real bge-m3 tokens. 500/50 per the migration plan.
    let chunk_max_tokens: usize = env::var("RAG_CHUNK_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    let chunk_overlap_tokens: usize = env::var("RAG_CHUNK_OVERLAP_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let chunker_tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow!("loading tokenizer for chunker: {e}"))?;
    let chunker = MarkdownChunker::new(chunker_tokenizer, chunk_max_tokens, chunk_overlap_tokens)?;
    info!(
        "chunker ready (max={chunk_max_tokens} tokens, overlap={chunk_overlap_tokens} tokens)"
    );

    info!("connecting to postgres");
    let pool = postgres::connect(&database_url, 4).await?;

    let items = read_items(&sqlite_path)?;
    info!("read {} items from {sqlite_path}", items.len());

    let started = Instant::now();
    let mut total_chunks = 0_usize;
    for (i, row) in items.iter().enumerate() {
        let chunks = chunker.chunks(&row.text);
        if chunks.is_empty() {
            warn!("skipping {} — chunker produced no chunks", row.id);
            continue;
        }

        let created_at: DateTime<Utc> =
            DateTime::<Utc>::from_timestamp_millis(row.created_at_ms)
                .unwrap_or_else(Utc::now);

        let author = row
            .metadata
            .get("author")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let tags: Vec<String> = row
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
        let status = row
            .metadata
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned);

        let mut client = pool.get().await?;
        let tx = client.transaction().await?;

        tx.execute(
            "INSERT INTO documents (id, source_id, kind, author, content, metadata, tags, status, created_at, updated_at) \
             VALUES ($1, $2, 'text', $3, $4, $5, $6, $7, $8, $8) \
             ON CONFLICT (id) DO UPDATE SET \
                 source_id = EXCLUDED.source_id, \
                 author = EXCLUDED.author, \
                 content = EXCLUDED.content, \
                 metadata = EXCLUDED.metadata, \
                 tags = EXCLUDED.tags, \
                 status = EXCLUDED.status, \
                 updated_at = now()",
            &[
                &row.id,
                &row.source_id,
                &author,
                &row.text,
                &row.metadata,
                &tags,
                &status,
                &created_at,
            ],
        )
        .await
        .with_context(|| format!("inserting document {}", row.id))?;

        tx.execute(
            "DELETE FROM chunks WHERE document_id = $1",
            &[&row.id],
        )
        .await?;

        for chunk in &chunks {
            let embedding = embedder.embed(&chunk.content)?;
            if embedding.len() != probe.len() {
                anyhow::bail!(
                    "embedding dim drifted on {} chunk {}: {} vs {}",
                    row.id,
                    chunk.position,
                    embedding.len(),
                    probe.len()
                );
            }
            let token_count = embedder.count_tokens(&chunk.content).ok().map(|n| n as i32);
            let vector = pgvector::Vector::from(embedding);
            tx.execute(
                "INSERT INTO chunks (document_id, position, content, token_count, dense_embedding, embedding_model, embedding_version) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &row.id,
                    &chunk.position,
                    &chunk.content,
                    &token_count,
                    &vector,
                    &embedding_model,
                    &embedding_version,
                ],
            )
            .await
            .with_context(|| format!("inserting chunk {} for {}", chunk.position, row.id))?;
        }

        tx.commit().await?;
        total_chunks += chunks.len();

        if (i + 1) % 10 == 0 || i + 1 == items.len() {
            info!(
                "migrated {}/{} docs, {} chunks total ({:.1}s elapsed)",
                i + 1,
                items.len(),
                total_chunks,
                started.elapsed().as_secs_f64()
            );
        }
    }

    info!(
        "done — {} documents, {total_chunks} chunks in {:.1}s",
        items.len(),
        started.elapsed().as_secs_f64()
    );

    // Auxiliary tables: schema-mirrored 1:1 from SQLite, no re-embedding.
    // ON CONFLICT DO NOTHING keeps the migration re-runnable; rerunning
    // won't undo edits made on the Postgres side after the first import.
    let messages = read_messages(&sqlite_path)?;
    let inserted = copy_messages(&pool, &messages).await?;
    info!(
        "messages: {} read, {inserted} inserted (skipped {} existing)",
        messages.len(),
        messages.len() - inserted
    );

    let tokens = read_mcp_tokens(&sqlite_path)?;
    let token_inserted = copy_mcp_tokens(&pool, &tokens).await?;
    info!(
        "mcp_tokens: {} read, {token_inserted} inserted (skipped {} existing)",
        tokens.len(),
        tokens.len() - token_inserted
    );

    let device_auths = read_device_auths(&sqlite_path)?;
    let device_inserted = copy_device_auths(&pool, &device_auths).await?;
    info!(
        "device_auth_requests: {} read, {device_inserted} inserted (skipped {} existing)",
        device_auths.len(),
        device_auths.len() - device_inserted
    );

    Ok(())
}

struct MessageRow {
    id: String,
    channel: String,
    sender: String,
    sender_kind: String,
    text: String,
    kind: String,
    metadata: Value,
    created_at: i64,
    updated_at: i64,
}

fn read_messages(path: &str) -> Result<Vec<MessageRow>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening sqlite snapshot at {path}"))?;
    let mut stmt = conn.prepare(
        "SELECT id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at \
         FROM messages ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let metadata_str: String = row.get(6)?;
            Ok(MessageRow {
                id: row.get(0)?,
                channel: row.get(1)?,
                sender: row.get(2)?,
                sender_kind: row.get(3)?,
                text: row.get(4)?,
                kind: row.get(5)?,
                metadata: serde_json::from_str(&metadata_str)
                    .unwrap_or(Value::Object(Default::default())),
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

struct McpTokenRow {
    id: String,
    token_hash: String,
    name: String,
    subject: Option<String>,
    created_at: i64,
    last_used_at: Option<i64>,
    expires_at: Option<i64>,
}

fn read_mcp_tokens(path: &str) -> Result<Vec<McpTokenRow>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening sqlite snapshot at {path}"))?;
    let mut stmt = conn.prepare(
        "SELECT id, token_hash, name, subject, created_at, last_used_at, expires_at \
         FROM mcp_tokens ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(McpTokenRow {
                id: row.get(0)?,
                token_hash: row.get(1)?,
                name: row.get(2)?,
                subject: row.get(3)?,
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
                expires_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

async fn copy_mcp_tokens(
    pool: &deadpool_postgres::Pool,
    tokens: &[McpTokenRow],
) -> Result<usize> {
    let mut inserted = 0_usize;
    let client = pool.get().await?;
    for t in tokens {
        let n = client
            .execute(
                "INSERT INTO mcp_tokens (id, token_hash, name, subject, created_at, last_used_at, expires_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) \
                 ON CONFLICT (id) DO NOTHING",
                &[
                    &t.id,
                    &t.token_hash,
                    &t.name,
                    &t.subject,
                    &t.created_at,
                    &t.last_used_at,
                    &t.expires_at,
                ],
            )
            .await
            .with_context(|| format!("inserting mcp_token {}", t.id))?;
        inserted += n as usize;
    }
    Ok(inserted)
}

struct DeviceAuthRow {
    device_code: String,
    user_code: String,
    status: String,
    token_id: Option<String>,
    subject: Option<String>,
    client_name: Option<String>,
    created_at: i64,
    expires_at: i64,
    interval_secs: i64,
    last_polled_at: Option<i64>,
}

fn read_device_auths(path: &str) -> Result<Vec<DeviceAuthRow>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening sqlite snapshot at {path}"))?;
    let mut stmt = conn.prepare(
        "SELECT device_code, user_code, status, token_id, subject, client_name, \
                created_at, expires_at, interval_secs, last_polled_at \
         FROM device_auth_requests ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(DeviceAuthRow {
                device_code: row.get(0)?,
                user_code: row.get(1)?,
                status: row.get(2)?,
                token_id: row.get(3)?,
                subject: row.get(4)?,
                client_name: row.get(5)?,
                created_at: row.get(6)?,
                expires_at: row.get(7)?,
                interval_secs: row.get(8)?,
                last_polled_at: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

async fn copy_device_auths(
    pool: &deadpool_postgres::Pool,
    requests: &[DeviceAuthRow],
) -> Result<usize> {
    let mut inserted = 0_usize;
    let client = pool.get().await?;
    for r in requests {
        let n = client
            .execute(
                "INSERT INTO device_auth_requests \
                     (device_code, user_code, status, token_id, subject, client_name, \
                      created_at, expires_at, interval_secs, last_polled_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
                 ON CONFLICT (device_code) DO NOTHING",
                &[
                    &r.device_code,
                    &r.user_code,
                    &r.status,
                    &r.token_id,
                    &r.subject,
                    &r.client_name,
                    &r.created_at,
                    &r.expires_at,
                    &r.interval_secs,
                    &r.last_polled_at,
                ],
            )
            .await
            .with_context(|| format!("inserting device_auth {}", r.device_code))?;
        inserted += n as usize;
    }
    Ok(inserted)
}

async fn copy_messages(
    pool: &deadpool_postgres::Pool,
    messages: &[MessageRow],
) -> Result<usize> {
    let mut inserted = 0_usize;
    let client = pool.get().await?;
    for m in messages {
        let n = client
            .execute(
                "INSERT INTO messages (id, channel, sender, sender_kind, text, kind, metadata, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
                 ON CONFLICT (id) DO NOTHING",
                &[
                    &m.id,
                    &m.channel,
                    &m.sender,
                    &m.sender_kind,
                    &m.text,
                    &m.kind,
                    &m.metadata,
                    &m.created_at,
                    &m.updated_at,
                ],
            )
            .await
            .with_context(|| format!("inserting message {}", m.id))?;
        inserted += n as usize;
    }
    Ok(inserted)
}

fn read_items(path: &str) -> Result<Vec<ItemRow>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening sqlite snapshot at {path}"))?;
    let mut stmt = conn.prepare(
        "SELECT id, text, metadata, source_id, created_at FROM items ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let metadata_str: String = row.get(2)?;
            Ok(ItemRow {
                id: row.get(0)?,
                text: row.get(1)?,
                metadata: serde_json::from_str(&metadata_str)
                    .unwrap_or(Value::Object(Default::default())),
                source_id: row.get(3)?,
                created_at_ms: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
