//! Row types for the code-repo ingestion tables (`code_repos`, `code_files`,
//! `code_chunks`).
//!
//! Persistence + query implementations land in `db/postgres.rs` and
//! `db/mod.rs` (sqlite fallback) in a later step. These structs are the
//! shared shape used by the ingest pipeline, MCP tools, and HTTP layer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRepo {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub enabled: bool,
    pub default_branch: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeFile {
    pub id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub path: String,
    pub basename: String,
    pub dir: String,
    pub extension: Option<String>,
    pub language: Option<String>,
    pub size_bytes: i64,
    pub line_count: i32,
    pub git_sha: Option<String>,
    pub git_branch: Option<String>,
    pub content_hash: String,
    pub mtime: Option<i64>,
    pub indexed_at: i64,
    pub summary: Option<String>,
    pub role: Option<String>,
    pub imports: Vec<String>,
    pub outline: Vec<OutlineEntry>,
    pub todos: Vec<TodoEntry>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineEntry {
    pub kind: String,
    pub name: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default)]
    pub is_public: bool,
    #[serde(default)]
    pub is_test: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoEntry {
    pub kind: String, // TODO|FIXME|HACK|XXX|NOTE
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunkRow {
    pub id: String,
    pub file_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub path: String,
    pub basename: String,
    pub language: Option<String>,
    pub ordinal: i32,
    pub start_line: i32,
    pub end_line: i32,
    pub byte_start: i64,
    pub byte_end: i64,
    pub symbol_kind: Option<String>,
    pub symbol_name: Option<String>,
    pub symbol_path: Option<String>,
    pub parent_symbol: Option<String>,
    pub visibility: Option<String>,
    pub doc_comment: Option<String>,
    pub signature: Option<String>,
    pub is_test: bool,
    pub is_public: bool,
    pub calls: Vec<String>,
    pub content: String,
    pub content_hash: String,
    pub token_count: Option<i32>,
    pub file_content_hash: String,
    pub git_sha: Option<String>,
    pub prev_chunk_id: Option<String>,
    pub next_chunk_id: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: String,
    pub embedding_version: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQuery {
    pub embedding: Vec<f32>,
    pub repo: Option<String>,
    pub language: Option<String>,
    pub path_prefix: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunkHit {
    pub chunk: CodeChunkRow,
    pub score: f32,
}
