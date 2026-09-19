//! Code-search HTTP routes.
//!
//! The external per-repo indexer (sqlite + sqlite-vec, all-MiniLM-L6-v2
//! 384-dim float32 embeddings) is the source of truth: `POST /api/code/upload`
//! ingests a `.codesearch/codesearch.db` snapshot and replaces that repo's
//! rows wholesale. Postgres holds the merged, cross-repo copy; queries embed
//! with a dedicated MiniLM ONNX session — NOT the main bge-m3 embedder
//! (different model and dimensions).

use axum::{
    Json,
    body::Bytes,
    extract::{Multipart, Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;

use super::{ApiError, AppState};
use crate::db::{
    CodeFileDetail, CodeFileMeta, CodeRepoSummary, CodeSearchHit, parse_codesearch_db,
};

/// Snapshot size cap. Whole-repo sqlite files are a few MB; this only
/// guards against pathological uploads. Also applied as the route's body
/// limit in `router.rs` — axum's default is 2 MB, which real snapshots
/// exceed.
pub const CODE_SNAPSHOT_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CodeDeleteResponse {
    pub deleted: String,
}

fn code_store(state: &AppState) -> Result<std::sync::Arc<crate::db::CodeStore>, ApiError> {
    state.code_store.clone().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "code search requires Postgres (set RAG_DATABASE_URL)".to_owned(),
        )
    })
}

fn code_embedder(
    state: &AppState,
) -> Result<std::sync::Arc<super::state::EmbedderHandle>, ApiError> {
    state.code_embedder.clone().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "code query embedder not configured (set RAG_CODE_MODEL_PATH \
             and RAG_CODE_TOKENIZER_PATH to an all-MiniLM-L6-v2 ONNX export)"
                .to_owned(),
        )
    })
}

/// POST /api/code/upload — multipart form with a `codesearch.db` snapshot.
/// Fields: `repo` (alias `name`, required), `root_path` (optional),
/// `file` (alias `db`/`snapshot`, required).
pub async fn upload_codesearch_db(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<crate::db::CodeUploadStats>), ApiError> {
    let mut repo: Option<String> = None;
    let mut root_path = String::new();
    let mut bytes: Option<Bytes> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_owned();
        match name.as_str() {
            "repo" | "name" => {
                repo = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(e.to_string()))?,
                );
            }
            "root_path" => {
                root_path = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
            }
            "file" | "db" | "snapshot" => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                if data.len() as u64 > CODE_SNAPSHOT_MAX_BYTES {
                    return Err(ApiError::BadRequest(format!(
                        "snapshot exceeds {CODE_SNAPSHOT_MAX_BYTES} bytes"
                    )));
                }
                bytes = Some(data);
            }
            _ => {}
        }
    }

    let repo = repo
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty())
        .ok_or_else(|| ApiError::BadRequest("missing repo field".to_owned()))?;
    let bytes =
        bytes.ok_or_else(|| ApiError::BadRequest("missing file field (codesearch.db)".to_owned()))?;

    let store = code_store(&state)?;
    let parsed = spawn_blocking(move || parse_codesearch_db(&bytes))
        .await?
        .map_err(ApiError::Internal)?;
    let stats = spawn_blocking(move || store.upsert_repo(&repo, &root_path, parsed))
        .await?
        .map_err(ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(stats)))
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CodeSearchRequest {
    pub query: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// POST /api/code/search — semantic symbol search. Query embedding comes
/// from the dedicated all-MiniLM-L6-v2 session, matching how the snapshots
/// were embedded upstream.
pub async fn search_code(
    State(state): State<AppState>,
    Json(req): Json<CodeSearchRequest>,
) -> Result<Json<Vec<CodeSearchHit>>, ApiError> {
    let query = req.query.trim().to_owned();
    if query.is_empty() {
        return Err(ApiError::BadRequest("query must not be empty".to_owned()));
    }

    let store = code_store(&state)?;
    let embedder = code_embedder(&state)?.get_ready()?;
    let embedding = spawn_blocking(move || embedder.embed(&query))
        .await?
        .map_err(ApiError::Internal)?;

    let limit = req.limit.unwrap_or(20);
    let hits = spawn_blocking(move || {
        store.search(
            &embedding,
            req.repo.as_deref(),
            req.language.as_deref(),
            req.path_prefix.as_deref(),
            limit,
        )
    })
    .await?
    .map_err(ApiError::Internal)?;

    Ok(Json(hits))
}

/// GET /api/code/repos
pub async fn list_code_repos(
    State(state): State<AppState>,
) -> Result<Json<Vec<CodeRepoSummary>>, ApiError> {
    let store = code_store(&state)?;
    let repos = spawn_blocking(move || store.list_repos())
        .await?
        .map_err(ApiError::Internal)?;
    Ok(Json(repos))
}

/// DELETE /api/code/repos/{name} — removes the repo and (via FK cascade)
/// its files, symbols, and calls.
pub async fn delete_code_repo(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<CodeDeleteResponse>, ApiError> {
    let store = code_store(&state)?;
    let repo = name.clone();
    let deleted = spawn_blocking(move || store.delete_repo(&repo))
        .await?
        .map_err(ApiError::Internal)?;
    if !deleted {
        return Err(ApiError::NotFound(format!("code repo `{name}` not found")));
    }
    Ok(Json(CodeDeleteResponse { deleted: name }))
}

/// GET /api/code/repos/{name}/files
pub async fn list_code_files(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<CodeFileMeta>>, ApiError> {
    let store = code_store(&state)?;
    let files = spawn_blocking(move || store.list_files(&name))
        .await?
        .map_err(ApiError::Internal)?;
    Ok(Json(files))
}

/// GET /api/code/repos/{name}/files/{*path}
pub async fn get_code_file(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
) -> Result<Json<CodeFileDetail>, ApiError> {
    let store = code_store(&state)?;
    let (repo, file_path) = (name.clone(), path.clone());
    let detail = spawn_blocking(move || store.file_detail(&repo, &file_path))
        .await?
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound(format!("`{path}` not found in `{name}`")))?;
    Ok(Json(detail))
}
