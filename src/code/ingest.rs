//! Repo-level ingest pipeline.
//!
//! Walks a repo root with `ignore::WalkBuilder` (respects `.gitignore` +
//! per-repo include/exclude globs), analyzes each text file via the
//! `chunker`, embeds chunks in batches with the code embedder, and upserts
//! through `CodeStore`.
//!
//! Stale-cleanup: after the walk, any `code_files` rows whose path was not
//! visited get deleted (cascades to their `code_chunks`).

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::api::EmbedderHandle;
use crate::code::chunker::{analyze_file, CodeChunk, FileAnalysis};
use crate::code::lang::{detect_lang, Lang};
use crate::db::code::{CodeChunkRow, CodeFile, CodeRepo};
use crate::db::code_store::CodeStore;

/// Reasonable defaults; tuned for BGE-Code-v1's 32k ctx but keeping chunks
/// small enough for good retrieval granularity.
pub const DEFAULT_MAX_CHUNK_BYTES: usize = 4096;
pub const EMBED_BATCH_SIZE: usize = 8;
/// Skip files larger than this — generated bundles, lockfiles, binaries.
pub const MAX_FILE_BYTES: u64 = 1_500_000;

pub const EMBEDDING_MODEL_NAME: &str = "bge-code-v1";
pub const EMBEDDING_VERSION: i32 = 1;

#[derive(Debug, Default, Clone)]
pub struct IngestReport {
    pub files_scanned: usize,
    pub files_changed: usize,
    pub files_deleted: usize,
    pub chunks_inserted: usize,
    pub bytes_embedded: usize,
    pub skipped_binary: usize,
    pub skipped_too_large: usize,
    pub errors: Vec<String>,
}

pub struct IngestOptions {
    /// When `true`, re-embeds even if `content_hash` matches the DB.
    pub force: bool,
    pub max_chunk_bytes: usize,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            force: false,
            max_chunk_bytes: DEFAULT_MAX_CHUNK_BYTES,
        }
    }
}

/// Walk + analyze + embed + upsert. Returns aggregate stats.
#[tracing::instrument(skip_all, fields(repo = %repo.name, root = %repo.root_path))]
pub async fn ingest_repo(
    repo: &CodeRepo,
    store: Arc<CodeStore>,
    embedder: Arc<EmbedderHandle>,
    opts: IngestOptions,
) -> Result<IngestReport> {
    let svc = embedder
        .try_ready()
        .context("code embedder not ready")?;
    let root = PathBuf::from(&repo.root_path);
    if !root.exists() {
        anyhow::bail!("repo root does not exist: {}", root.display());
    }

    let includes = build_globset(&repo.include_globs)?;
    let excludes = build_globset(&repo.exclude_globs)?;

    let mut walker = WalkBuilder::new(&root);
    walker
        .follow_links(false)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .require_git(false);

    let mut visited: HashSet<String> = HashSet::new();
    let mut report = IngestReport::default();

    for entry in walker.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                report.errors.push(format!("walk error: {e}"));
                continue;
            }
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let abs_path = entry.path();
        let rel_path = match abs_path.strip_prefix(&root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        if !includes.is_empty() && !includes.is_match(&rel_path) {
            continue;
        }
        if !excludes.is_empty() && excludes.is_match(&rel_path) {
            continue;
        }
        if matches_default_excludes(&rel_path) {
            continue;
        }

        report.files_scanned += 1;
        visited.insert(rel_path.clone());

        match ingest_file(
            repo,
            &rel_path,
            abs_path,
            store.clone(),
            svc.clone(),
            &opts,
            &mut report,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => report
                .errors
                .push(format!("{rel_path}: {e:#}")),
        }
    }

    // Stale cleanup: anything in DB but not visited this scan.
    let known = store.list_file_paths(&repo.id).await?;
    for (file_id, path) in known {
        if !visited.contains(&path) {
            store.delete_file(&file_id).await?;
            report.files_deleted += 1;
        }
    }

    tracing::info!(
        scanned = report.files_scanned,
        changed = report.files_changed,
        deleted = report.files_deleted,
        chunks = report.chunks_inserted,
        skipped_binary = report.skipped_binary,
        skipped_too_large = report.skipped_too_large,
        errors = report.errors.len(),
        "code ingest complete"
    );
    Ok(report)
}

#[tracing::instrument(skip_all, fields(path = %rel_path))]
pub async fn ingest_file(
    repo: &CodeRepo,
    rel_path: &str,
    abs_path: &Path,
    store: Arc<CodeStore>,
    embedder: Arc<dyn crate::embedding::EmbeddingService>,
    opts: &IngestOptions,
    report: &mut IngestReport,
) -> Result<()> {
    let metadata = std::fs::metadata(abs_path)?;
    if metadata.len() > MAX_FILE_BYTES {
        report.skipped_too_large += 1;
        return Ok(());
    }
    let bytes = std::fs::read(abs_path)?;
    if looks_binary(&bytes) {
        report.skipped_binary += 1;
        return Ok(());
    }
    let content = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            report.skipped_binary += 1;
            return Ok(());
        }
    };
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);
    let size_bytes = metadata.len() as i64;
    ingest_file_content(
        repo,
        rel_path,
        &content,
        size_bytes,
        mtime,
        store,
        embedder,
        opts,
        report,
    )
    .await
}

/// FS-free variant — used by the HTTP/CLI path where the client uploads
/// file bytes directly. Same hash-diff dedup logic; caller is responsible
/// for any "skip too large / binary" decisions (CLI does that locally).
#[tracing::instrument(skip_all, fields(path = %rel_path))]
pub async fn ingest_file_content(
    repo: &CodeRepo,
    rel_path: &str,
    content: &str,
    size_bytes: i64,
    mtime: Option<i64>,
    store: Arc<CodeStore>,
    embedder: Arc<dyn crate::embedding::EmbeddingService>,
    opts: &IngestOptions,
    report: &mut IngestReport,
) -> Result<()> {
    let file_content_hash = sha256_hex(content.as_bytes());
    let file_id = file_id_for(&repo.id, rel_path);

    let prior = store.get_file(&repo.id, rel_path).await?;
    let unchanged =
        prior.as_ref().map(|f| f.content_hash == file_content_hash).unwrap_or(false);
    if unchanged && !opts.force {
        return Ok(());
    }

    let lang = detect_lang(Path::new(rel_path));
    let analysis: FileAnalysis = analyze_file(rel_path, lang, content, opts.max_chunk_bytes);

    let now = now_ms();
    let dir = Path::new(rel_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let basename = Path::new(rel_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| rel_path.to_string());
    let extension = Path::new(rel_path)
        .extension()
        .map(|e| e.to_string_lossy().to_string());

    let file_row = CodeFile {
        id: file_id.clone(),
        repo_id: repo.id.clone(),
        repo_name: repo.name.clone(),
        path: rel_path.to_string(),
        basename: basename.clone(),
        dir,
        extension,
        language: Some(lang.name().to_string()),
        size_bytes,
        line_count: analysis.line_count as i32,
        git_sha: None,
        git_branch: None,
        content_hash: file_content_hash.clone(),
        mtime,
        indexed_at: now,
        summary: analysis.summary.clone(),
        role: analysis.role.clone(),
        imports: analysis.imports.clone(),
        outline: analysis.outline.clone(),
        todos: analysis.todos.clone(),
        created_at: prior.as_ref().map(|f| f.created_at).unwrap_or(now),
        updated_at: now,
    };
    store.upsert_file(&file_row).await?;
    report.files_changed += 1;

    // Wipe + re-insert chunks for this file.
    store.delete_chunks_for_file(&file_id).await?;
    if analysis.chunks.is_empty() {
        return Ok(());
    }

    // Embed in batches. Each chunk gets embedded independently to keep memory
    // bounded; batching here is logical (back-to-back spawn_blocking calls)
    // rather than a single multi-input forward pass — bge-code is heavy enough
    // that per-call latency dominates.
    let mut chunk_rows: Vec<CodeChunkRow> = Vec::with_capacity(analysis.chunks.len());
    let total_chunks = analysis.chunks.len();
    for (i, chunk) in analysis.chunks.iter().enumerate() {
        let prev_id = if i > 0 {
            Some(chunk_id_for(&file_id, (i - 1) as i32))
        } else {
            None
        };
        let next_id = if i + 1 < total_chunks {
            Some(chunk_id_for(&file_id, (i + 1) as i32))
        } else {
            None
        };
        let id = chunk_id_for(&file_id, i as i32);

        let embed_text = build_embed_text(rel_path, lang, chunk);
        let svc = embedder.clone();
        let embed_in = embed_text.clone();
        let embedding = tokio::task::spawn_blocking(move || svc.embed(&embed_in))
            .await
            .context("embed task join")?
            .with_context(|| format!("embedding chunk {} of {}", i, rel_path))?;
        report.bytes_embedded += embed_text.len();

        let content_hash = sha256_hex(chunk.content.as_bytes());
        chunk_rows.push(CodeChunkRow {
            id,
            file_id: file_id.clone(),
            repo_id: repo.id.clone(),
            repo_name: repo.name.clone(),
            path: rel_path.to_string(),
            basename: basename.clone(),
            language: Some(lang.name().to_string()),
            ordinal: i as i32,
            start_line: chunk.start_line as i32,
            end_line: chunk.end_line as i32,
            byte_start: chunk.byte_start as i64,
            byte_end: chunk.byte_end as i64,
            symbol_kind: Some(chunk.kind.clone()),
            symbol_name: chunk.name.clone(),
            symbol_path: chunk.symbol_path.clone(),
            parent_symbol: chunk.parent_symbol.clone(),
            visibility: chunk.visibility.clone(),
            doc_comment: chunk.doc_comment.clone(),
            signature: chunk.signature.clone(),
            is_test: chunk.is_test,
            is_public: chunk.is_public,
            calls: chunk.calls.clone(),
            content: chunk.content.clone(),
            content_hash,
            token_count: None,
            file_content_hash: file_content_hash.clone(),
            git_sha: None,
            prev_chunk_id: prev_id,
            next_chunk_id: next_id,
            embedding: Some(embedding),
            embedding_model: EMBEDDING_MODEL_NAME.to_string(),
            embedding_version: EMBEDDING_VERSION,
            created_at: now,
            updated_at: now,
        });

        if chunk_rows.len() >= EMBED_BATCH_SIZE {
            let batch_len = chunk_rows.len();
            store.insert_chunks(&chunk_rows).await?;
            report.chunks_inserted += batch_len;
            chunk_rows.clear();
        }
    }
    if !chunk_rows.is_empty() {
        let n = chunk_rows.len();
        store.insert_chunks(&chunk_rows).await?;
        report.chunks_inserted += n;
    }
    Ok(())
}

/// Build the text actually fed to BGE-Code-v1. Includes path + symbol header
/// so the embedding sees the context, then the chunk body. Matches the
/// "code search" framing of the model card.
fn build_embed_text(rel_path: &str, lang: Lang, chunk: &CodeChunk) -> String {
    let mut header = format!("// file: {rel_path} ({lang})\n", lang = lang.name());
    if let Some(sym) = chunk.name.as_deref() {
        header.push_str(&format!("// symbol: {sym}\n"));
    }
    if let Some(doc) = chunk.doc_comment.as_deref() {
        header.push_str(&format!("// doc: {doc}\n"));
    }
    header.push_str(&chunk.content);
    header
}

fn build_globset(globs: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g).with_context(|| format!("invalid glob: {g}"))?);
    }
    b.build().context("building globset")
}

/// Hard-coded excludes that almost every repo wants. Cheap to filter here
/// rather than force users to spell out repeatedly.
fn matches_default_excludes(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    p.starts_with("target/")
        || p.starts_with("node_modules/")
        || p.starts_with(".git/")
        || p.starts_with(".next/")
        || p.starts_with(".turbo/")
        || p.starts_with("dist/")
        || p.starts_with("build/")
        || p.starts_with(".venv/")
        || p.starts_with("venv/")
        || p.starts_with(".cache/")
        || p.ends_with(".lock")
        || p.ends_with("Cargo.lock")
        || p.ends_with("package-lock.json")
        || p.ends_with("pnpm-lock.yaml")
        || p.ends_with("yarn.lock")
        || p.ends_with("poetry.lock")
        || p.ends_with("go.sum")
        || p.ends_with(".min.js")
        || p.ends_with(".min.css")
        || p.ends_with(".map")
        || p.ends_with(".snap")
}

/// Quick heuristic: a NUL byte in the first 8 KiB → binary.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn file_id_for(repo_id: &str, rel_path: &str) -> String {
    let mut h = Sha256::new();
    h.update(repo_id.as_bytes());
    h.update(b":");
    h.update(rel_path.as_bytes());
    format!("cf_{}", hex_short(&h.finalize()))
}

fn chunk_id_for(file_id: &str, ordinal: i32) -> String {
    format!("{file_id}:{ordinal}")
}

fn hex_short(d: &impl AsRef<[u8]>) -> String {
    let bytes = d.as_ref();
    let mut s = String::with_capacity(32);
    for b in bytes.iter().take(16) {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
