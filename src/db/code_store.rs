//! Async data-access layer for the code-ingestion tables
//! (`code_repos`, `code_files`, `code_chunks`).
//!
//! Shares the existing `PgPool` (handed in from `PostgresVectorStore::pool`).
//! Async-first because every caller — the file-watcher worker, the `code_*`
//! MCP tools, the `/api/code/*` HTTP layer — already lives in a tokio
//! context.
//!
//! Sqlite fallback is **not** implemented here. The CUDA pod runs Postgres in
//! prod; pure-sqlite dev mode keeps `vec_code_chunks` table reserved in the
//! schema but the code-ingest worker stays disabled when `RAG_DATABASE_URL`
//! is unset.

use anyhow::{Context, Result};
use serde_json::Value as Json;
use tokio_postgres::types::ToSql;

use super::postgres::PgPool;
use crate::db::code::{
    CodeChunkHit, CodeChunkRow, CodeFile, CodeQuery, CodeRepo, OutlineEntry, TodoEntry,
};

pub struct CodeStore {
    pool: PgPool,
}

impl CodeStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ----- repos -----------------------------------------------------------

    pub async fn upsert_repo(&self, r: &CodeRepo) -> Result<()> {
        let client = self.pool.get().await?;
        let include_globs = serde_json::to_value(&r.include_globs)?;
        let exclude_globs = serde_json::to_value(&r.exclude_globs)?;
        client
            .execute(
                "INSERT INTO code_repos \
                    (id, name, root_path, include_globs, exclude_globs, enabled, default_branch, created_at, updated_at) \
                 VALUES ($1,$2,$3,$4::jsonb,$5::jsonb,$6,$7,$8,$9) \
                 ON CONFLICT (name) DO UPDATE SET \
                    root_path = EXCLUDED.root_path, \
                    include_globs = EXCLUDED.include_globs, \
                    exclude_globs = EXCLUDED.exclude_globs, \
                    enabled = EXCLUDED.enabled, \
                    default_branch = EXCLUDED.default_branch, \
                    updated_at = EXCLUDED.updated_at",
                &[
                    &r.id,
                    &r.name,
                    &r.root_path,
                    &include_globs,
                    &exclude_globs,
                    &r.enabled,
                    &r.default_branch,
                    &r.created_at,
                    &r.updated_at,
                ],
            )
            .await
            .context("upsert code_repo")?;
        Ok(())
    }

    pub async fn list_repos(&self, enabled_only: bool) -> Result<Vec<CodeRepo>> {
        let client = self.pool.get().await?;
        let sql = if enabled_only {
            "SELECT * FROM code_repos WHERE enabled = TRUE ORDER BY name"
        } else {
            "SELECT * FROM code_repos ORDER BY name"
        };
        let rows = client.query(sql, &[]).await?;
        rows.iter().map(row_to_repo).collect()
    }

    pub async fn get_repo_by_name(&self, name: &str) -> Result<Option<CodeRepo>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt("SELECT * FROM code_repos WHERE name = $1", &[&name])
            .await?;
        row.as_ref().map(row_to_repo).transpose()
    }

    pub async fn delete_repo(&self, id: &str) -> Result<()> {
        let client = self.pool.get().await?;
        client
            .execute("DELETE FROM code_repos WHERE id = $1", &[&id])
            .await?;
        Ok(())
    }

    // ----- files -----------------------------------------------------------

    /// Returns `true` when the stored content_hash differs from the incoming
    /// hash (i.e. caller must re-chunk + re-embed). On insert returns `true`.
    pub async fn upsert_file(&self, f: &CodeFile) -> Result<bool> {
        let client = self.pool.get().await?;
        let prior: Option<String> = client
            .query_opt(
                "SELECT content_hash FROM code_files WHERE id = $1",
                &[&f.id],
            )
            .await?
            .and_then(|r| r.get::<_, Option<String>>(0));
        let changed = prior.as_deref() != Some(f.content_hash.as_str());
        let imports = serde_json::to_value(&f.imports)?;
        let outline = serde_json::to_value(&f.outline)?;
        let todos = serde_json::to_value(&f.todos)?;
        client
            .execute(
                "INSERT INTO code_files \
                    (id, repo_id, repo_name, path, basename, dir, extension, language, \
                     size_bytes, line_count, git_sha, git_branch, content_hash, mtime, \
                     indexed_at, summary, role, imports, outline, todos, \
                     created_at, updated_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17, \
                         $18::jsonb,$19::jsonb,$20::jsonb,$21,$22) \
                 ON CONFLICT (id) DO UPDATE SET \
                    repo_name=EXCLUDED.repo_name, path=EXCLUDED.path, basename=EXCLUDED.basename, \
                    dir=EXCLUDED.dir, extension=EXCLUDED.extension, language=EXCLUDED.language, \
                    size_bytes=EXCLUDED.size_bytes, line_count=EXCLUDED.line_count, \
                    git_sha=EXCLUDED.git_sha, git_branch=EXCLUDED.git_branch, \
                    content_hash=EXCLUDED.content_hash, mtime=EXCLUDED.mtime, \
                    indexed_at=EXCLUDED.indexed_at, summary=EXCLUDED.summary, role=EXCLUDED.role, \
                    imports=EXCLUDED.imports, outline=EXCLUDED.outline, todos=EXCLUDED.todos, \
                    updated_at=EXCLUDED.updated_at",
                &[
                    &f.id,
                    &f.repo_id,
                    &f.repo_name,
                    &f.path,
                    &f.basename,
                    &f.dir,
                    &f.extension,
                    &f.language,
                    &f.size_bytes,
                    &f.line_count,
                    &f.git_sha,
                    &f.git_branch,
                    &f.content_hash,
                    &f.mtime,
                    &f.indexed_at,
                    &f.summary,
                    &f.role,
                    &imports,
                    &outline,
                    &todos,
                    &f.created_at,
                    &f.updated_at,
                ],
            )
            .await
            .context("upsert code_file")?;
        Ok(changed)
    }

    pub async fn get_file(&self, repo_id: &str, path: &str) -> Result<Option<CodeFile>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT * FROM code_files WHERE repo_id = $1 AND path = $2",
                &[&repo_id, &path],
            )
            .await?;
        row.as_ref().map(row_to_file).transpose()
    }

    pub async fn list_files(&self, repo_id: &str) -> Result<Vec<CodeFile>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT * FROM code_files WHERE repo_id = $1 ORDER BY path",
                &[&repo_id],
            )
            .await?;
        rows.iter().map(row_to_file).collect()
    }

    pub async fn delete_file(&self, file_id: &str) -> Result<()> {
        let client = self.pool.get().await?;
        client
            .execute("DELETE FROM code_files WHERE id = $1", &[&file_id])
            .await?;
        Ok(())
    }

    pub async fn list_file_paths(&self, repo_id: &str) -> Result<Vec<(String, String)>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT id, path FROM code_files WHERE repo_id = $1",
                &[&repo_id],
            )
            .await?;
        rows.iter()
            .map(|r| Ok((r.try_get::<_, String>(0)?, r.try_get::<_, String>(1)?)))
            .collect()
    }

    // ----- chunks ----------------------------------------------------------

    pub async fn delete_chunks_for_file(&self, file_id: &str) -> Result<()> {
        let client = self.pool.get().await?;
        client
            .execute("DELETE FROM code_chunks WHERE file_id = $1", &[&file_id])
            .await?;
        Ok(())
    }

    pub async fn insert_chunks(&self, chunks: &[CodeChunkRow]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let stmt = tx
            .prepare(
                "INSERT INTO code_chunks \
                    (id, file_id, repo_id, repo_name, path, basename, language, \
                     ordinal, start_line, end_line, byte_start, byte_end, \
                     symbol_kind, symbol_name, symbol_path, parent_symbol, visibility, \
                     doc_comment, signature, is_test, is_public, calls, \
                     content, content_hash, token_count, file_content_hash, git_sha, \
                     prev_chunk_id, next_chunk_id, embedding, \
                     embedding_model, embedding_version, created_at, updated_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,\
                         $18,$19,$20,$21,$22::jsonb,$23,$24,$25,$26,$27,$28,$29,$30::vector,$31,$32,$33,$34)",
            )
            .await?;
        for c in chunks {
            let embedding = c
                .embedding
                .as_ref()
                .map(|v| pgvector::Vector::from(v.clone()));
            let calls_json = serde_json::to_value(&c.calls)?;
            tx.execute(
                &stmt,
                &[
                    &c.id,
                    &c.file_id,
                    &c.repo_id,
                    &c.repo_name,
                    &c.path,
                    &c.basename,
                    &c.language,
                    &c.ordinal,
                    &c.start_line,
                    &c.end_line,
                    &c.byte_start,
                    &c.byte_end,
                    &c.symbol_kind,
                    &c.symbol_name,
                    &c.symbol_path,
                    &c.parent_symbol,
                    &c.visibility,
                    &c.doc_comment,
                    &c.signature,
                    &c.is_test,
                    &c.is_public,
                    &calls_json,
                    &c.content,
                    &c.content_hash,
                    &c.token_count,
                    &c.file_content_hash,
                    &c.git_sha,
                    &c.prev_chunk_id,
                    &c.next_chunk_id,
                    &embedding,
                    &c.embedding_model,
                    &c.embedding_version,
                    &c.created_at,
                    &c.updated_at,
                ],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn search(&self, q: &CodeQuery) -> Result<Vec<CodeChunkHit>> {
        if q.embedding.is_empty() {
            anyhow::bail!("code search embedding cannot be empty");
        }
        let client = self.pool.get().await?;
        let vector = pgvector::Vector::from(q.embedding.clone());
        let limit = q.limit.max(1) as i64;
        let rows = client
            .query(
                "SELECT *, (embedding <=> $1::vector) AS distance \
                 FROM code_chunks \
                 WHERE embedding IS NOT NULL \
                   AND ($2::text IS NULL OR repo_name = $2) \
                   AND ($3::text IS NULL OR language = $3) \
                   AND ($4::text IS NULL OR path LIKE ($4 || '%')) \
                 ORDER BY embedding <=> $1::vector \
                 LIMIT $5",
                &[
                    &vector,
                    &q.repo,
                    &q.language,
                    &q.path_prefix,
                    &limit,
                ],
            )
            .await?;
        rows.iter()
            .map(|row| {
                let chunk = row_to_chunk(row)?;
                let distance: f64 = row.try_get("distance")?;
                Ok(CodeChunkHit {
                    chunk,
                    score: 1.0 - distance as f32,
                })
            })
            .collect()
    }

    // ----- direct (non-vector) lookup paths --------------------------------

    pub async fn lookup_chunks_by_file(
        &self,
        repo_name: &str,
        path: &str,
    ) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT * FROM code_chunks \
                 WHERE repo_name = $1 AND path = $2 \
                 ORDER BY ordinal",
                &[&repo_name, &path],
            )
            .await?;
        rows.iter().map(row_to_chunk).collect()
    }

    pub async fn lookup_chunks_by_basename(&self, basename: &str) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT * FROM code_chunks WHERE basename = $1 ORDER BY repo_name, path, ordinal",
                &[&basename],
            )
            .await?;
        rows.iter().map(row_to_chunk).collect()
    }

    pub async fn lookup_chunks_by_path_pattern(
        &self,
        pattern: &str,
    ) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT * FROM code_chunks WHERE path ILIKE $1 \
                 ORDER BY repo_name, path, ordinal LIMIT 500",
                &[&pattern],
            )
            .await?;
        rows.iter().map(row_to_chunk).collect()
    }

    pub async fn lookup_symbol(
        &self,
        repo: Option<&str>,
        symbol_name: &str,
    ) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let rows = if let Some(r) = repo {
            client
                .query(
                    "SELECT * FROM code_chunks \
                     WHERE symbol_name = $1 AND repo_name = $2 \
                     ORDER BY path, ordinal LIMIT 100",
                    &[&symbol_name, &r],
                )
                .await?
        } else {
            client
                .query(
                    "SELECT * FROM code_chunks \
                     WHERE symbol_name = $1 \
                     ORDER BY repo_name, path, ordinal LIMIT 100",
                    &[&symbol_name],
                )
                .await?
        };
        rows.iter().map(row_to_chunk).collect()
    }

    /// Find chunks whose `calls` JSONB array contains the given symbol — i.e.
    /// callers of `symbol_name`. Backed by the `calls jsonb_path_ops` GIN
    /// index.
    pub async fn find_callers(
        &self,
        symbol_name: &str,
        repo: Option<&str>,
    ) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let needle = serde_json::json!([symbol_name]);
        let rows = if let Some(r) = repo {
            client
                .query(
                    "SELECT * FROM code_chunks \
                     WHERE calls @> $1::jsonb AND repo_name = $2 \
                     ORDER BY repo_name, path, ordinal LIMIT 200",
                    &[&needle, &r],
                )
                .await?
        } else {
            client
                .query(
                    "SELECT * FROM code_chunks \
                     WHERE calls @> $1::jsonb \
                     ORDER BY repo_name, path, ordinal LIMIT 200",
                    &[&needle],
                )
                .await?
        };
        rows.iter().map(row_to_chunk).collect()
    }

    pub async fn find_tests(&self, repo: Option<&str>) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let mut sql = String::from(
            "SELECT * FROM code_chunks WHERE is_test = TRUE",
        );
        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        let r_owned;
        if let Some(r) = repo {
            r_owned = r.to_string();
            sql.push_str(" AND repo_name = $1");
            params.push(&r_owned);
        }
        sql.push_str(" ORDER BY repo_name, path, ordinal LIMIT 500");
        let rows = client.query(sql.as_str(), &params).await?;
        rows.iter().map(row_to_chunk).collect()
    }

    pub async fn find_public_api(&self, repo: &str) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT * FROM code_chunks \
                 WHERE is_public = TRUE AND repo_name = $1 \
                 ORDER BY path, ordinal LIMIT 1000",
                &[&repo],
            )
            .await?;
        rows.iter().map(row_to_chunk).collect()
    }

    pub async fn find_files_by_role(
        &self,
        role: &str,
        repo: Option<&str>,
    ) -> Result<Vec<CodeFile>> {
        let client = self.pool.get().await?;
        let rows = if let Some(r) = repo {
            client
                .query(
                    "SELECT * FROM code_files WHERE role = $1 AND repo_name = $2 \
                     ORDER BY path LIMIT 1000",
                    &[&role, &r],
                )
                .await?
        } else {
            client
                .query(
                    "SELECT * FROM code_files WHERE role = $1 \
                     ORDER BY repo_name, path LIMIT 1000",
                    &[&role],
                )
                .await?
        };
        rows.iter().map(row_to_file).collect()
    }

    pub async fn find_files_importing(&self, module: &str) -> Result<Vec<CodeFile>> {
        let client = self.pool.get().await?;
        let needle = serde_json::json!([module]);
        let rows = client
            .query(
                "SELECT * FROM code_files WHERE imports @> $1::jsonb \
                 ORDER BY repo_name, path LIMIT 500",
                &[&needle],
            )
            .await?;
        rows.iter().map(row_to_file).collect()
    }

    pub async fn list_todos(&self, repo: Option<&str>) -> Result<Vec<(String, String, Json)>> {
        let client = self.pool.get().await?;
        let rows = if let Some(r) = repo {
            client
                .query(
                    "SELECT repo_name, path, todos FROM code_files \
                     WHERE repo_name = $1 AND jsonb_array_length(todos) > 0 \
                     ORDER BY path",
                    &[&r],
                )
                .await?
        } else {
            client
                .query(
                    "SELECT repo_name, path, todos FROM code_files \
                     WHERE jsonb_array_length(todos) > 0 \
                     ORDER BY repo_name, path",
                    &[],
                )
                .await?
        };
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get::<_, String>(0)?,
                    row.try_get::<_, String>(1)?,
                    row.try_get::<_, Json>(2)?,
                ))
            })
            .collect()
    }

    /// Fetch a chunk + N preceding/following chunks from the same file for
    /// context window expansion.
    pub async fn get_chunk_with_neighbors(
        &self,
        chunk_id: &str,
        window: usize,
    ) -> Result<Vec<CodeChunkRow>> {
        let client = self.pool.get().await?;
        let target = client
            .query_opt(
                "SELECT file_id, ordinal FROM code_chunks WHERE id = $1",
                &[&chunk_id],
            )
            .await?;
        let Some(t) = target else {
            return Ok(Vec::new());
        };
        let file_id: String = t.try_get(0)?;
        let ord: i32 = t.try_get(1)?;
        let w = window.min(50) as i32;
        let rows = client
            .query(
                "SELECT * FROM code_chunks \
                 WHERE file_id = $1 AND ordinal BETWEEN $2 AND $3 \
                 ORDER BY ordinal",
                &[&file_id, &(ord - w), &(ord + w)],
            )
            .await?;
        rows.iter().map(row_to_chunk).collect()
    }

    pub async fn recent_files(&self, limit: usize) -> Result<Vec<CodeFile>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT * FROM code_files ORDER BY indexed_at DESC LIMIT $1",
                &[&(limit.max(1) as i64)],
            )
            .await?;
        rows.iter().map(row_to_file).collect()
    }
}

// ----- row decoders ---------------------------------------------------------

fn row_to_repo(row: &tokio_postgres::Row) -> Result<CodeRepo> {
    let include_globs: Json = row.try_get("include_globs")?;
    let exclude_globs: Json = row.try_get("exclude_globs")?;
    Ok(CodeRepo {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        root_path: row.try_get("root_path")?,
        include_globs: serde_json::from_value(include_globs).unwrap_or_default(),
        exclude_globs: serde_json::from_value(exclude_globs).unwrap_or_default(),
        enabled: row.try_get("enabled")?,
        default_branch: row.try_get("default_branch")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_file(row: &tokio_postgres::Row) -> Result<CodeFile> {
    let imports: Json = row.try_get("imports")?;
    let outline: Json = row.try_get("outline")?;
    let todos: Json = row.try_get("todos")?;
    Ok(CodeFile {
        id: row.try_get("id")?,
        repo_id: row.try_get("repo_id")?,
        repo_name: row.try_get("repo_name")?,
        path: row.try_get("path")?,
        basename: row.try_get("basename")?,
        dir: row.try_get("dir")?,
        extension: row.try_get("extension")?,
        language: row.try_get("language")?,
        size_bytes: row.try_get("size_bytes")?,
        line_count: row.try_get("line_count")?,
        git_sha: row.try_get("git_sha")?,
        git_branch: row.try_get("git_branch")?,
        content_hash: row.try_get("content_hash")?,
        mtime: row.try_get("mtime")?,
        indexed_at: row.try_get("indexed_at")?,
        summary: row.try_get("summary")?,
        role: row.try_get("role")?,
        imports: serde_json::from_value::<Vec<String>>(imports).unwrap_or_default(),
        outline: serde_json::from_value::<Vec<OutlineEntry>>(outline).unwrap_or_default(),
        todos: serde_json::from_value::<Vec<TodoEntry>>(todos).unwrap_or_default(),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_chunk(row: &tokio_postgres::Row) -> Result<CodeChunkRow> {
    let calls_json: Json = row.try_get("calls")?;
    let embedding: Option<pgvector::Vector> = row.try_get("embedding").ok();
    Ok(CodeChunkRow {
        id: row.try_get("id")?,
        file_id: row.try_get("file_id")?,
        repo_id: row.try_get("repo_id")?,
        repo_name: row.try_get("repo_name")?,
        path: row.try_get("path")?,
        basename: row.try_get("basename")?,
        language: row.try_get("language")?,
        ordinal: row.try_get("ordinal")?,
        start_line: row.try_get("start_line")?,
        end_line: row.try_get("end_line")?,
        byte_start: row.try_get("byte_start")?,
        byte_end: row.try_get("byte_end")?,
        symbol_kind: row.try_get("symbol_kind")?,
        symbol_name: row.try_get("symbol_name")?,
        symbol_path: row.try_get("symbol_path")?,
        parent_symbol: row.try_get("parent_symbol")?,
        visibility: row.try_get("visibility")?,
        doc_comment: row.try_get("doc_comment")?,
        signature: row.try_get("signature")?,
        is_test: row.try_get("is_test")?,
        is_public: row.try_get("is_public")?,
        calls: serde_json::from_value(calls_json).unwrap_or_default(),
        content: row.try_get("content")?,
        content_hash: row.try_get("content_hash")?,
        token_count: row.try_get("token_count")?,
        file_content_hash: row.try_get("file_content_hash")?,
        git_sha: row.try_get("git_sha")?,
        prev_chunk_id: row.try_get("prev_chunk_id")?,
        next_chunk_id: row.try_get("next_chunk_id")?,
        embedding: embedding.map(|v| v.to_vec()),
        embedding_model: row.try_get("embedding_model")?,
        embedding_version: row.try_get("embedding_version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
