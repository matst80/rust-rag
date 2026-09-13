//! One-shot re-embed: regenerates every Postgres chunk's dense + sparse
//! embeddings from its parent document's content using the currently
//! configured model.
//!
//! Use when the embedding model changes (e.g. bge-m3 fp32 → int8) so stored
//! vectors and query vectors live in the same space. Documents are re-chunked
//! with the same `MarkdownChunker` settings the runtime ingest path uses, so
//! chunk contents should be identical unless chunking params changed.
//!
//! This binary:
//!   1. Loads the embedder from RAG_MODEL_PATH and probes the dense
//!      dimension (must be 1024 — the pgvector columns are fixed).
//!   2. For every document: re-chunks `documents.content`, embeds each chunk
//!      (dense + sparse), then in one transaction deletes the document's
//!      chunks and inserts the new ones with `bge-m3-int8` / version 2.
//!   3. Never touches `documents` rows.
//!
//! Pass `--dry-run` to count what would be regenerated without writing.
//!
//! Required env (same as the other bins):
//!   RAG_DATABASE_URL, RAG_MODEL_PATH, RAG_TOKENIZER_PATH

use anyhow::{Context, Result, anyhow};
use rust_rag::{
    chunking_md::MarkdownChunker,
    db::postgres,
    embedding::{Embedder, EmbeddingService, Pooling},
};
use std::{env, path::PathBuf, sync::Arc, time::Instant};
use tokenizers::Tokenizer;
use tracing::{info, warn};

const EMBEDDING_MODEL: &str = "bge-m3-int8";
const EMBEDDING_VERSION: i32 = 2;
const EXPECTED_DIM: usize = 1024;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
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
        .unwrap_or(4);
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

    let documents: Vec<(String, String)> = {
        let client = pool.get().await?;
        let rows = client
            .query("SELECT id, content FROM documents ORDER BY id", &[])
            .await?;
        rows.into_iter().map(|r| (r.get(0), r.get(1))).collect()
    };
    let current_chunks: i64 = {
        let client = pool.get().await?;
        client
            .query_one("SELECT count(*) FROM chunks", &[])
            .await?
            .get(0)
    };
    info!(
        "{} documents, {current_chunks} existing chunks",
        documents.len()
    );

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
    if probe.len() != EXPECTED_DIM {
        anyhow::bail!(
            "expected vector({EXPECTED_DIM}) dense output, got {}",
            probe.len()
        );
    }
    let chunker_tokenizer =
        Tokenizer::from_file(&tokenizer_path).map_err(|e| anyhow!("loading chunker tokenizer: {e}"))?;
    let chunker = MarkdownChunker::new(chunker_tokenizer, chunk_max_tokens, chunk_overlap_tokens)?;

    // Re-chunk + embed everything up front so the write phase is short
    // transactions and the dry-run sees the real plan.
    let started = Instant::now();
    let mut planned: Vec<(String, Vec<rust_rag::db::DocChunk>)> = Vec::with_capacity(documents.len());
    let mut total_chunks = 0_usize;
    for (id, content) in &documents {
        let chunks = chunker.chunks(content);
        if chunks.is_empty() {
            warn!("skipping {id}: chunker produced no chunks");
            continue;
        }
        let mut embedded = Vec::with_capacity(chunks.len());
        for c in chunks {
            let (embedding, sparse) = embedder.embed_both(&c.content)?;
            embedded.push(rust_rag::db::DocChunk {
                position: c.position,
                content: c.content,
                embedding,
                section_path: c.section_path,
                sparse: if sparse.is_empty() { None } else { Some(sparse) },
            });
        }
        total_chunks += embedded.len();
        planned.push((id.clone(), embedded));
    }
    info!(
        "embedded {total_chunks} chunks across {} documents in {:?}",
        planned.len(),
        started.elapsed()
    );

    if dry_run {
        info!("DRY: no writes performed");
        return Ok(());
    }

    let write_started = Instant::now();
    let mut written = 0_usize;
    for (i, (id, chunks)) in planned.iter().enumerate() {
        let mut client = pool.get().await?;
        let tx = client.transaction().await?;
        tx.execute("DELETE FROM chunks WHERE document_id = $1", &[id])
            .await?;
        for chunk in chunks {
            let vector = pgvector::Vector::from(chunk.embedding.clone());
            let sparse = chunk.sparse.as_deref().and_then(|pairs| {
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
                    postgres::SPARSE_DIM,
                ))
            });
            let section_path: Option<&[String]> = if chunk.section_path.is_empty() {
                None
            } else {
                Some(&chunk.section_path)
            };
            tx.execute(
                "INSERT INTO chunks (document_id, position, content, section_path, dense_embedding, sparse_embedding, embedding_model, embedding_version) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    id,
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
        written += chunks.len();
        if (i + 1) % 100 == 0 {
            info!("wrote {}/{} documents ({written} chunks)", i + 1, planned.len());
        }
    }

    info!(
        "re-embed done in {:?}: {} documents, {written} chunks written as {EMBEDDING_MODEL} v{EMBEDDING_VERSION}",
        write_started.elapsed(),
        planned.len(),
    );
    Ok(())
}
