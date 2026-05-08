-- Phase 1 schema: parent/child documents + chunks, dense embeddings only.
-- Sparse (sparsevec) and HNSW indexes are added in phase 2 once data is loaded.

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE documents (
    id           TEXT PRIMARY KEY,
    source_id    TEXT NOT NULL,
    kind         TEXT NOT NULL DEFAULT 'text',
    author       TEXT,
    content      TEXT NOT NULL,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    tags         TEXT[] NOT NULL DEFAULT '{}',
    status       TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_documents_source ON documents (source_id, updated_at DESC);
CREATE INDEX idx_documents_tags   ON documents USING GIN (tags);
CREATE INDEX idx_documents_meta   ON documents USING GIN (metadata);

CREATE TABLE chunks (
    id                 BIGSERIAL PRIMARY KEY,
    document_id        TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    position           INT NOT NULL,
    -- Header path for parent-section reassembly at retrieval time
    -- (e.g. ARRAY['Architecture', 'Embedding execution']).
    section_path       TEXT[],
    content            TEXT NOT NULL,
    token_count        INT,
    dense_embedding    vector(1024),
    embedding_model    TEXT NOT NULL,
    embedding_version  INT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_chunks_doc ON chunks (document_id);
CREATE UNIQUE INDEX idx_chunks_doc_position ON chunks (document_id, position);
