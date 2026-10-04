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
use std::collections::{HashMap, HashSet};
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

/// One node in a symbol call graph: a resolved row from `code_symbols`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeSymbolRef {
    pub path: String,
    pub name: String,
    pub qualname: String,
    pub kind: String,
    pub line: i32,
    pub end_line: i32,
    pub signature: Option<String>,
}

/// One visited symbol in the multi-hop call graph, at its minimum BFS depth
/// from the anchor (anchor = 0).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeGraphNode {
    pub symbol: CodeSymbolRef,
    pub depth: usize,
}

/// One resolved call edge: `from` calls `to` at `call_line` in `from`'s file.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeGraphEdge {
    pub from: CodeSymbolRef,
    pub to: CodeSymbolRef,
    pub call_line: i32,
}

/// Call-graph neighborhood of one symbol, BFS-expanded `depth` hops in both
/// directions (what it calls, what calls it). `callees`/`callers` remain the
/// anchor's direct (1-hop) lists; `nodes`/`edges` carry the full multi-hop
/// view for graph rendering.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeSymbolGraph {
    pub repo: String,
    pub symbol: CodeSymbolRef,
    /// Traversal depth the graph was built with (1 = direct neighbors only).
    pub depth: usize,
    pub callees: Vec<CodeSymbolRef>,
    pub callers: Vec<CodeSymbolRef>,
    /// Every visited symbol across all hops, anchor first, in BFS order.
    pub nodes: Vec<CodeGraphNode>,
    /// Resolved call edges (caller -> callee) across all hops, discovery
    /// order. One edge per (caller, callee) pair, at the earliest call line.
    pub edges: Vec<CodeGraphEdge>,
    /// Outgoing callee names of the anchor that matched no indexed symbol
    /// (dynamic dispatch, stdlib, external crates). Distinct, call-order.
    pub unresolved_callees: Vec<String>,
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
    pub embedding: Option<Vec<f32>>,
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
    let symbols_skipped = 0usize;
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
        let embedding = match embeddings.get(&id) {
            Some(vec) => {
                if vec.len() != CODE_EMBEDDING_DIM {
                    return Err(anyhow!(
                        "symbol {id} ({name}) has {}-dim embedding, expected {CODE_EMBEDDING_DIM}",
                        vec.len()
                    ));
                }
                Some(vec.clone())
            }
            None => None,
        };
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
            embedding,
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
                let vector = s.embedding.as_ref().map(|e| pgvector::Vector::from(e.clone()));
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

    /// Call-graph neighborhood for one symbol, BFS-expanded `depth` hops in
    /// both directions. `code_calls` rows carry simple names, not symbol ids
    /// — the external indexer doesn't resolve callees — so edges are matched
    /// by name and disambiguated by preferring candidates in the same file.
    /// `limit` caps each direction per expanded symbol; `max_nodes` caps the
    /// total visited nodes including the anchor. Call cycles (a -> b -> c -> a)
    /// terminate the walk: every symbol expands exactly once per query (the
    /// visited set keys on path/name/line) and edges dedup per symbol pair.
    pub fn symbol_graph(
        &self,
        repo: &str,
        path: &str,
        symbol: &str,
        limit: usize,
        depth: usize,
        max_nodes: usize,
    ) -> Result<Option<CodeSymbolGraph>> {
        let limit = limit.clamp(1, 64);
        let depth = depth.clamp(1, 5);
        let max_nodes = max_nodes.clamp(1, 500);
        let pool = self.pool.clone();
        let repo = repo.to_owned();
        let path = path.to_owned();
        let symbol = symbol.to_owned();

        self.block(async move {
            let client = pool
                .get()
                .await
                .context("acquiring postgres connection")?;

            let anchor_row = client
                .query_opt(
                    "SELECT path, name, qualname, kind, line, end_line, signature \
                     FROM code_symbols \
                     WHERE repo = $1 AND path = $2 AND (name = $3 OR qualname = $3) \
                     ORDER BY line LIMIT 1",
                    &[&repo, &path, &symbol],
                )
                .await?;
            let Some(anchor_row) = anchor_row else {
                return Ok(None);
            };
            let anchor = symbol_ref_from_row(&anchor_row);

            let key_of = |r: &CodeSymbolRef| (r.path.clone(), r.name.clone(), r.line);
            let same_ref = |a: &CodeSymbolRef, b: &CodeSymbolRef| {
                a.path == b.path && a.name == b.name && a.line == b.line
            };

            let mut nodes: Vec<CodeGraphNode> =
                vec![CodeGraphNode { symbol: anchor.clone(), depth: 0 }];
            let mut index_of: HashMap<SymbolKey, usize> = HashMap::new();
            index_of.insert(key_of(&anchor), 0);
            let mut edges: Vec<CodeGraphEdge> = Vec::new();
            let mut edge_seen: HashSet<(SymbolKey, SymbolKey)> = HashSet::new();

            let mut direct_callees: Vec<CodeSymbolRef> = Vec::new();
            let mut direct_callers: Vec<CodeSymbolRef> = Vec::new();
            let mut unresolved_callees: Vec<String> = Vec::new();

            // BFS by level so name resolution batches per level: collect raw
            // call sites for the whole frontier, resolve once, then wire.
            let mut frontier: Vec<CodeSymbolRef> = vec![anchor.clone()];
            for level in 0..depth {
                if frontier.is_empty() || nodes.len() >= max_nodes {
                    break;
                }

                struct Sites {
                    node: CodeSymbolRef,
                    /// (callee name, earliest call line), call order, per name.
                    callees: Vec<(String, i32)>,
                    /// (caller path, caller base name, earliest call line).
                    callers: Vec<(String, String, i32)>,
                }
                let mut level_sites: Vec<Sites> = Vec::with_capacity(frontier.len());
                for node in &frontier {
                    // Outgoing call sites: calls made from inside this symbol.
                    // The indexer records the caller's simple name, sometimes
                    // qualified.
                    let callee_rows = client
                        .query(
                            "SELECT callee_name, call_line FROM code_calls \
                             WHERE repo = $1 AND caller_path = $2 \
                               AND (caller_symbol = $3 OR caller_symbol = $4 \
                                    OR caller_symbol LIKE '%.' || $3 \
                                    OR caller_symbol LIKE '%::' || $3) \
                             ORDER BY call_line LIMIT 512",
                            &[&repo, &node.path, &node.name, &node.qualname],
                        )
                        .await?;
                    let mut callees: Vec<(String, i32)> = Vec::new();
                    for row in &callee_rows {
                        let name: String = row.get("callee_name");
                        let line: i32 = row.get("call_line");
                        if name.is_empty() {
                            continue;
                        }
                        match callees.iter_mut().find(|(n, _)| n == &name) {
                            Some(slot) => slot.1 = slot.1.min(line),
                            None => callees.push((name, line)),
                        }
                    }

                    // Incoming call sites: any call whose callee name matches.
                    let caller_rows = client
                        .query(
                            "SELECT caller_path, caller_symbol, MIN(call_line) AS call_line \
                             FROM code_calls \
                             WHERE repo = $1 AND callee_name = $2 \
                             GROUP BY caller_path, caller_symbol \
                             ORDER BY caller_path, caller_symbol LIMIT 512",
                            &[&repo, &node.name],
                        )
                        .await?;
                    let mut callers: Vec<(String, String, i32)> = Vec::new();
                    for row in &caller_rows {
                        let caller_path: String = row.get("caller_path");
                        let raw: String = row.get("caller_symbol");
                        let base = raw.rsplit(['.', ':']).next().unwrap_or(&raw).to_owned();
                        if base.is_empty() {
                            continue;
                        }
                        if !callers
                            .iter()
                            .any(|(p, n, _): &(String, String, i32)| {
                                p == &caller_path && n == &base
                            })
                        {
                            callers.push((caller_path, base, row.get("call_line")));
                        }
                    }

                    level_sites.push(Sites {
                        node: node.clone(),
                        callees,
                        callers,
                    });
                }

                let mut names: Vec<String> = Vec::new();
                for sites in &level_sites {
                    for (n, _) in &sites.callees {
                        if !names.contains(n) {
                            names.push(n.clone());
                        }
                    }
                    for (_, n, _) in &sites.callers {
                        if !names.contains(n) {
                            names.push(n.clone());
                        }
                    }
                }
                let candidates = resolve_names(&client, &repo, &names).await?;

                let mut next_frontier: Vec<CodeSymbolRef> = Vec::new();
                for sites in level_sites {
                    let node = sites.node;

                    let mut fan_out = 0usize;
                    for (name, call_line) in &sites.callees {
                        if fan_out >= limit {
                            break;
                        }
                        let Some(row) = candidates
                            .iter()
                            .filter(|r| r.get::<_, String>("name") == *name)
                            .min_by_key(|r| {
                                (r.get::<_, String>("path") != node.path, r.get::<_, String>("path"))
                            })
                        else {
                            if level == 0
                                && unresolved_callees.len() < 24
                                && !unresolved_callees.contains(name)
                            {
                                unresolved_callees.push(name.clone());
                            }
                            continue;
                        };
                        let target = symbol_ref_from_row(row);
                        if same_ref(&target, &node) {
                            continue; // recursion renders as the node itself
                        }
                        fan_out += 1;
                        if try_add_node(
                            target.clone(),
                            level + 1,
                            max_nodes,
                            &mut nodes,
                            &mut index_of,
                            &mut next_frontier,
                        )
                        .is_some()
                        {
                            try_add_edge(&node, &target, *call_line, &mut edges, &mut edge_seen);
                        }
                        if level == 0 && !direct_callees.iter().any(|c| same_ref(c, &target)) {
                            direct_callees.push(target);
                        }
                    }

                    let mut fan_in = 0usize;
                    for (caller_path, base, call_line) in &sites.callers {
                        if fan_in >= limit {
                            break;
                        }
                        let Some(row) = candidates
                            .iter()
                            .filter(|r| r.get::<_, String>("name") == *base)
                            .min_by_key(|r| {
                                (
                                    r.get::<_, String>("path") != *caller_path,
                                    r.get::<_, String>("path"),
                                )
                            })
                        else {
                            continue;
                        };
                        let source = symbol_ref_from_row(row);
                        if same_ref(&source, &node) {
                            continue;
                        }
                        fan_in += 1;
                        if try_add_node(
                            source.clone(),
                            level + 1,
                            max_nodes,
                            &mut nodes,
                            &mut index_of,
                            &mut next_frontier,
                        )
                        .is_some()
                        {
                            try_add_edge(&source, &node, *call_line, &mut edges, &mut edge_seen);
                        }
                        if level == 0 && !direct_callers.iter().any(|c| same_ref(c, &source)) {
                            direct_callers.push(source);
                        }
                    }
                }

                frontier = next_frontier;
            }

            Ok(Some(CodeSymbolGraph {
                repo,
                symbol: anchor,
                depth,
                callees: direct_callees,
                callers: direct_callers,
                nodes,
                edges,
                unresolved_callees,
            }))
        })
    }

    /// Full-text BM25 / cover-density ranking over code symbols.
    ///
    /// Combines PostgreSQL weighted full-text search (`tsvector` over name, qualname,
    /// signature, doc, text) with exact and prefix symbol-name boosts.
    pub fn search(
        &self,
        query: &str,
        repo: Option<&str>,
        language: Option<&str>,
        path_prefix: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CodeSearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 200) as i64;
        let pool = self.pool.clone();
        let (or_query, and_query) = prepare_code_search_query(query);
        let raw_query = query
            .trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
            .trim()
            .to_owned();
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
                            ( \
                                CASE \
                                    WHEN $5::text IS NOT NULL AND s.tsv @@ to_tsquery('simple', $5) THEN \
                                        ts_rank_cd(ARRAY[0.1, 0.2, 0.4, 1.0], s.tsv, to_tsquery('simple', $5), 32 | 1) \
                                    ELSE 0.0 \
                                END \
                                + \
                                CASE \
                                    WHEN $6::text IS NOT NULL AND s.tsv @@ to_tsquery('simple', $6) THEN 0.25 \
                                    ELSE 0.0 \
                                END \
                                + \
                                CASE \
                                    WHEN lower(s.name) = lower($7) THEN 1.0 \
                                    WHEN lower(s.qualname) = lower($7) THEN 0.8 \
                                    WHEN lower(s.name) LIKE lower($7) || '%' THEN 0.5 \
                                    WHEN lower(s.name) LIKE '%' || lower($7) || '%' THEN 0.3 \
                                    ELSE 0.0 \
                                END \
                                + \
                                CASE \
                                    WHEN s.signature ILIKE '%' || $7 || '%' THEN 0.15 \
                                    ELSE 0.0 \
                                END \
                            )::REAL AS raw_score \
                     FROM code_symbols s \
                     LEFT JOIN code_files f ON f.repo = s.repo AND f.path = s.path \
                     WHERE ($1::text IS NULL OR s.repo = $1) \
                       AND ($2::text IS NULL OR s.lang = $2) \
                       AND ($3::text IS NULL OR s.path LIKE $3 || '%') \
                       AND ( \
                           ($5::text IS NOT NULL AND s.tsv @@ to_tsquery('simple', $5)) \
                           OR s.name ILIKE '%' || $7 || '%' \
                           OR s.signature ILIKE '%' || $7 || '%' \
                       ) \
                     ORDER BY raw_score DESC, s.line ASC \
                     LIMIT $4",
                    &[&repo, &language, &path_prefix, &limit, &or_query, &and_query, &raw_query],
                )
                .await?;
            Ok(rows
                .iter()
                .map(|row| {
                    let raw_score: f32 = row.get("raw_score");
                    let score = (raw_score / 2.0).clamp(0.01, 1.0);
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
                        score,
                    }
                })
                .collect())
        })
    }
}

fn prepare_code_search_query(query: &str) -> (Option<String>, Option<String>) {
    let mut or_terms: Vec<String> = Vec::new();
    let mut and_terms: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for raw_token in query.split_whitespace() {
        let cleaned: String = raw_token
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if cleaned.is_empty() {
            continue;
        }

        if seen.insert(cleaned.to_lowercase()) {
            and_terms.push(cleaned.clone());
            or_terms.push(cleaned.clone());
        }

        if cleaned.contains('_') {
            for part in cleaned.split('_') {
                if !part.is_empty() && seen.insert(part.to_lowercase()) {
                    or_terms.push(part.to_owned());
                }
            }
        }

        let mut camel_part = String::new();
        for ch in cleaned.chars() {
            if ch.is_uppercase() && !camel_part.is_empty() {
                if seen.insert(camel_part.to_lowercase()) {
                    or_terms.push(camel_part.clone());
                }
                camel_part.clear();
            }
            camel_part.push(ch);
        }
        if !camel_part.is_empty() && seen.insert(camel_part.to_lowercase()) {
            or_terms.push(camel_part);
        }
    }

    let or_query = if or_terms.is_empty() {
        None
    } else {
        Some(or_terms.join(" | "))
    };
    let and_query = if and_terms.is_empty() {
        None
    } else {
        Some(and_terms.join(" & "))
    };

    (or_query, and_query)
}

/// Node identity in the call graph: (path, name, line) — the same key the
/// frontend uses for `nodeKey`.
type SymbolKey = (String, String, i32);

fn symbol_ref_from_row(row: &tokio_postgres::Row) -> CodeSymbolRef {
    CodeSymbolRef {
        path: row.get("path"),
        name: row.get("name"),
        qualname: row.get("qualname"),
        kind: row.get("kind"),
        line: row.get("line"),
        end_line: row.get("end_line"),
        signature: row.get("signature"),
    }
}

/// Add a BFS neighbor unless already visited or the node cap is reached.
/// Returns the node's index when present after the call, `None` when the cap
/// blocked it (so callers can skip the edge too). New symbols join
/// `next_frontier` for the next level.
fn try_add_node(
    r: CodeSymbolRef,
    depth: usize,
    max_nodes: usize,
    nodes: &mut Vec<CodeGraphNode>,
    index_of: &mut HashMap<SymbolKey, usize>,
    next_frontier: &mut Vec<CodeSymbolRef>,
) -> Option<usize> {
    let key = (r.path.clone(), r.name.clone(), r.line);
    if let Some(&idx) = index_of.get(&key) {
        return Some(idx);
    }
    if nodes.len() >= max_nodes {
        return None;
    }
    let idx = nodes.len();
    nodes.push(CodeGraphNode { symbol: r, depth });
    index_of.insert(key, idx);
    next_frontier.push(nodes[idx].symbol.clone());
    Some(idx)
}

/// Record a caller -> callee edge once per symbol pair (earliest call line
/// wins, since expansion visits pairs in call order).
fn try_add_edge(
    from: &CodeSymbolRef,
    to: &CodeSymbolRef,
    call_line: i32,
    edges: &mut Vec<CodeGraphEdge>,
    seen: &mut HashSet<(SymbolKey, SymbolKey)>,
) {
    let key = (
        (from.path.clone(), from.name.clone(), from.line),
        (to.path.clone(), to.name.clone(), to.line),
    );
    if seen.insert(key) {
        edges.push(CodeGraphEdge {
            from: from.clone(),
            to: to.clone(),
            call_line,
        });
    }
}

/// Fetch `code_symbols` rows matching any of `names` (simple-name match) for
/// call-edge resolution. Empty input short-circuits to no rows.
async fn resolve_names(
    client: &tokio_postgres::Client,
    repo: &str,
    names: &[String],
) -> Result<Vec<tokio_postgres::Row>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    Ok(client
        .query(
            "SELECT path, name, qualname, kind, line, end_line, signature \
             FROM code_symbols WHERE repo = $1 AND name = ANY($2::text[])",
            &[&repo.to_owned(), &names.to_vec()],
        )
        .await?)
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
