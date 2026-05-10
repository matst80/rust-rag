-- Wiki-style hierarchical path on documents (e.g. 'engineering/runbooks/db').
-- Distinct from chunk-level `section_path` (chunker-derived from markdown headers).
-- User-asserted, normalized: no leading/trailing '/', no '..', empty -> NULL.
ALTER TABLE documents ADD COLUMN IF NOT EXISTS path TEXT;
CREATE INDEX IF NOT EXISTS idx_documents_path ON documents(path);
