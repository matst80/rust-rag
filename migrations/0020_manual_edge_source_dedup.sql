-- Second, additive dedup lane for manual edges: idempotent upsert keyed on
-- (from, to, relation, source) for any caller-tagged source OTHER than
-- `ontology_worker` (which keeps its own DO-NOTHING lane from migration
-- 0012, preserving HITL approval/rejection state on first write).
--
-- This is for retry-safe linking — e.g. replaying a projection/event
-- stream — where a repeated call with the same (from, to, relation, source)
-- should converge to the latest metadata/weight rather than duplicate the
-- row. Edges with no `metadata.source` tag (plain curator-created edges)
-- are not covered and continue to insert fresh, same as before.

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_source_dedup
    ON graph_edges (from_item_id, to_item_id, relation, (metadata->>'source'))
    WHERE edge_type = 'manual'
      AND metadata->>'source' IS NOT NULL
      AND metadata->>'source' != 'ontology_worker';
