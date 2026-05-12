-- LLM-on-store analysis columns persisted per document. Mirrors the sqlite
-- columns added by `ensure_column_exists` in src/db/schema.rs. `analysis_json`
-- holds the serialized `StoreAnalysis` payload; `analysis_at` is the wall-clock
-- timestamp the analysis ran; `analysis_model` is the model id used.
ALTER TABLE documents ADD COLUMN IF NOT EXISTS analysis_json JSONB;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS analysis_at TIMESTAMPTZ;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS analysis_model TEXT;
