//! HTTP endpoints for code-repo ingestion.
//!
//! The intended client is the `rust-rag-ingest` CLI: it walks a local repo
//! and uploads file batches here. The server only walks when the MCP
//! `code_add_repo` tool is used. Endpoints here are batch-friendly and
//! return per-file outcomes so the CLI can stream a useful progress bar.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, AppState};
use crate::code::ingest::{
    ingest_file_content, IngestOptions, IngestReport, MAX_FILE_BYTES,
};
use crate::db::code::{CodeFile, CodeRepo};
use crate::db::code_store::CodeStore;

// ---- types -----------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpsertRepoRequest {
    pub name: String,
    pub root_path: Option<String>,
    pub include_globs: Option<Vec<String>>,
    pub exclude_globs: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct RepoSummary {
    pub name: String,
    pub root_path: String,
    pub enabled: bool,
    pub file_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct PlanRequest {
    /// One entry per locally-walked file.
    pub files: Vec<PlanEntry>,
    /// Force re-ingest even when hashes match.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
pub struct PlanEntry {
    pub path: String,
    pub hash: String,
    pub size_bytes: i64,
}

#[derive(Debug, Serialize)]
pub struct PlanResponse {
    /// Subset of incoming paths whose content the CLI should upload.
    pub upload: Vec<String>,
    /// Files known on the server but not in the incoming set (would be
    /// removed by `/sweep`). For preview UI.
    pub stale: Vec<String>,
    pub total_local: usize,
    pub unchanged: usize,
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub files: Vec<FilePayload>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
pub struct FilePayload {
    pub path: String,
    pub content: String,
    pub mtime: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub repo: String,
    pub report: ReportJson,
    pub per_file: Vec<FileOutcome>,
}

#[derive(Debug, Serialize)]
pub struct ReportJson {
    pub files_scanned: usize,
    pub files_changed: usize,
    pub chunks_inserted: usize,
    pub skipped_binary: usize,
    pub skipped_too_large: usize,
    pub errors: Vec<String>,
}

impl From<&IngestReport> for ReportJson {
    fn from(r: &IngestReport) -> Self {
        Self {
            files_scanned: r.files_scanned,
            files_changed: r.files_changed,
            chunks_inserted: r.chunks_inserted,
            skipped_binary: r.skipped_binary,
            skipped_too_large: r.skipped_too_large,
            errors: r.errors.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FileOutcome {
    pub path: String,
    pub status: &'static str, // ingested | skipped | error
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SweepRequest {
    /// All paths currently present locally. Anything in the DB not in this
    /// set gets deleted.
    pub paths: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SweepResponse {
    pub deleted: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FileMeta {
    pub path: String,
    pub basename: String,
    pub language: Option<String>,
    pub role: Option<String>,
    pub summary: Option<String>,
    pub size_bytes: i64,
    pub line_count: i32,
    pub indexed_at: i64,
}

impl From<CodeFile> for FileMeta {
    fn from(f: CodeFile) -> Self {
        Self {
            path: f.path,
            basename: f.basename,
            language: f.language,
            role: f.role,
            summary: f.summary,
            size_bytes: f.size_bytes,
            line_count: f.line_count,
            indexed_at: f.indexed_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FileDetail {
    #[serde(flatten)]
    pub meta: FileMeta,
    pub outline: Vec<crate::db::code::OutlineEntry>,
    pub todos: Vec<crate::db::code::TodoEntry>,
    pub imports: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub repo: Option<String>,
    pub language: Option<String>,
    pub path_prefix: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub repo: String,
    pub path: String,
    pub language: Option<String>,
    pub symbol_kind: Option<String>,
    pub symbol_name: Option<String>,
    pub signature: Option<String>,
    pub start_line: i32,
    pub end_line: i32,
    pub snippet: String,
    pub score: f32,
}

// ---- helpers ---------------------------------------------------------------

fn require_store(state: &AppState) -> Result<Arc<CodeStore>, ApiError> {
    state
        .code_store
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("code store not configured".into()))
}

fn require_embedder(
    state: &AppState,
) -> Result<Arc<dyn crate::embedding::EmbeddingService>, ApiError> {
    let handle = state
        .code_embedder
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("code embedder not configured".into()))?;
    handle
        .try_ready()
        .map_err(|e| ApiError::ServiceUnavailable(e.to_string()))
}

async fn require_repo(store: &CodeStore, name: &str) -> Result<CodeRepo, ApiError> {
    store
        .get_repo_by_name(name)
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound(format!("repo not found: {name}")))
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---- handlers --------------------------------------------------------------

pub async fn upsert_repo(
    State(state): State<AppState>,
    Json(req): Json<UpsertRepoRequest>,
) -> Result<Json<RepoSummary>, ApiError> {
    let store = require_store(&state)?;
    let existing = store
        .get_repo_by_name(&req.name)
        .await
        .map_err(ApiError::Internal)?;
    let now = now_ms();
    let repo = CodeRepo {
        id: existing
            .as_ref()
            .map(|r| r.id.clone())
            .unwrap_or_else(|| format!("cr_{}", uuid::Uuid::now_v7().simple())),
        name: req.name.clone(),
        root_path: req
            .root_path
            .clone()
            .or_else(|| existing.as_ref().map(|r| r.root_path.clone()))
            .unwrap_or_else(|| "".to_string()),
        include_globs: req
            .include_globs
            .clone()
            .or_else(|| existing.as_ref().map(|r| r.include_globs.clone()))
            .unwrap_or_default(),
        exclude_globs: req
            .exclude_globs
            .clone()
            .or_else(|| existing.as_ref().map(|r| r.exclude_globs.clone()))
            .unwrap_or_default(),
        enabled: true,
        default_branch: existing.as_ref().and_then(|r| r.default_branch.clone()),
        created_at: existing.as_ref().map(|r| r.created_at).unwrap_or(now),
        updated_at: now,
    };
    store.upsert_repo(&repo).await.map_err(ApiError::Internal)?;
    let files = store
        .list_file_paths(&repo.id)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(RepoSummary {
        name: repo.name,
        root_path: repo.root_path,
        enabled: repo.enabled,
        file_count: files.len(),
    }))
}

pub async fn list_repos(
    State(state): State<AppState>,
) -> Result<Json<Vec<RepoSummary>>, ApiError> {
    let store = require_store(&state)?;
    let repos = store.list_repos(false).await.map_err(ApiError::Internal)?;
    let mut out = Vec::with_capacity(repos.len());
    for r in repos {
        let files = store
            .list_file_paths(&r.id)
            .await
            .map_err(ApiError::Internal)?;
        out.push(RepoSummary {
            name: r.name,
            root_path: r.root_path,
            enabled: r.enabled,
            file_count: files.len(),
        });
    }
    Ok(Json(out))
}

pub async fn delete_repo(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = require_store(&state)?;
    let repo = require_repo(&store, &name).await?;
    store
        .delete_repo(&repo.id)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(serde_json::json!({ "deleted": name })))
}

pub async fn plan_ingest(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<PlanRequest>,
) -> Result<Json<PlanResponse>, ApiError> {
    let store = require_store(&state)?;
    let repo = require_repo(&store, &name).await?;
    let total_local = req.files.len();

    let mut upload: Vec<String> = Vec::new();
    let mut unchanged: usize = 0;
    // Build a lookup of path -> incoming hash for fast lookup.
    let mut incoming: std::collections::HashMap<&str, &str> =
        std::collections::HashMap::with_capacity(total_local);
    for f in &req.files {
        incoming.insert(f.path.as_str(), f.hash.as_str());
    }

    // Pull current server-side state.
    let known = store
        .list_files(&repo.id)
        .await
        .map_err(ApiError::Internal)?;
    let mut known_by_path: std::collections::HashMap<String, String> =
        std::collections::HashMap::with_capacity(known.len());
    for f in &known {
        known_by_path.insert(f.path.clone(), f.content_hash.clone());
    }

    for f in &req.files {
        if f.size_bytes as u64 > MAX_FILE_BYTES {
            continue; // CLI shouldn't have sent this; reject silently
        }
        let prior_hash = known_by_path.get(&f.path);
        let same = prior_hash.map(|h| h == &f.hash).unwrap_or(false);
        if same && !req.force {
            unchanged += 1;
            continue;
        }
        upload.push(f.path.clone());
    }

    let stale: Vec<String> = known_by_path
        .keys()
        .filter(|p| !incoming.contains_key(p.as_str()))
        .cloned()
        .collect();

    Ok(Json(PlanResponse {
        upload,
        stale,
        total_local,
        unchanged,
    }))
}

pub async fn ingest_batch(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, ApiError> {
    let store = require_store(&state)?;
    let embedder = require_embedder(&state)?;
    let repo = require_repo(&store, &name).await?;
    let opts = IngestOptions {
        force: req.force,
        ..Default::default()
    };

    let mut report = IngestReport::default();
    let mut per_file: Vec<FileOutcome> = Vec::with_capacity(req.files.len());
    for f in &req.files {
        report.files_scanned += 1;
        if f.content.len() as u64 > MAX_FILE_BYTES {
            report.skipped_too_large += 1;
            per_file.push(FileOutcome {
                path: f.path.clone(),
                status: "skipped",
                error: Some("too large".into()),
            });
            continue;
        }
        if f.content.as_bytes().iter().take(8192).any(|&b| b == 0) {
            report.skipped_binary += 1;
            per_file.push(FileOutcome {
                path: f.path.clone(),
                status: "skipped",
                error: Some("looks binary".into()),
            });
            continue;
        }
        let size = f.content.len() as i64;
        let outcome = ingest_file_content(
            &repo,
            &f.path,
            &f.content,
            size,
            f.mtime,
            store.clone(),
            embedder.clone(),
            &opts,
            &mut report,
        )
        .await;
        match outcome {
            Ok(()) => per_file.push(FileOutcome {
                path: f.path.clone(),
                status: "ingested",
                error: None,
            }),
            Err(e) => {
                let msg = format!("{e:#}");
                report.errors.push(format!("{}: {msg}", f.path));
                per_file.push(FileOutcome {
                    path: f.path.clone(),
                    status: "error",
                    error: Some(msg),
                });
            }
        }
    }

    Ok(Json(BatchResponse {
        repo: name,
        report: ReportJson::from(&report),
        per_file,
    }))
}

pub async fn sweep(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<SweepRequest>,
) -> Result<Json<SweepResponse>, ApiError> {
    let store = require_store(&state)?;
    let repo = require_repo(&store, &name).await?;
    let live: std::collections::HashSet<String> = req.paths.into_iter().collect();
    let known = store
        .list_file_paths(&repo.id)
        .await
        .map_err(ApiError::Internal)?;
    let mut deleted: Vec<String> = Vec::new();
    for (id, path) in known {
        if !live.contains(&path) {
            store.delete_file(&id).await.map_err(ApiError::Internal)?;
            deleted.push(path);
        }
    }
    Ok(Json(SweepResponse { deleted }))
}

pub async fn list_files(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<FileMeta>>, ApiError> {
    let store = require_store(&state)?;
    let repo = require_repo(&store, &name).await?;
    let files = store
        .list_files(&repo.id)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(files.into_iter().map(FileMeta::from).collect()))
}

pub async fn get_file_detail(
    State(state): State<AppState>,
    Path((name, file_path)): Path<(String, String)>,
) -> Result<Json<FileDetail>, ApiError> {
    let store = require_store(&state)?;
    let repo = require_repo(&store, &name).await?;
    let file = store
        .get_file(&repo.id, &file_path)
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound(format!("file not found: {file_path}")))?;
    Ok(Json(FileDetail {
        outline: file.outline.clone(),
        todos: file.todos.clone(),
        imports: file.imports.clone(),
        meta: FileMeta::from(file),
    }))
}

pub async fn search_code(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    let store = require_store(&state)?;
    let embedder = require_embedder(&state)?;
    if req.query.trim().is_empty() {
        return Err(ApiError::BadRequest("empty query".into()));
    }
    let q = req.query.clone();
    let svc = embedder.clone();
    let embedding = tokio::task::spawn_blocking(move || svc.embed(&q))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    let hits = store
        .search(&crate::db::code::CodeQuery {
            embedding,
            repo: req.repo,
            language: req.language,
            path_prefix: req.path_prefix,
            limit: req.limit.unwrap_or(10).min(50),
        })
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(
        hits.into_iter()
            .map(|h| {
                let snippet = h.chunk.content.lines().take(8).collect::<Vec<_>>().join("\n");
                SearchHit {
                    repo: h.chunk.repo_name,
                    path: h.chunk.path,
                    language: h.chunk.language,
                    symbol_kind: h.chunk.symbol_kind,
                    symbol_name: h.chunk.symbol_name,
                    signature: h.chunk.signature,
                    start_line: h.chunk.start_line,
                    end_line: h.chunk.end_line,
                    snippet,
                    score: h.score,
                }
            })
            .collect(),
    ))
}
