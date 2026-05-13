-- Tighten ontology edge dedup: one edge per directed (from, to) pair,
-- regardless of relation. Without this, a 4B model hedging between
-- overlapping predicates (e.g. `part_of` AND `implemented_by` for the same
-- pair) lands two near-duplicate rows.
--
-- The reverse direction (B → A) is still allowed because that's a
-- genuinely different statement — e.g. `A supersedes B` and `B caused_by A`
-- can both be true.
--
-- Supersedes migration 0011's `(from, to, relation)` index, which is
-- strictly weaker than this one (any (from, to) conflict is also a
-- (from, to, relation) conflict). Drop the old index to keep the schema
-- clean.
--
-- Existing data may already contain (from, to) duplicates from before this
-- constraint existed (the 4B model's hedging output). Collapse them first:
-- keep the highest-weight row per pair (ties broken by relation alphabetical
-- for determinism), drop the rest. Without this DELETE, the CREATE UNIQUE
-- INDEX would fail.

DELETE FROM graph_edges
WHERE id IN (
    SELECT id FROM (
        SELECT id,
               ROW_NUMBER() OVER (
                   PARTITION BY from_item_id, to_item_id
                   ORDER BY weight DESC, relation ASC, created_at ASC
               ) AS rn
        FROM graph_edges
        WHERE edge_type = 'manual'
          AND metadata->>'source' = 'ontology_worker'
    ) ranked
    WHERE rn > 1
);

DROP INDEX IF EXISTS idx_graph_edges_ontology_pair;

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_ontology_directed_pair
    ON graph_edges (from_item_id, to_item_id)
    WHERE edge_type = 'manual' AND metadata->>'source' = 'ontology_worker';
