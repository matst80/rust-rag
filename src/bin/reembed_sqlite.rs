//! One-shot SQLite re-embed: regenerates every item's `vec_items` vector with
//! the currently configured model + pooling.
//!
//! Use when the embedding inputs change without a dimension change — e.g.
//! switching `RAG_EMBEDDING_POOLING` from `mean` to the BGE-reference `cls`
//! on a bge-small SQLite store — so stored vectors and query vectors live in
//! the same space again. External (`ext:`) placeholders are skipped: they
//! carry no vectors by design.
//!
//! Each item is re-embedded from its stored text with the same contextual
//! `[repo:…]` prefix the runtime store path applies, then written back via
//! `upsert_item` (row content untouched, FTS triggers keep `items_fts` in
//! sync). Dimension changes are NOT handled here — `vec0` columns are fixed
//! at creation, so a dim mismatch fails loudly instead of corrupting.
//!
//! Pass `--dry-run` to count what would be regenerated without writing.
//!
//! Required env: RAG_DB_PATH, RAG_MODEL_PATH, RAG_TOKENIZER_PATH.
//! Optional: RAG_EMBEDDING_POOLING (default `cls`), RAG_INTRA_THREADS (4),
//! RAG_EMBEDDING_DIMENSION (probed from the model; explicit value asserts).

use anyhow::{Context, Result};
use rust_rag::{
    api::store_search::build_contextual_embed_prefix,
    db::{GraphConfig, ListItemsRequest, SortOrder, SqliteVectorStore, VectorStore},
    embedding::{Embedder, EmbeddingService, Pooling},
};
use std::{env, path::PathBuf, time::Instant};

fn is_external(id: &str, metadata: &serde_json::Value, source_id: &str) -> bool {
    id.starts_with("ext:")
        || source_id == "external"
        || metadata.get("external").and_then(|v| v.as_bool()) == Some(true)
}

fn main() -> Result<()> {
    let dry_run = env::args().any(|a| a == "--dry-run");

    let db_path = env::var("RAG_DB_PATH").context("RAG_DB_PATH must be set")?;
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

    println!("loading embedder from {}", model_path.display());
    let embedder = Embedder::from_paths(
        &model_path,
        &tokenizer_path,
        intra_threads,
        env::var_os("RAG_ORT_DYLIB_PATH")
            .map(PathBuf::from)
            .as_deref(),
    )?
    .with_pooling(pooling);
    let probe = embedder.embed("dimension probe")?;
    println!("embedder ready: dim = {}, pooling = {pooling:?}", probe.len());
    if let Some(expected) = env::var("RAG_EMBEDDING_DIMENSION")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        if expected != probe.len() {
            anyhow::bail!("RAG_EMBEDDING_DIMENSION={expected} but model emits {}", probe.len());
        }
    }

    println!("opening sqlite store at {db_path}");
    let store = SqliteVectorStore::connect(
        &db_path,
        probe.len(),
        GraphConfig {
            enabled: false,
            ..GraphConfig::default()
        },
    )?;

    let started = Instant::now();
    let stats = reembed(&store, &|text| embedder.embed(text), dry_run)?;
    println!(
        "{} in {:?}: {} re-embedded, {} external skipped, {} failed{}",
        if dry_run { "DRY-RUN" } else { "done" },
        started.elapsed(),
        stats.done,
        stats.skipped_external,
        stats.failed,
        if dry_run { " (no writes)" } else { "" },
    );
    if stats.failed > 0 {
        anyhow::bail!("{} items failed", stats.failed);
    }
    Ok(())
}

#[derive(Debug, Default, PartialEq)]
struct ReembedStats {
    done: usize,
    skipped_external: usize,
    failed: usize,
}

/// Reproduce the runtime store path's embed input (text + contextual prefix)
/// per item and rewrite its vector. `embed` is a closure over the real
/// embedder so unit tests can stub it.
fn reembed(
    store: &SqliteVectorStore,
    embed: &dyn Fn(&str) -> Result<Vec<f32>>,
    dry_run: bool,
) -> Result<ReembedStats> {
    // Page ids first so a long run never holds a stale snapshot.
    let mut ids = Vec::new();
    let mut offset = 0_usize;
    loop {
        let (page, total) = store.list_items(ListItemsRequest {
            limit: Some(500),
            offset: Some(offset),
            sort_order: SortOrder::Asc,
            ..Default::default()
        })?;
        if page.is_empty() {
            break;
        }
        ids.extend(page.into_iter().map(|item| item.id));
        offset += 500;
        if offset as i64 >= total {
            break;
        }
    }
    println!("{} items listed", ids.len());

    let mut stats = ReembedStats::default();
    for (i, id) in ids.iter().enumerate() {
        let Some(item) = store.get_item(id)? else {
            continue;
        };
        if is_external(&item.id, &item.metadata, &item.source_id) {
            stats.skipped_external += 1;
            continue;
        }
        let embed_text = match build_contextual_embed_prefix(
            &item.metadata,
            item.data.as_ref(),
            item.path.as_deref(),
            item.type_name.as_deref(),
        ) {
            Some(prefix) => format!("{prefix}{}", item.text),
            None => item.text.clone(),
        };
        let embedding = match embed(&embed_text) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skipping {id}: embed failed: {e:#}");
                stats.failed += 1;
                continue;
            }
        };
        if !dry_run {
            if let Err(e) = store.upsert_item(item, &embedding) {
                eprintln!("skipping {id}: write failed: {e:#}");
                stats.failed += 1;
                continue;
            }
        }
        stats.done += 1;
        if (i + 1) % 100 == 0 {
            println!("  {}/{} …", i + 1, ids.len());
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_rag::db::ItemRecord;
    use serde_json::json;

    use std::sync::atomic::{AtomicU64, Ordering};

    static MEM_DB_SEQ: AtomicU64 = AtomicU64::new(0);

    fn mem_store() -> SqliteVectorStore {
        // Unique name per test: shared-cache memory DBs with the same name
        // are the same database, which leaks rows across parallel tests.
        let seq = MEM_DB_SEQ.fetch_add(1, Ordering::Relaxed);
        SqliteVectorStore::connect_uri(
            &format!("file:reembed_test_{seq}?mode=memory&cache=shared"),
            4,
            GraphConfig {
                enabled: false,
                ..GraphConfig::default()
            },
        )
        .expect("in-memory store")
    }

    fn put(store: &SqliteVectorStore, id: &str, text: &str, metadata: serde_json::Value) {
        store
            .upsert_item(
                ItemRecord {
                    id: id.to_owned(),
                    text: text.to_owned(),
                    metadata,
                    source_id: "memory".to_owned(),
                    created_at: 1,
                    updated_at: 1,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                    dreamed_at: None,
                },
                &[0.0, 0.0, 0.0, 1.0],
            )
            .expect("seed item");
    }

    #[test]
    fn rewrites_vectors_and_applies_repo_prefix() {
        let store = mem_store();
        put(&store, "a", "hello world", json!({}));
        put(&store, "b", "service notes", json!({"repo": "notes"}));

        let seen = std::cell::RefCell::new(Vec::new());
        let stats = reembed(
            &store,
            &|text| {
                seen.borrow_mut().push(text.to_owned());
                Ok(vec![0.5, 0.5, 0.5, 0.5])
            },
            false,
        )
        .expect("reembed");
        let seen = seen.into_inner();

        assert_eq!(
            stats,
            ReembedStats {
                done: 2,
                skipped_external: 0,
                failed: 0,
            }
        );
        // Repo metadata reproduces the production embed prefix; plain docs
        // embed raw.
        assert!(seen.contains(&"hello world".to_owned()));
        assert!(seen.contains(&"[repo:notes] service notes".to_owned()));
        // Vector actually replaced: distance to the new vector is zero.
        let hits = store
            .distances_for_ids(&[0.5, 0.5, 0.5, 0.5], &["a".to_owned(), "b".to_owned()])
            .expect("distances");
        assert!(hits.iter().all(|h| h.distance.abs() < 1e-6));
    }

    #[test]
    fn skips_external_placeholders_without_vectors() {
        let store = mem_store();
        put(&store, "a", "real note", json!({}));
        // Placeholder shaped exactly like add_manual_edge writes it.
        store
            .upsert_item(
                ItemRecord {
                    id: "ext:repo:notes".to_owned(),
                    text: String::new(),
                    metadata: json!({"external": true, "embed": false}),
                    source_id: "external".to_owned(),
                    created_at: 1,
                    updated_at: 1,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                    dreamed_at: None,
                },
                &[0.0, 0.0, 0.0, 1.0],
            )
            .expect("seed placeholder");

        let stats = reembed(&store, &|_| Ok(vec![1.0, 0.0, 0.0, 0.0]), true).expect("dry run");

        assert_eq!(
            stats,
            ReembedStats {
                done: 1,
                skipped_external: 1,
                failed: 0,
            }
        );
    }
}
