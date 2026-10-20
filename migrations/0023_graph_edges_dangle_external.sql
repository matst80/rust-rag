-- External edge endpoints (`ext:{kind}:{key}`) no longer need placeholder
-- rows: graph_edges drops its foreign keys so those endpoints dangle by
-- design and resolve synthetically at read time. Existence of non-external
-- endpoints is enforced by `add_manual_edge`, and `delete_item` removes
-- incident edges explicitly (replacing ON DELETE CASCADE).
ALTER TABLE graph_edges DROP CONSTRAINT IF EXISTS graph_edges_from_item_id_fkey;
ALTER TABLE graph_edges DROP CONSTRAINT IF EXISTS graph_edges_to_item_id_fkey;

-- Remove the now-unneeded empty placeholder entries the old constraints
-- forced into existence. Edges pointing at them are kept and dangle.
-- Only empty-content rows are touched — never real content.
DELETE FROM documents
WHERE content = ''
  AND (id LIKE 'ext:%'
       OR source_id = 'external'
       OR COALESCE((metadata->>'external')::boolean, FALSE) = TRUE);
