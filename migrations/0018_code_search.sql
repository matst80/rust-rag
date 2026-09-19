-- Code-search domain: per-repo snapshots merged in from the external
-- `.codesearch/codesearch.db` sqlite+sqlite-vec sidecar (symbols + 384-dim
-- all-MiniLM-L6-v2 float32 embeddings). Postgres becomes the shared,
-- cross-repo search index; the sqlite files stay per-repo scratch.
--
-- An earlier, never-shipped code-ingest experiment left code_* tables of a
-- different shape (text-UUID repo ids, code_chunks, git metadata). Nothing
-- in any build reads them — drop so this schema applies cleanly on
-- databases the experiment touched. Fresh databases: all no-ops.

DROP TABLE IF EXISTS code_chunks CASCADE;
DROP TABLE IF EXISTS code_files CASCADE;
DROP TABLE IF EXISTS code_symbols CASCADE;
DROP TABLE IF EXISTS code_calls CASCADE;
DROP TABLE IF EXISTS code_repos CASCADE;

CREATE TABLE code_repos (
    name TEXT PRIMARY KEY,
    root_path TEXT NOT NULL DEFAULT '',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    -- version of the external indexer that produced the snapshot
    -- (embeddings_info.CREATE_VERSION)
    source_version TEXT NOT NULL DEFAULT '',
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE code_files (
    repo TEXT NOT NULL REFERENCES code_repos(name) ON DELETE CASCADE,
    path TEXT NOT NULL,
    basename TEXT NOT NULL DEFAULT '',
    language TEXT,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    mtime BIGINT NOT NULL DEFAULT 0,
    line_count INTEGER NOT NULL DEFAULT 0,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (repo, path)
);

CREATE TABLE code_symbols (
    id BIGSERIAL PRIMARY KEY,
    repo TEXT NOT NULL REFERENCES code_repos(name) ON DELETE CASCADE,
    path TEXT NOT NULL,
    line INTEGER NOT NULL DEFAULT 0,
    end_line INTEGER NOT NULL DEFAULT 0,
    kind TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL DEFAULT '',
    qualname TEXT NOT NULL DEFAULT '',
    signature TEXT NOT NULL DEFAULT '',
    doc TEXT NOT NULL DEFAULT '',
    lang TEXT NOT NULL DEFAULT '',
    text TEXT NOT NULL DEFAULT '',
    visibility TEXT NOT NULL DEFAULT '',
    exemplar TEXT NOT NULL DEFAULT '',
    hotpath TEXT NOT NULL DEFAULT '',
    -- all-MiniLM-L6-v2, 384 dims, cosine-normalized float32
    dense_embedding vector(384) NOT NULL,
    embedding_model TEXT NOT NULL DEFAULT 'all-MiniLM-L6-v2',
    embedding_version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_code_symbols_repo_path ON code_symbols(repo, path);
CREATE INDEX idx_code_symbols_name ON code_symbols(name);
CREATE INDEX idx_code_symbols_embedding
    ON code_symbols USING hnsw (dense_embedding vector_cosine_ops);

CREATE TABLE code_calls (
    id BIGSERIAL PRIMARY KEY,
    repo TEXT NOT NULL REFERENCES code_repos(name) ON DELETE CASCADE,
    caller_path TEXT NOT NULL,
    caller_symbol TEXT NOT NULL,
    caller_line INTEGER NOT NULL DEFAULT 0,
    callee_name TEXT NOT NULL,
    callee_operand TEXT NOT NULL DEFAULT '',
    call_line INTEGER NOT NULL DEFAULT 0,
    call_column INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_code_calls_callee ON code_calls(repo, callee_name);
CREATE INDEX idx_code_calls_caller ON code_calls(repo, caller_path);
