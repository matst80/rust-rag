-- Phase 1 cutover: auxiliary tables previously kept in SQLite. Schema mirrors
-- the SQLite shape 1:1 so the migration is straight INSERT...SELECT and the
-- trait impls stay simple. Timestamps stored as BIGINT (ms since epoch) to
-- match the existing trait API (`now: i64`) without conversion overhead.
--
-- Embeddings carried in `user_events` / `user_profiles` are dimensioned for
-- bge-m3 (1024). Old 384-d query embeddings from the legacy stack are
-- discarded on migration — the system rebuilds them from fresh search events
-- (see PROFILE_REFRESH_AFTER in src/db/mod.rs).

CREATE TABLE mcp_tokens (
    id           TEXT PRIMARY KEY,
    token_hash   TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    subject      TEXT,
    created_at   BIGINT NOT NULL,
    last_used_at BIGINT,
    expires_at   BIGINT
);

CREATE INDEX idx_mcp_tokens_subject ON mcp_tokens (subject);

CREATE TABLE device_auth_requests (
    device_code     TEXT PRIMARY KEY,
    user_code       TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'denied', 'expired')),
    token_id        TEXT REFERENCES mcp_tokens(id) ON DELETE SET NULL,
    subject         TEXT,
    client_name     TEXT,
    created_at      BIGINT NOT NULL,
    expires_at      BIGINT NOT NULL,
    interval_secs   BIGINT NOT NULL DEFAULT 5,
    last_polled_at  BIGINT
);

CREATE INDEX idx_device_auth_status     ON device_auth_requests (status);
CREATE INDEX idx_device_auth_expires_at ON device_auth_requests (expires_at);

CREATE TABLE user_events (
    id               TEXT PRIMARY KEY,
    subject          TEXT NOT NULL,
    event_type       TEXT NOT NULL CHECK (event_type IN ('search', 'view', 'store', 'chat')),
    query            TEXT,
    query_embedding  vector(1024),
    item_ids         JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at       BIGINT NOT NULL
);

CREATE INDEX idx_user_events_subject ON user_events (subject, created_at DESC);

CREATE TABLE user_profiles (
    subject             TEXT PRIMARY KEY,
    interest_embedding  vector(1024),
    event_horizon       BIGINT NOT NULL DEFAULT 0,
    updated_at          BIGINT NOT NULL
);

CREATE TABLE messages (
    id            TEXT PRIMARY KEY,
    channel       TEXT NOT NULL,
    sender        TEXT NOT NULL,
    sender_kind   TEXT NOT NULL DEFAULT 'human' CHECK (sender_kind IN ('human', 'agent', 'system')),
    text          TEXT NOT NULL,
    kind          TEXT NOT NULL DEFAULT 'text',
    metadata      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX idx_messages_channel_created ON messages (channel, created_at DESC);
CREATE INDEX idx_messages_sender_created  ON messages (sender, created_at DESC);
CREATE INDEX idx_messages_created_at      ON messages (created_at DESC);
CREATE INDEX idx_messages_kind            ON messages (kind);
CREATE INDEX idx_messages_updated_at      ON messages (updated_at DESC);
-- Permission_request lookup hits metadata->>'request_id'.
CREATE INDEX idx_messages_request_id      ON messages ((metadata->>'request_id'))
    WHERE kind = 'permission_request';

-- `manager_memory` table intentionally NOT ported. It existed as a separate
-- store in an earlier design but was replaced by storing manager notes as
-- regular `items`/`documents` rows with `source_id='manager_memory'`. No
-- live code references the SQLite table — the 7 rows in the prod snapshot
-- are legacy. The frontend (frontend/lib/api/client.ts) and manager skill
-- (.claude/skills/manager-agent/SKILL.md) both go through the items API
-- with that source_id.

-- graph_edges references documents (the new parent rows from 0001).
-- edge_type matches the SQLite shape; manual edges carry a relation string,
-- similarity edges fill it from the rebuild pass.
CREATE TABLE graph_edges (
    id             TEXT PRIMARY KEY,
    from_item_id   TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    to_item_id     TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    edge_type      TEXT NOT NULL CHECK (edge_type IN ('similarity', 'manual')),
    relation       TEXT,
    weight         REAL NOT NULL,
    directed       BOOLEAN NOT NULL DEFAULT FALSE,
    metadata       JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at     BIGINT NOT NULL,
    updated_at     BIGINT NOT NULL
);

CREATE INDEX idx_graph_edges_from ON graph_edges (from_item_id);
CREATE INDEX idx_graph_edges_to   ON graph_edges (to_item_id);
CREATE INDEX idx_graph_edges_type ON graph_edges (edge_type);
CREATE UNIQUE INDEX idx_graph_edges_similarity_pair
    ON graph_edges (from_item_id, to_item_id, edge_type)
    WHERE edge_type = 'similarity';
