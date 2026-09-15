-- Public shares for documents/items.
-- Allows read-only unauthenticated access via an unguessable token.
CREATE TABLE IF NOT EXISTS public_shares (
    token TEXT PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_public_shares_item_id ON public_shares(item_id);
CREATE INDEX IF NOT EXISTS idx_public_shares_expires_at ON public_shares(expires_at);
