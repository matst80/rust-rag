-- Code-repo ingestion: separate store for source-code chunks embedded with
-- BGE-Code-v1 (1536-d). Kept distinct from `documents`/`chunks` because:
--   * different lifecycle (file-watcher driven, content-hash dedup)
--   * different embedding model + dim → separate HNSW index
--   * different metadata shape (symbols, line ranges, git_sha)
-- Cross-store edges live in `graph_edges` (made polymorphic in 0016).
--
-- Denormalized columns on `code_chunks` (repo_name, path, basename, language)
-- avoid a join on every search/display.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE code_repos (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    root_path       TEXT NOT NULL,
    include_globs   JSONB NOT NULL DEFAULT '[]'::jsonb,
    exclude_globs   JSONB NOT NULL DEFAULT '[]'::jsonb,
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    default_branch  TEXT,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL
);

CREATE INDEX idx_code_repos_enabled ON code_repos (enabled) WHERE enabled = TRUE;

CREATE TABLE code_files (
    id              TEXT PRIMARY KEY,
    repo_id         TEXT NOT NULL REFERENCES code_repos(id) ON DELETE CASCADE,
    repo_name       TEXT NOT NULL,
    path            TEXT NOT NULL,           -- relative to repo root
    basename        TEXT NOT NULL,
    dir             TEXT NOT NULL DEFAULT '',
    extension       TEXT,
    language        TEXT,
    size_bytes      BIGINT NOT NULL DEFAULT 0,
    line_count      INT NOT NULL DEFAULT 0,
    git_sha         TEXT,
    git_branch      TEXT,
    content_hash    TEXT NOT NULL,           -- sha256 of file content
    mtime           BIGINT,                  -- ms since epoch
    indexed_at      BIGINT NOT NULL,
    -- developer-search aids (extracted at ingest time, kept on the file row
    -- so "what's in this file?" answers in a single lookup):
    summary         TEXT,                    -- top-of-file doc comment / module doc
    role            TEXT,                    -- test|lib|bin|example|build|config|doc|script
    imports         JSONB NOT NULL DEFAULT '[]'::jsonb,  -- ["module::path", ...]
    outline         JSONB NOT NULL DEFAULT '[]'::jsonb,  -- [{kind,name,line,signature?}]
    todos           JSONB NOT NULL DEFAULT '[]'::jsonb,  -- [{kind:"TODO"|"FIXME"|...,line,text}]
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL
);

CREATE UNIQUE INDEX idx_code_files_repo_path ON code_files (repo_id, path);
CREATE INDEX idx_code_files_basename         ON code_files (basename);
CREATE INDEX idx_code_files_language         ON code_files (language);
CREATE INDEX idx_code_files_repo_dir         ON code_files (repo_id, dir);

CREATE TABLE code_chunks (
    id                 TEXT PRIMARY KEY,
    file_id            TEXT NOT NULL REFERENCES code_files(id) ON DELETE CASCADE,
    repo_id            TEXT NOT NULL REFERENCES code_repos(id) ON DELETE CASCADE,
    -- denormalized for fast filter/display without join:
    repo_name          TEXT NOT NULL,
    path               TEXT NOT NULL,
    basename           TEXT NOT NULL,
    language           TEXT,
    -- location:
    ordinal            INT NOT NULL,         -- chunk order within file
    start_line         INT NOT NULL,
    end_line           INT NOT NULL,
    byte_start         BIGINT NOT NULL DEFAULT 0,
    byte_end           BIGINT NOT NULL DEFAULT 0,
    -- symbol info:
    symbol_kind        TEXT,                 -- function|method|struct|impl|class|module|fallback
    symbol_name        TEXT,
    symbol_path        TEXT,                 -- e.g. mod::Type::method
    parent_symbol      TEXT,
    visibility         TEXT,                 -- pub|priv|pub(crate)
    doc_comment        TEXT,
    -- developer-search aids:
    signature          TEXT,                 -- single-line declaration (fn foo(...) -> Bar)
    is_test            BOOLEAN NOT NULL DEFAULT FALSE,
    is_public          BOOLEAN NOT NULL DEFAULT FALSE,
    calls              JSONB NOT NULL DEFAULT '[]'::jsonb,  -- referenced symbol names
    -- content + dedup:
    content            TEXT NOT NULL,
    content_hash       TEXT NOT NULL,
    token_count        INT,
    -- file-level snapshot at index time:
    file_content_hash  TEXT NOT NULL,
    git_sha            TEXT,
    -- chain (for context expansion):
    prev_chunk_id      TEXT,
    next_chunk_id      TEXT,
    -- embedding:
    embedding          vector(1536),
    embedding_model    TEXT NOT NULL,
    embedding_version  INT NOT NULL,
    created_at         BIGINT NOT NULL,
    updated_at         BIGINT NOT NULL
);

CREATE UNIQUE INDEX idx_code_chunks_file_ordinal ON code_chunks (file_id, ordinal);
CREATE INDEX idx_code_chunks_repo_path           ON code_chunks (repo_id, path, start_line);
CREATE INDEX idx_code_chunks_basename            ON code_chunks (basename);
CREATE INDEX idx_code_chunks_path_trgm           ON code_chunks USING gin (path gin_trgm_ops);
CREATE INDEX idx_code_chunks_symbol              ON code_chunks (repo_id, symbol_name) WHERE symbol_name IS NOT NULL;
CREATE INDEX idx_code_chunks_symbol_path_trgm    ON code_chunks USING gin (symbol_path gin_trgm_ops) WHERE symbol_path IS NOT NULL;
CREATE INDEX idx_code_chunks_language            ON code_chunks (language);
CREATE INDEX idx_code_chunks_is_test             ON code_chunks (is_test) WHERE is_test = TRUE;
CREATE INDEX idx_code_chunks_is_public           ON code_chunks (is_public) WHERE is_public = TRUE;
CREATE INDEX idx_code_chunks_calls_gin           ON code_chunks USING gin (calls jsonb_path_ops);
CREATE INDEX idx_code_chunks_content_trgm        ON code_chunks USING gin (content gin_trgm_ops);
CREATE INDEX idx_code_chunks_embedding           ON code_chunks USING hnsw (embedding vector_cosine_ops);

CREATE INDEX idx_code_files_role                 ON code_files (role);
CREATE INDEX idx_code_files_imports_gin          ON code_files USING gin (imports jsonb_path_ops);
