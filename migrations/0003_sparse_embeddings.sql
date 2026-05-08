-- Phase 2: bge-m3 sparse vector storage. Nullable so existing rows stay
-- valid; the column is populated for newly written chunks once the sparse
-- encoder lands. HNSW indexing is deferred — sequential scan is faster
-- than HNSW until the corpus grows past a few thousand chunks.

ALTER TABLE chunks
    ADD COLUMN IF NOT EXISTS sparse_embedding sparsevec(250002);
