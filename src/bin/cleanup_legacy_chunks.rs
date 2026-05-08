//! One-shot cleanup: regroup legacy `<id>:c:N` documents into a single parent.
//!
//! The pre-Postgres SQLite store chunked at the API layer (`src/api/chunking.rs`)
//! and persisted each chunk as its own `items` row keyed `<id>:c:0`, `<id>:c:1`,
//! ... When that snapshot was migrated to Postgres each suffixed row landed
//! as its *own* document, so the parent/child layout we actually want
//! degenerates to lots of fake-parent documents that are really sibling
//! chunks of the same logical doc.
//!
//! This binary:
//!   1. Finds documents whose id matches `^.+:c:[0-9]+$`.
//!   2. Groups them by bare prefix (everything before the trailing `:c:N`).
//!   3. Concatenates content in N order, keeps the metadata/source_id from
//!      the lowest-N member, re-chunks + re-embeds the merged text via the
//!      configured bge-m3 chunker, and upserts a single document at the bare
//!      id with N freshly-written chunks.
//!   4. Deletes the original `:c:N` documents (CASCADE drops their chunks).
//!
//! Idempotent: a re-run on already-cleaned data finds no `:c:N` rows and
//! exits without writes. Pass `--dry-run` to log the plan without touching
//! Postgres.
//!
//! Required env (same as `migrate_sqlite_to_pg`):
//!   RAG_DATABASE_URL, RAG_MODEL_PATH, RAG_TOKENIZER_PATH

use anyhow::{anyhow, Context, Result};
use deadpool_postgres::Pool;
use rust_rag::{
    chunking_md::MarkdownChunker,
    db::postgres,
    embedding::{Embedder, EmbeddingService, Pooling},
};
use serde_json::Value;
use std::{collections::BTreeMap, env, path::PathBuf, sync::Arc, time::Instant};
use tokenizers::Tokenizer;
use tracing::{info, warn};

#[derive(Debug)]
struct LegacyChunk {
    id: String,
    bare_id: String,
    chunk_index: i64,
    content: String,
    metadata: Value,
    source_id: String,
    author: Option<String>,
    tags: Vec<String>,
    status: Option<String>,
}

fn parse_legacy_id(id: &str) -> Option<(String, i64)> {
    // Expect suffix ":c:<digits>" at end.
    let (head, tail) = id.rsplit_once(":c:")?;
    let n: i64 = tail.parse().ok()?;
    Some((head.to_owned(), n))
}

async fn fetch_legacy(pool: &Pool) -> Result<Vec<LegacyChunk>> {
    let client = pool.get().await?;
    let rows = client
        .query(
            "SELECT id, source_id, author, content, metadata, tags, status \
             FROM documents WHERE id ~ ':c:[0-9]+$' \
             ORDER BY id",
            &[],
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: String = r.get(0);
        let Some((bare, n)) = parse_legacy_id(&id) else {
            warn!("regex match but parse failed: {id}");
            continue;
        };
        out.push(LegacyChunk {
            id,
            bare_id: bare,
            chunk_index: n,
            content: r.get(3),
            metadata: r.get(4),
            source_id: r.get(1),
            author: r.get(2),
            tags: r.get(5),
            status: r.get(6),
        });
    }
    Ok(out)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let dry_run = env::args().any(|a| a == "--dry-run");

    let database_url = env::var("RAG_DATABASE_URL").context("RAG_DATABASE_URL must be set")?;
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
        .unwrap_or(Pooling::Cls);
    let chunk_max_tokens: usize = env::var("RAG_CHUNK_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    let chunk_overlap_tokens: usize = env::var("RAG_CHUNK_OVERLAP_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);

    info!("connecting to postgres");
    let pool = postgres::connect(&database_url, 4).await?;

    let legacy = fetch_legacy(&pool).await?;
    if legacy.is_empty() {
        info!("no legacy :c:N documents found — already clean");
        return Ok(());
    }

    // Group by bare id, sorted by chunk index.
    let mut groups: BTreeMap<String, Vec<LegacyChunk>> = BTreeMap::new();
    for row in legacy {
        groups.entry(row.bare_id.clone()).or_default().push(row);
    }
    for v in groups.values_mut() {
        v.sort_by_key(|c| c.chunk_index);
    }
    info!(
        "found {} legacy chunks across {} bare ids",
        groups.values().map(Vec::len).sum::<usize>(),
        groups.len()
    );

    if dry_run {
        for (bare, members) in &groups {
            info!(
                "DRY: would merge {} → bare id {} ({} bytes total)",
                members.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", "),
                bare,
                members.iter().map(|m| m.content.len()).sum::<usize>()
            );
        }
        return Ok(());
    }

    info!("loading embedder from {}", model_path.display());
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
    if probe.len() != 1024 {
        anyhow::bail!("expected vector(1024) for bge-m3, got {}", probe.len());
    }
    let chunker_tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow!("loading chunker tokenizer: {e}"))?;
    let chunker = MarkdownChunker::new(chunker_tokenizer, chunk_max_tokens, chunk_overlap_tokens)?;
    let embedding_model = "bge-m3";
    let embedding_version: i32 = 1;

    let started = Instant::now();
    let mut total_new_chunks = 0_usize;
    for (bare, members) in &groups {
        // Refuse to clobber an existing parent that has *real* (non-legacy)
        // content under the bare id — that's user data, not legacy chunkery.
        let existing: Option<String> = {
            let client = pool.get().await?;
            client
                .query_opt(
                    "SELECT id FROM documents WHERE id = $1 AND id !~ ':c:[0-9]+$'",
                    &[bare],
                )
                .await?
                .map(|r| r.get(0))
        };
        if existing.is_some() {
            warn!(
                "skipping {bare}: a non-legacy document already exists at the bare id; \
                 manual review needed"
            );
            continue;
        }

        let head = members.first().expect("group is non-empty");
        let merged_content = members
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let chunks = chunker.chunks(&merged_content);
        if chunks.is_empty() {
            warn!("skipping {bare}: chunker produced no chunks for merged content");
            continue;
        }

        let mut client = pool.get().await?;
        let tx = client.transaction().await?;

        tx.execute(
            "INSERT INTO documents (id, source_id, kind, author, content, metadata, tags, status, created_at, updated_at) \
             VALUES ($1, $2, 'text', $3, $4, $5, $6, $7, now(), now()) \
             ON CONFLICT (id) DO UPDATE SET \
                 source_id = EXCLUDED.source_id, \
                 author    = EXCLUDED.author, \
                 content   = EXCLUDED.content, \
                 metadata  = EXCLUDED.metadata, \
                 tags      = EXCLUDED.tags, \
                 status    = EXCLUDED.status, \
                 updated_at = now()",
            &[
                bare,
                &head.source_id,
                &head.author,
                &merged_content,
                &head.metadata,
                &head.tags,
                &head.status,
            ],
        )
        .await
        .with_context(|| format!("upserting merged document {bare}"))?;

        tx.execute("DELETE FROM chunks WHERE document_id = $1", &[bare])
            .await?;

        for chunk in &chunks {
            let embedding = embedder.embed(&chunk.content)?;
            let token_count = embedder.count_tokens(&chunk.content).ok().map(|n| n as i32);
            let vector = pgvector::Vector::from(embedding);
            let section_path: Option<&[String]> = if chunk.section_path.is_empty() {
                None
            } else {
                Some(&chunk.section_path)
            };
            tx.execute(
                "INSERT INTO chunks (document_id, position, content, section_path, token_count, dense_embedding, embedding_model, embedding_version) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    bare,
                    &chunk.position,
                    &chunk.content,
                    &section_path,
                    &token_count,
                    &vector,
                    &embedding_model,
                    &embedding_version,
                ],
            )
            .await?;
        }

        // Drop the original :c:N documents (cascades to their chunks).
        for m in members {
            tx.execute("DELETE FROM documents WHERE id = $1", &[&m.id])
                .await?;
        }
        tx.commit().await?;
        total_new_chunks += chunks.len();
        info!(
            "merged {} legacy → {bare} ({} new chunks)",
            members.len(),
            chunks.len()
        );
    }

    info!(
        "cleanup done in {:?}: {} groups → {total_new_chunks} chunks",
        started.elapsed(),
        groups.len()
    );
    Ok(())
}
