//! Source-code repo ingestion.
//!
//! Parses local source repos into `code_chunks` with metadata that supports
//! both AI-driven semantic search (BGE-Code-v1 embeddings) and traditional
//! navigation (jump-to-def, file outline, callers).
//!
//! Layered:
//! - `lang`     — file-extension → language detection.
//! - `parser`   — tree-sitter `Language` + symbol queries per language.
//! - `chunker`  — `analyze_file` produces chunks + file-level analysis.
//! - `watcher`  — (later) file-watcher worker that drives ingestion.

pub mod chunker;
pub mod ingest;
pub mod lang;
pub mod parser;

pub use chunker::{CodeChunk, FileAnalysis, analyze_file, detect_role};
pub use ingest::{ingest_repo, IngestOptions, IngestReport};
pub use lang::{Lang, detect_lang};
