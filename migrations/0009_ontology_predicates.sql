CREATE TABLE IF NOT EXISTS ontology_predicates (
    name TEXT NOT NULL,
    source_id TEXT NOT NULL DEFAULT '*',
    description TEXT NOT NULL,
    direction TEXT NOT NULL,
    example_from TEXT,
    example_to TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (name, source_id)
);
