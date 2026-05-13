-- Ontology worker queue: track which documents still need ontology extraction.
--
-- Status lifecycle:
--   pending  → row queued for the worker (default for new + existing rows)
--   done     → worker processed it, regardless of whether edges were committed
--   failed   → LLM call or schema parse failed; manual re-queue via UPDATE
--
-- The worker's `get_items_pending_ontology(limit)` selects from this column.
-- A partial index keeps the queue scan O(pending) instead of O(documents).

ALTER TABLE documents
    ADD COLUMN IF NOT EXISTS ontology_status TEXT NOT NULL DEFAULT 'pending';

-- Partial index — only pending rows are interesting for the worker scan.
CREATE INDEX IF NOT EXISTS idx_documents_ontology_pending
    ON documents (created_at ASC)
    WHERE ontology_status = 'pending';
