-- Files bound to documents. Disk storage lives under RAG_UPLOAD_PATH;
-- `stored_name` is the on-disk filename (UUID + extension). Cascade on
-- document delete; the API layer is responsible for unlinking the file
-- after the row is gone.
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    filename TEXT,
    stored_name TEXT NOT NULL,
    mime TEXT,
    size BIGINT,
    sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_attachments_document_id ON attachments(document_id);
CREATE INDEX IF NOT EXISTS idx_attachments_created_at ON attachments(created_at DESC);
