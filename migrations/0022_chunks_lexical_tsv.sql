-- Migration 0022: Lexical (full-text) leg for hybrid search over memory chunks.
--
-- Adds a generated `tsvector` column on `chunks` plus a GIN index so
-- `search_hybrid` can fuse a keyword leg (ts_rank_cd over plainto_tsquery)
-- with the dense + sparse RRF scores. Uses the `simple` dictionary on
-- purpose: the corpus mixes languages (English/Swedish) and `simple` only
-- lowercases + tokenizes — no English stop-word removal or stemming that
-- would silently drop query terms in another language. Dense embeddings
-- already cover the semantic side; this leg is the exact-keyword signal
-- (e.g. "enduro") that cosine undervalues on short notes.
--
-- STORED generated column backfills existing rows on ALTER; the GIN index
-- build is the only heavy step (fine at current corpus size).

ALTER TABLE chunks ADD COLUMN IF NOT EXISTS tsv tsvector
    GENERATED ALWAYS AS (to_tsvector('simple', coalesce(content, ''))) STORED;

CREATE INDEX IF NOT EXISTS idx_chunks_tsv ON chunks USING gin(tsv);
