-- Dreaming worker: durable cursor + audit log.
--
-- `documents.dreamed_at` is NULL until the dreaming worker reaches a terminal
-- decision (promote / merge / prune / keep) about a row. The round query asks
-- for `dreamed_at IS NULL`, so a processed item is never re-picked and `keep`
-- advances the cursor instead of spinning forever.
--
-- `dream_actions` is the audit record: every action the worker actually applied
-- (and every dry-run proposal) with reason, item, outcome and timestamp.
ALTER TABLE documents ADD COLUMN IF NOT EXISTS dreamed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_documents_dreamed_at
    ON documents(source_id, dreamed_at, created_at);

CREATE TABLE IF NOT EXISTS dream_actions (
    id TEXT PRIMARY KEY,
    round_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_id TEXT,
    result_item_id TEXT,
    reason TEXT NOT NULL DEFAULT '',
    outcome TEXT NOT NULL,
    dry_run BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_dream_actions_created_at ON dream_actions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_dream_actions_item_id ON dream_actions(item_id);