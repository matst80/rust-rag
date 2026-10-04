-- Migration 0019: Code search without vector data (BM25 / full-text ranking).
--
-- Drop NOT NULL constraint on dense_embedding since snapshots uploaded from
-- external indexers now omit vector data.
-- Add weighted full-text search (tsvector) and indexes for fast ranking.

ALTER TABLE code_symbols ALTER COLUMN dense_embedding DROP NOT NULL;

-- Trigram extension for fuzzy & substring matching on symbol names
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX IF NOT EXISTS idx_code_symbols_name_trgm
    ON code_symbols USING gin (name gin_trgm_ops);

-- Generated weighted tsvector column for BM25 / cover-density ranking:
-- Weight A: symbol name and qualname (with snake_case and :: split)
-- Weight B: signature
-- Weight C: doc comments
-- Weight D: symbol source text (capped at 50,000 chars)
ALTER TABLE code_symbols ADD COLUMN IF NOT EXISTS tsv tsvector
    GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', coalesce(name, '') || ' ' || replace(replace(coalesce(name, ''), '_', ' '), '::', ' ')), 'A') ||
        setweight(to_tsvector('simple', coalesce(qualname, '') || ' ' || replace(replace(coalesce(qualname, ''), '_', ' '), '::', ' ')), 'A') ||
        setweight(to_tsvector('simple', replace(coalesce(signature, ''), '_', ' ')), 'B') ||
        setweight(to_tsvector('simple', coalesce(doc, '')), 'C') ||
        setweight(to_tsvector('simple', substr(coalesce(text, ''), 1, 50000)), 'D')
    ) STORED;

CREATE INDEX IF NOT EXISTS idx_code_symbols_tsv ON code_symbols USING gin(tsv);
