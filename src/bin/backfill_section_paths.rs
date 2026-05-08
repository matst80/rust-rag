//! Backfill `chunks.section_path` for documents written before the
//! header-walk landed.
//!
//! Reads `documents.content`, re-runs the markdown chunker (no embeddings),
//! and updates each existing chunk's `section_path` by matching on
//! `(document_id, position)`. If the chunk count diverges from what's
//! stored, the document is skipped — better to log a mismatch than to
//! quietly fix the wrong row. Idempotent: re-running on already-populated
//! rows is a no-op overwrite.
//!
//! Required env: RAG_DATABASE_URL, RAG_TOKENIZER_PATH.

use anyhow::{anyhow, Context, Result};
use deadpool_postgres::Pool;
use rust_rag::{chunking_md::MarkdownChunker, db::postgres};
use std::{env, path::PathBuf};
use tokenizers::Tokenizer;
use tracing::{info, warn};

async fn fetch_documents(pool: &Pool) -> Result<Vec<(String, String)>> {
    let client = pool.get().await?;
    let rows = client
        .query("SELECT id, content FROM documents ORDER BY id", &[])
        .await?;
    Ok(rows.into_iter().map(|r| (r.get(0), r.get(1))).collect())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url = env::var("RAG_DATABASE_URL").context("RAG_DATABASE_URL must be set")?;
    let tokenizer_path: PathBuf = env::var_os("RAG_TOKENIZER_PATH")
        .map(PathBuf::from)
        .context("RAG_TOKENIZER_PATH must be set")?;
    let chunk_max_tokens: usize = env::var("RAG_CHUNK_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    let chunk_overlap_tokens: usize = env::var("RAG_CHUNK_OVERLAP_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);

    let chunker_tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow!("loading chunker tokenizer: {e}"))?;
    let chunker = MarkdownChunker::new(chunker_tokenizer, chunk_max_tokens, chunk_overlap_tokens)?;

    info!("connecting to postgres");
    let pool = postgres::connect(&database_url, 4).await?;

    let docs = fetch_documents(&pool).await?;
    info!("loaded {} documents", docs.len());

    let mut updated = 0_usize;
    let mut skipped = 0_usize;
    for (id, content) in &docs {
        let chunks = chunker.chunks(content);

        let client = pool.get().await?;
        let stored: i64 = client
            .query_one(
                "SELECT count(*) FROM chunks WHERE document_id = $1",
                &[id],
            )
            .await?
            .get(0);
        if stored as usize != chunks.len() {
            warn!(
                "skip {id}: stored chunks={stored} but re-chunk produced {} — likely \
                 different chunker config or stale doc",
                chunks.len()
            );
            skipped += 1;
            continue;
        }

        for c in &chunks {
            let section_path: Option<&[String]> = if c.section_path.is_empty() {
                None
            } else {
                Some(&c.section_path)
            };
            client
                .execute(
                    "UPDATE chunks SET section_path = $1 \
                     WHERE document_id = $2 AND position = $3",
                    &[&section_path, id, &c.position],
                )
                .await?;
        }
        updated += 1;
    }

    info!("done: updated {updated} docs, skipped {skipped}");
    Ok(())
}
