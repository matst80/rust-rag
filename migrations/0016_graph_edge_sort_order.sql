ALTER TABLE graph_edges
    ADD COLUMN IF NOT EXISTS sort_order TEXT NOT NULL DEFAULT '00000000000000000000';

UPDATE graph_edges
SET sort_order = LPAD(COALESCE(updated_at, created_at, 0)::TEXT, 20, '0')
WHERE sort_order IS NULL OR BTRIM(sort_order) = '';

CREATE INDEX IF NOT EXISTS idx_graph_edges_sort_order
    ON graph_edges (sort_order ASC, id ASC);
