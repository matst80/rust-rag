-- Prevent the ontology worker from inserting duplicate rows for the same
-- (from, to, relation) tuple. The edge id includes a millisecond timestamp,
-- so without this index every re-run of the worker against the same item
-- created a new row instead of a no-op.
--
-- Scope:
--   - Only manual edges from `metadata.source = 'ontology_worker'`.
--   - Human-created manual edges (curator-added via /admin/graph/edges)
--     are NOT covered — they don't carry the source tag and can intentionally
--     duplicate the worker's verdicts when overriding them.
--   - Similarity edges already have their own unique index from 0002.
--
-- `relation` is part of the key so two different relations between the same
-- pair (e.g. `is_a` AND `depends_on`) can coexist. NULL relations are
-- treated as not-equal-to-NULL by Postgres, so they don't collide — but in
-- practice the worker always emits a non-null relation from the schema.

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_ontology_pair
    ON graph_edges (from_item_id, to_item_id, relation)
    WHERE edge_type = 'manual' AND metadata->>'source' = 'ontology_worker';
