//! Code-search store: per-repo snapshots merged in from the external
//! `.codesearch/codesearch.db` sqlite + sqlite-vec sidecar that each repo's
//! local indexer produces (symbols + calls + files + 384-dim float32
//! all-MiniLM-L6-v2 embeddings).
//!
//! Postgres is the only backend — code search routes 503 when
//! `RAG_DATABASE_URL` is unset, mirroring the chunks-table limitation of the
//! main store.

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use rusqlite::OptionalExtension;
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use tokio::runtime::Handle;
use tracing::info;

/// Width of the code embedding columns. Fixed by the external indexer
/// (all-MiniLM-L6-v2) and by `vector(384)` in migration 0018.
pub const CODE_EMBEDDING_DIM: usize = 384;
pub const CODE_EMBEDDING_MODEL: &str = "all-MiniLM-L6-v2";
pub const CODE_EMBEDDING_VERSION: i32 = 1;

// ---------------------------------------------------------------------------
// Public shapes (mirror frontend/lib/api/types.ts `Code*` interfaces)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeRepoSummary {
    pub name: String,
    pub root_path: String,
    pub enabled: bool,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeFileMeta {
    pub path: String,
    pub basename: String,
    pub language: Option<String>,
    pub role: Option<String>,
    pub summary: Option<String>,
    pub size_bytes: i64,
    pub line_count: i32,
    pub indexed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeOutlineEntry {
    pub kind: String,
    pub name: String,
    pub line: i32,
    pub signature: Option<String>,
    pub is_public: Option<bool>,
    pub is_test: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeTodoEntry {
    pub kind: String,
    pub line: i32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeFileDetail {
    pub path: String,
    pub basename: String,
    pub language: Option<String>,
    pub role: Option<String>,
    pub summary: Option<String>,
    pub size_bytes: i64,
    pub line_count: i32,
    pub indexed_at: i64,
    pub outline: Vec<CodeOutlineEntry>,
    pub todos: Vec<CodeTodoEntry>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeSearchHit {
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeUploadStats {
    pub repo: String,
    pub files: usize,
    pub symbols: usize,
    pub symbols_skipped: usize,
    pub calls: usize,
    pub source_version: String,
}

// ---------------------------------------------------------------------------
// Snapshot parsing (sqlite + vec0 -> ParsedCodeRepo)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ParsedCodeRepo {
    pub source_version: String,
    pub files: Vec<ParsedCodeFile>,
    pub symbols: Vec<ParsedCodeSymbol>,
    pub calls: Vec<ParsedCodeCall>,
    /// Symbols dropped because neither the vec0 table nor the fallback
    /// table had an embedding for them.
    pub symbols_skipped: usize,
}

#[derive(Debug)]
pub struct ParsedCodeFile {
    pub path: String,
    pub size_bytes: i64,
    pub mtime: i64,
}

#[derive(Debug)]
pub struct ParsedCodeSymbol {
    pub path: String,
    pub line: i32,
    pub end_line: i32,
    pub kind: String,
    pub name: String,
    pub qualname: String,
    pub signature: String,
    pub doc: String,
    pub lang: String,
    pub text: String,
    pub visibility: String,
    pub exemplar: String,
    pub hotpath: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug)]
pub struct ParsedCodeCall {
    pub caller_path: String,
    pub caller_symbol: String,
    pub caller_line: i32,
    pub callee_name: String,
    pub callee_operand: String,
    pub call_line: i32,
    pub call_column: i32,
}

/// Parse an uploaded `.codesearch/codesearch.db` snapshot. The bytes are
/// spilled to a temp file because the vec0 extension needs a real sqlite
/// file; the temp copy is removed before returning.
pub fn parse_codesearch_db(bytes: &[u8]) -> Result<ParsedCodeRepo> {
    let tmp = std::env::temp_dir().join(format!("codesearch-upload-{}.db", uuid::Uuid::now_v7()));
    {
        let mut file = std::fs::File::create(&tmp)
            .with_context(|| format!("creating temp snapshot at {}", tmp.display()))?;
        file.write_all(bytes)?;
    }
    let result = parse_codesearch_db_file(&tmp);
    let _ = std::fs::remove_file(&tmp);
    result
}

fn table_exists(conn: &rusqlite::Connection, name: &str) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

fn parse_codesearch_db_file(path: &Path) -> Result<ParsedCodeRepo> {
    super::schema::register_sqlite_vec();
    let conn = rusqlite::Connection::open(path)
        .with_context(|| format!("opening codesearch snapshot {}", path.display()))?;

    if !table_exists(&conn, "symbols")? {
        anyhow::bail!("snapshot has no `symbols` table — not a codesearch.db file?");
    }

    // Indexer version (embeddings_info key/value; absent in exotic builds).
    // The column is sqlite `any` — read it as a dynamic Value.
    let source_version = if table_exists(&conn, "embeddings_info")? {
        conn.query_row(
            "SELECT value FROM embeddings_info WHERE key = 'CREATE_VERSION'",
            [],
            |row| row.get::<_, SqlValue>(0),
        )
        .optional()?
        .and_then(|v| match v {
            SqlValue::Text(s) => Some(s),
            SqlValue::Integer(i) => Some(i.to_string()),
            SqlValue::Real(f) => Some(f.to_string()),
            _ => None,
        })
        .unwrap_or_default()
    } else {
        String::new()
    };

    // Files table lists every indexed file (with or without symbols).
    let mut files = Vec::new();
    if table_exists(&conn, "files")? {
        let mut stmt = conn.prepare("SELECT path, size, mtime FROM files")?;
        let rows = stmt.query_map([], |row| {
            Ok(ParsedCodeFile {
                path: row.get(0)?,
                size_bytes: row.get::<_, i64>(1)?,
                mtime: row.get::<_, i64>(2)?,
            })
        })?;
        for row in rows {
            files.push(row?);
        }
    }

    // Vectors: the vec0 table is authoritative; fallback_embeddings covers
    // symbols the vec table rejected. Either table may be missing/empty.
    let mut embeddings: HashMap<i64, Vec<f32>> = HashMap::new();
    for table in ["embeddings", "fallback_embeddings"] {
        if !table_exists(&conn, table)? {
            continue;
        }
        let sql = match table {
            "embeddings" => "SELECT symbol_id, embedding FROM embeddings",
            _ => "SELECT symbol_id, embedding FROM fallback_embeddings",
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(stmt) => stmt,
            Err(err) => {
                info!(table, error = %err, "codesearch: skipping embedding table");
                continue;
            }
        };
        let rows =
            stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)))?;
        for row in rows {
            let (symbol_id, blob) = row?;
            let vec = decode_f32_blob(&blob)?;
            embeddings.entry(symbol_id).or_insert(vec);
        }
    }

    let mut symbols = Vec::new();
    let mut symbols_skipped = 0usize;
    let mut stmt = conn.prepare(
        "SELECT id, path, line, end_line, kind, name, qualname, signature, doc, lang, \
                text, visibility, exemplar, hotpath \
         FROM symbols ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, String>(13)?,
        ))
    })?;
    for row in rows {
        let (id, path, line, end_line, kind, name, qualname, signature, doc, lang, text, visibility, exemplar, hotpath) = row?;
        let Some(embedding) = embeddings.get(&id) else {
            symbols_skipped += 1;
            continue;
        };
        if embedding.len() != CODE_EMBEDDING_DIM {
            return Err(anyhow!(
                "symbol {id} ({name}) has {}-dim embedding, expected {CODE_EMBEDDING_DIM}",
                embedding.len()
            ));
        }
        symbols.push(ParsedCodeSymbol {
            path,
            line: line as i32,
            end_line: end_line as i32,
            kind,
            name,
            qualname,
            signature,
            doc,
            lang,
            text,
            visibility,
            exemplar,
            hotpath,
            embedding: embedding.clone(),
        });
    }

    let mut calls = Vec::new();
    if table_exists(&conn, "calls")? {
        let mut stmt = conn.prepare(
            "SELECT caller_path, caller_symbol, caller_line, callee_name, callee_operand, \
                    call_line, call_column \
             FROM calls",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ParsedCodeCall {
                caller_path: row.get(0)?,
                caller_symbol: row.get(1)?,
                caller_line: row.get::<_, i64>(2)? as i32,
                callee_name: row.get(3)?,
                callee_operand: row.get(4)?,
                call_line: row.get::<_, i64>(5)? as i32,
                call_column: row.get::<_, i64>(6)? as i32,
            })
        })?;
        for row in rows {
            calls.push(row?);
        }
    }

    Ok(ParsedCodeRepo {
        source_version,
        files,
        symbols,
        calls,
        symbols_skipped,
    })
}

fn decode_f32_blob(blob: &[u8]) -> Result<Vec<f32>> {
    if blob.len() % 4 != 0 {
        return Err(anyhow!(
            "embedding blob is {} bytes, not a multiple of 4 (float32)",
            blob.len()
        ));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn basename_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

/// Best-effort display language from the file extension. The symbol rows
/// carry a `lang` field too, but the files table doesn't.
fn language_of_path(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    if ext == path.to_ascii_lowercase() {
        return None; // no extension in path
    }
    let lang = match ext.as_str() {
        "rs" => "rust",
        "go" => "go",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "py" => "python",
        "rb" => "ruby",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" | "cxx" => "cpp",
        "cs" => "csharp",
        "php" => "php",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "shell",
        "sql" => "sql",
        "tf" | "tfvars" => "terraform",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "json" => "json",
        "md" | "mdx" => "markdown",
        "vue" => "vue",
        "svelte" => "svelte",
        _ => return None,
    };
    Some(lang.to_owned())
}

// ---------------------------------------------------------------------------
// CodeStore (Postgres)
// ---------------------------------------------------------------------------

pub struct CodeStore {
    pool: Pool,
    runtime: Handle,
}

impl CodeStore {
    pub fn new(pool: Pool, runtime: Handle) -> Self {
        Self { pool, runtime }
    }

    /// Same bridge contract as `PostgresVectorStore::block` — only call from
    /// `spawn_blocking` threads.
    fn block<F, T>(&self, fut: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.runtime.block_on(fut)
    }

    /// Replace a repo's snapshot wholesale: idempotent re-push semantics,
    /// matching the external indexer's "push sweeps stale entries" behavior.
    pub fn upsert_repo(
        &self,
        name: &str,
        root_path: &str,
        parsed: ParsedCodeRepo,
    ) -> Result<CodeUploadStats> {
        let pool = self.pool.clone();
        let name = name.to_owned();
        let root_path = root_path.to_owned();

        let stats = self.block(async move {
            let mut client = pool
                .get()
                .await
                .context("acquiring postgres connection")?;
            let tx = client.transaction().await?;

            tx.execute(
                "INSERT INTO code_repos (name, root_path, source_version) \
                 VALUES ($1, $2, $3) \
                 ON CONFLICT (name) DO UPDATE SET \
                     root_path = EXCLUDED.root_path, \
                     source_version = EXCLUDED.source_version, \
                     updated_at = now()",
                &[&name, &root_path, &parsed.source_version],
            )
            .await?;
            tx.execute("DELETE FROM code_files WHERE repo = $1", &[&name])
                .await?;
            tx.execute("DELETE FROM code_symbols WHERE repo = $1", &[&name])
                .await?;
            tx.execute("DELETE FROM code_calls WHERE repo = $1", &[&name])
                .await?;

            // max(end_line) per file, for display in the file browser.
            let mut max_lines: HashMap<String, i32> = HashMap::new();
            for s in &parsed.symbols {
                let slot = max_lines.entry(s.path.clone()).or_insert(0);
                if s.end_line > *slot {
                    *slot = s.end_line;
                }
            }

            let file_stmt = tx
                .prepare(
                    "INSERT INTO code_files (repo, path, basename, language, size_bytes, mtime, line_count) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .await?;
            for f in &parsed.files {
                let line_count = max_lines.get(&f.path).copied().unwrap_or(0);
                tx.execute(
                    &file_stmt,
                    &[
                        &name,
                        &f.path,
                        &basename_of(&f.path),
                        &language_of_path(&f.path),
                        &f.size_bytes,
                        &f.mtime,
                        &line_count,
                    ],
                )
                .await?;
            }

            let sym_stmt = tx
                .prepare(
                    "INSERT INTO code_symbols \
                     (repo, path, line, end_line, kind, name, qualname, signature, doc, lang, \
                      text, visibility, exemplar, hotpath, dense_embedding, embedding_model, embedding_version) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
                )
                .await?;
            for s in &parsed.symbols {
                let vector = pgvector::Vector::from(s.embedding.clone());
                tx.execute(
                    &sym_stmt,
                    &[
                        &name,
                        &s.path,
                        &s.line,
                        &s.end_line,
                        &s.kind,
                        &s.name,
                        &s.qualname,
                        &s.signature,
                        &s.doc,
                        &s.lang,
                        &s.text,
                        &s.visibility,
                        &s.exemplar,
                        &s.hotpath,
                        &vector,
                        &CODE_EMBEDDING_MODEL,
                        &CODE_EMBEDDING_VERSION,
                    ],
                )
                .await?;
            }

            let call_stmt = tx
                .prepare(
                    "INSERT INTO code_calls \
                     (repo, caller_path, caller_symbol, caller_line, callee_name, callee_operand, call_line, call_column) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .await?;
            for c in &parsed.calls {
                tx.execute(
                    &call_stmt,
                    &[
                        &name,
                        &c.caller_path,
                        &c.caller_symbol,
                        &c.caller_line,
                        &c.callee_name,
                        &c.callee_operand,
                        &c.call_line,
                        &c.call_column,
                    ],
                )
                .await?;
            }

            tx.commit().await?;
            Ok(CodeUploadStats {
                repo: name,
                files: parsed.files.len(),
                symbols: parsed.symbols.len(),
                symbols_skipped: parsed.symbols_skipped,
                calls: parsed.calls.len(),
                source_version: parsed.source_version,
            })
        });
        match stats {
            Ok(s) => {
                info!(
                    repo = %s.repo,
                    files = s.files,
                    symbols = s.symbols,
                    calls = s.calls,
                    "codesearch: repo snapshot merged"
                );
                Ok(s)
            }
            Err(e) => Err(e),
        }
    }

    pub fn list_repos(&self) -> Result<Vec<CodeRepoSummary>> {
        let pool = self.pool.clone();
        self.block(async move {
            let client = pool
                .get()
                .await
                .context("acquiring postgres connection")?;
            let rows = client
                .query(
                    "SELECT r.name, r.root_path, r.enabled, COUNT(f.path)::BIGINT AS file_count \
                     FROM code_repos r \
                     LEFT JOIN code_files f ON f.repo = r.name \
                     GROUP BY r.name, r.root_path, r.enabled \
                     ORDER BY r.name",
                    &[],
                )
                .await?;
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                out.push(CodeRepoSummary {
                    name: row.get("name"),
                    root_path: row.get("root_path"),
                    enabled: row.get("enabled"),
                    file_count: row.get("file_count"),
                });
            }
            Ok(out)
        })
    }

    pub fn delete_repo(&self, name: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let name = name.to_owned();
        self.block(async move {
            let client = pool.get().await?;
            // Children cascade via FK; report whether the repo existed.
            let n = client
                .execute("DELETE FROM code_repos WHERE name = $1", &[&name])
                .await?;
            Ok(n > 0)
        })
    }

    pub fn list_files(&self, repo: &str) -> Result<Vec<CodeFileMeta>> {
        let pool = self.pool.clone();
        let repo = repo.to_owned();
        self.block(async move {
            let client = pool
                .get()
                .await
                .context("acquiring postgres connection")?;
            let rows = client
                .query(
                    "SELECT path, basename, language, size_bytes, line_count, indexed_at \
                     FROM code_files WHERE repo = $1 ORDER BY path",
                    &[&repo],
                )
                .await?;
            Ok(rows.iter().map(file_meta_from_row).collect())
        })
    }

    pub fn file_detail(&self, repo: &str, path: &str) -> Result<Option<CodeFileDetail>> {
        let pool = self.pool.clone();
        let repo = repo.to_owned();
        let path = path.to_owned();
        self.block(async move {
            let client = pool
                .get()
                .await
                .context("acquiring postgres connection")?;
            let file_row = client
                .query_opt(
                    "SELECT path, basename, language, size_bytes, line_count, indexed_at \
                     FROM code_files WHERE repo = $1 AND path = $2",
                    &[&repo, &path],
                )
                .await?;
            let Some(file_row) = file_row else {
                return Ok(None);
            };

            // The snapshot's outline IS the symbol table for this file.
            let sym_rows = client
                .query(
                    "SELECT kind, name, line, signature, visibility, lang \
                     FROM code_symbols WHERE repo = $1 AND path = $2 ORDER BY line",
                    &[&repo, &path],
                )
                .await?;
            let outline = sym_rows
                .iter()
                .map(|r| {
                    let kind: String = r.get("kind");
                    let name: String = r.get("name");
                    let visibility: String = r.get("visibility");
                    CodeOutlineEntry {
                        is_test: Some(
                            kind.eq_ignore_ascii_case("test")
                                || name.to_ascii_lowercase().contains("test"),
                        ),
                        kind,
                        name,
                        line: r.get("line"),
                        signature: r.get::<_, Option<String>>("signature"),
                        is_public: Some(
                            visibility.eq_ignore_ascii_case("export")
                                || visibility.eq_ignore_ascii_case("public"),
                        ),
                    }
                })
                .collect();

            // The codesearch snapshot has no import/todo extracts; the
            // frontend renders these sections only when non-empty.
            let meta = file_meta_from_row(&file_row);
            Ok(Some(CodeFileDetail {
                outline,
                todos: Vec::new(),
                imports: Vec::new(),
                path: meta.path,
                basename: meta.basename,
                language: meta.language,
                role: meta.role,
                summary: meta.summary,
                size_bytes: meta.size_bytes,
                line_count: meta.line_count,
                indexed_at: meta.indexed_at,
            }))
        })
    }

    /// Cosine KNN over code symbols. `embedding` must be a query vector from
    /// the same all-MiniLM-L6-v2 model the snapshots were embedded with.
    pub fn search(
        &self,
        embedding: &[f32],
        repo: Option<&str>,
        language: Option<&str>,
        path_prefix: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CodeSearchHit>> {
        if embedding.len() != CODE_EMBEDDING_DIM {
            return Err(anyhow!(
                "query embedding is {}-dim, code index expects {CODE_EMBEDDING_DIM}",
                embedding.len()
            ));
        }
        let limit = limit.clamp(1, 200) as i64;
        let pool = self.pool.clone();
        let vector = pgvector::Vector::from(embedding.to_vec());
        let repo = repo.map(str::to_owned);
        let language = language.map(str::to_owned);
        let path_prefix = path_prefix.map(str::to_owned);

        self.block(async move {
            let client = pool
                .get()
                .await
                .context("acquiring postgres connection")?;
            let rows = client
                .query(
                    "SELECT s.repo, s.path, f.language, s.kind, s.name, s.signature, \
                            s.line, s.end_line, s.text, \
                            (s.dense_embedding <=> $1::vector)::REAL AS distance \
                     FROM code_symbols s \
                     LEFT JOIN code_files f ON f.repo = s.repo AND f.path = s.path \
                     WHERE ($2::text IS NULL OR s.repo = $2) \
                       AND ($3::text IS NULL OR s.lang = $3) \
                       AND ($4::text IS NULL OR s.path LIKE $4 || '%') \
                     ORDER BY s.dense_embedding <=> $1::vector \
                     LIMIT $5",
                    &[&vector, &repo, &language, &path_prefix, &limit],
                )
                .await?;
            Ok(rows
                .iter()
                .map(|row| {
                    let distance: f32 = row.get("distance");
                    CodeSearchHit {
                        repo: row.get("repo"),
                        path: row.get("path"),
                        language: row.get("language"),
                        symbol_kind: row.get("kind"),
                        symbol_name: row.get("name"),
                        signature: row.get("signature"),
                        start_line: row.get("line"),
                        end_line: row.get("end_line"),
                        snippet: row.get("text"),
                        score: (1.0 - distance).max(0.0),
                    }
                })
                .collect())
        })
    }
}

fn file_meta_from_row(row: &tokio_postgres::Row) -> CodeFileMeta {
    let indexed_at: DateTime<Utc> = row.get("indexed_at");
    CodeFileMeta {
        path: row.get("path"),
        basename: row.get("basename"),
        language: row.get("language"),
        role: None,
        summary: None,
        size_bytes: row.get("size_bytes"),
        line_count: row.get("line_count"),
        indexed_at: indexed_at.timestamp_millis(),
    }
}
