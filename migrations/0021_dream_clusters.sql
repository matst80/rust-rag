-- Corpus-scale dreaming: deterministic clustering results.
--
-- A corpus round partitions a whole namespace into clusters with no model
-- call at all (TF-IDF over identifying tokens, hub filtering, mutual-kNN, a
-- cohesion gate). Cluster ids are a hash over the sorted member set, so the
-- same corpus always yields the same ids and the audit log stays readable
-- across runs.
--
-- `dream_actions` gains the columns needed to trace a decision back to the
-- cluster and prompt version that produced it. Previously a row recorded only
-- "something happened to this item", which made it impossible to tell a
-- rejected proposal from an applied one, or to re-derive decisions after a
-- prompt change.
CREATE TABLE IF NOT EXISTS dream_clusters (
    id TEXT PRIMARY KEY,
    round_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    member_count INTEGER NOT NULL DEFAULT 0,
    -- Highest-IDF identifying tokens, JSON array. The deterministic stand-in
    -- for an embedding: what this group is actually about.
    vocabulary TEXT NOT NULL DEFAULT '[]',
    -- Largest "best similarity to another member" seen while grouping. A low
    -- value means the group is a chain, not a topic.
    cohesion REAL NOT NULL DEFAULT 0,
    -- Entries covered by this cluster at discovery time.
    params TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_dream_clusters_round ON dream_clusters(round_id);
CREATE INDEX IF NOT EXISTS idx_dream_clusters_source ON dream_clusters(source_id, created_at DESC);

CREATE TABLE IF NOT EXISTS dream_cluster_members (
    cluster_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (cluster_id, item_id),
    FOREIGN KEY (cluster_id) REFERENCES dream_clusters(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dream_cluster_members_item ON dream_cluster_members(item_id);

ALTER TABLE dream_actions ADD COLUMN IF NOT EXISTS cluster_id TEXT;
ALTER TABLE dream_actions ADD COLUMN IF NOT EXISTS confidence REAL NOT NULL DEFAULT 0;
ALTER TABLE dream_actions ADD COLUMN IF NOT EXISTS prompt_version TEXT;
-- What the decision was based on, so a proposal can be audited without
-- re-running the round.
ALTER TABLE dream_actions ADD COLUMN IF NOT EXISTS evidence TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_dream_actions_round_id ON dream_actions(round_id);
CREATE INDEX IF NOT EXISTS idx_dream_actions_cluster ON dream_actions(cluster_id);