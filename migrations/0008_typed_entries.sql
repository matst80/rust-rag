-- Typed structured data on entries. Adds `type` + `data` to documents and
-- a `schemas` table holding JSON Schema definitions per type. Mirrors the
-- sqlite columns added in src/db/schema.rs (`items.type`, `items.data`,
-- `schemas`).
ALTER TABLE documents ADD COLUMN IF NOT EXISTS type TEXT;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS data JSONB;
CREATE INDEX IF NOT EXISTS idx_documents_type ON documents(type);

CREATE TABLE IF NOT EXISTS schemas (
    type_name TEXT PRIMARY KEY,
    json_schema JSONB NOT NULL,
    title TEXT,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
