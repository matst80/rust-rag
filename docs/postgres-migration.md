# Postgres + bge-m3 Migration Plan

Status: **phase 1 dense-only stack landed locally, including chunking**. `make run-pg` boots the server with bge-m3 (CLS-pooled, 1024-d, ~500-token markdown-aware chunks) routed at the new `PostgresVectorStore`; the migrated 55 docs (66 chunks — 7 split across 2-5 chunks, 48 single-chunk) are searchable end-to-end via the frontend (`make frontend-dev`). Auth / messages / user_memory still go through SQLite — those tables aren't ported yet. Eval harness is the next required step before this can replace the SQLite/bge-small backend in prod.

Target Postgres: `10.10.10.207`, Postgres 18.3, pgvector 0.8.2 installed (verified `vector` + `sparsevec` types working).

## Why migrate

- bge-small (384d) loses recall on exact keywords/IDs; bge-m3 (1024d dense + sparse + colbert-style multi-vec) materially improves both keyword and semantic retrieval.
- bge-m3 produces a sparse output natively — pgvector's `sparsevec` is the right home for it; SQLite would need BLOB hacks.
- Concurrency: SQLite locks during writes; this RAG store is now used as cross-agent memory and will see simultaneous read/write.
- Hosted Postgres frees us from backups/PITR/connection pooling concerns.

## Constraints

- **VRAM**: 6 GB GPU. bge-m3 fp16 ~2.3 GB resident; embedding batches must stay small (1–4 chunks) to avoid OOM.
- **Hosting**: Postgres 18.3 already running on `10.10.10.207`, pgvector 0.8.2 installed. `mats` is a regular role (no superuser).
- **Backend topology**: Rust API runs in-cluster as the CUDA Deployment on node `midi` (`rag-service-cuda`); SQLite lives on PVC `rust-rag-cuda-data`. Postgres at `10.10.10.207` is reachable from the cluster.

## Decomposition (do not bundle)

These are four independent migrations stacked on top of each other. Ship them in order; each must beat the previous on the eval harness or it doesn't merge.

1. **SQLite → Postgres + pgvector**, dense-only, bge-m3 (1024d).
2. **Add sparse** (`sparsevec(250002)`) and hybrid scoring (RRF).
3. **Add cross-encoder reranker** (`bge-reranker-v2-m3`) on top-50 candidates.
4. **(Maybe never) Add colbert** late-interaction. Skipped unless reranker is empirically insufficient and storage budget is solved.

## Architectural decisions (decide before phase 1)

### Embedding execution

- **Option A**: ONNX-in-Rust (current pattern). Requires exporting bge-m3 to ONNX with dense (and later sparse) heads. Sparse export is feasible; colbert per-token export is awkward.
- **Option B**: Python sidecar (FastAPI + FlagEmbedding) on the GPU host. Faster to build, simplest sparse + colbert support, but adds a deploy unit and a network hop on every ingest.

Recommendation: start with Option A for dense-only (small change). Reassess at phase 2 — if sparse export proves painful, switch to Option B.

### Document model: parent/child

Move from flat `items` to `documents` + `chunks`:

- Index *small* chunks (~500 tokens, ~50 overlap) for precise retrieval.
- Return *parent* document/section to the LLM for context.
- Biggest single quality lever, independent of model choice.

### Chunking

- Token-aware (bge-m3 tokenizer, XLM-RoBERTa). No char-based splits.
- Structure-aware: respect markdown headers, code fences, paragraphs.
- **Default ~500 tokens per child chunk, ~50 overlap.** bge-m3 supports 8k tokens per embedding, but the parent/child architecture deliberately uses small index chunks (precision) and expands to the parent section at LLM context time (recall). The 8k headroom is safety margin, not the target. 1024 is a defensible alternative — pick by eval if recall on long-doc queries regresses at 500.
- Use the `text-splitter` Rust crate (`MarkdownSplitter` with the bge-m3 tokenizer as sizer).

### Retrieval shape

Storing `documents` + `chunks` is half the decision; the other half is what retrieval actually returns. Pin these down before phase 1 — they shape the SQL and the API.

- **Chunk-hit aggregation**: merge ranks per document with RRF *before* the cross-retriever RRF (document score = RRF over its chunk ranks within each retriever, then RRF across dense + sparse). Avoids one large doc with several mediocre chunks collapsing to only its single best chunk, while still rewarding scattered relevance.
- **Per-document cap into the reranker**: keep at most N chunks per document (start N=3) in the top-50 candidate pool. Cheap `ROW_NUMBER() OVER (PARTITION BY document_id ORDER BY rank)` filter. Prevents one chatty document monopolizing the reranker budget.
- **Context returned to the LLM**: return the *parent section* (header-bounded), not the raw 500-token chunk and not the whole document. Requires `section_path TEXT[]` (or `parent_chunk_id`) on `chunks` so section reassembly is a single indexed lookup. Whole-document return defeats the point of small-chunk indexing for any doc over ~2k tokens.
- **Response shape — LLM vs UI**: the search API should return enough for both consumers without a second round-trip. Per hit: `document_id`, `chunk_id`, `score`, `matched_chunk` (raw, for highlighting), `parent_section` (for the LLM), and document metadata (`title`, `source_id`, `updated_at`). The frontend then fetches the full document on click via `GET /documents/:id` — humans want the whole thing with the matched chunk anchored/highlighted, not just the section the LLM saw.
- **Deferred — document-level summary chunk** (phase 2 experiment): one extra chunk per document at `position = -1` containing title + summary / first-N tokens, so a query can match the doc as a whole when no single 500-token chunk is a clean hit. Revisit once eval shows where large-doc recall actually fails.

### Hybrid scoring

- **Reciprocal Rank Fusion (RRF)**, parameter-free. No weighted-sum tuning that drifts per corpus.
- `score = sum(1 / (60 + rank_i))` across dense + sparse rankings.
- Pull top-50 to feed reranker (phase 3) — design `retrieve(50) → rerank → top_k` from day one.

### Recency / decay

This RAG store is durable cross-session memory (per `CLAUDE.md`). Pure cosine ignores time.

- Add a recency boost: `final_score = rrf_score * exp(-age_days / half_life)`.
- Half-life tunable per `kind` (todos decay fast, knowledge slowly).

### Embedding versioning

First-class columns on every chunk row:

```sql
embedding_model     TEXT NOT NULL,   -- 'bge-m3'
embedding_version   INT  NOT NULL,   -- 1
```

Enables incremental re-embedding, side-by-side A/B, and stale detection after model upgrades. Cheap now, painful later.

### Schema sketch

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE documents (
  id           TEXT PRIMARY KEY,
  source_id    TEXT NOT NULL,           -- e.g. 'project:rust-rag:knowledge'
  kind         TEXT NOT NULL DEFAULT 'text',
  author       TEXT,
  content      TEXT NOT NULL,
  metadata     JSONB NOT NULL DEFAULT '{}',
  tags         TEXT[] NOT NULL DEFAULT '{}',
  status       TEXT,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_documents_source ON documents(source_id, updated_at DESC);
CREATE INDEX idx_documents_tags   ON documents USING GIN (tags);
CREATE INDEX idx_documents_meta   ON documents USING GIN (metadata);

CREATE TABLE chunks (
  id                 BIGSERIAL PRIMARY KEY,
  document_id        TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  position           INT  NOT NULL,
  content            TEXT NOT NULL,
  token_count        INT,
  dense_embedding    vector(1024),
  sparse_embedding   sparsevec(250002),       -- phase 2
  embedding_model    TEXT NOT NULL,
  embedding_version  INT  NOT NULL,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_chunks_doc ON chunks(document_id);
-- HNSW index added once data is loaded; defaults are fine for <1M rows.
CREATE INDEX idx_chunks_dense_hnsw  ON chunks USING hnsw (dense_embedding vector_cosine_ops);
-- Phase 2:
-- CREATE INDEX idx_chunks_sparse_hnsw ON chunks USING hnsw (sparse_embedding sparsevec_cosine_ops);
```

Plus tables for `messages`, manual edges (graph), and any auxiliary state currently in SQLite.

### Operational

- **Migrations**: tiny embedded runner in `src/db/postgres.rs` (reads `migrations/*.sql`, tracks applied set in `schema_migrations`). **Not sqlx** — sqlx 0.8 has an optional `sqlx-sqlite` dep that pins `libsqlite3-sys 0.30.1`, which Cargo's `links = "sqlite3"` rule rejects against rusqlite's `0.37.0` even when the feature isn't activated. `tokio-postgres` has no such transitive sqlite drag.
- **Pool**: `deadpool-postgres`, max ~10–20.
- **Connection string**: env-driven (`RAG_DATABASE_URL`). Keep SQLite path as fallback during cutover.
- **Backups**: confirm hosted provider takes them, otherwise `pg_dump` cron.

### Local dev on macOS (Apple Silicon)

The whole stack runs locally on the M-series dev box for iteration; production runs in-cluster (`rag-service-cuda` on node `midi`) and connects to Postgres at `10.10.10.207`. Concrete commands (all wired in the Makefile):

```sh
make export-bge-m3        # one-time: BAAI/bge-m3 → assets/bge-m3/ via optimum-cli
make fetch-prod-snapshot  # kubectl-cp /app/data/rag.db out of the cuda pod into /tmp/rust-rag-prod-snapshot/
make migrate-prod         # re-embed snapshot with bge-m3 (CLS) into rust_rag_dev
make run-pg               # backend: bge-m3 + Postgres-routed VectorStore
make frontend-dev         # frontend pointed at the local backend (auth off by default)
make e2e-local            # prints the run-pg + frontend-dev recipe
```

- **Postgres**: shared instance at `10.10.10.207` with `mats` role. `make run-pg` defaults `RAG_DATABASE_URL=postgres://mats:…@10.10.10.207/rust_rag_dev`. Override to use a local Homebrew Postgres if preferred.
- **Embedder acceleration via CoreML** (still pending): `ort` 2.0 ships a CoreML execution provider that routes ops to the Apple Neural Engine, falling back to GPU (Metal) and CPU. Mirror the existing `cuda` feature: `coreml = ["ort/coreml"]` + a `#[cfg(feature = "coreml")]` arm in `embedding/mod.rs::execution_providers()`. Default stays CPU so non-Mac CI builds stay clean.
- **bge-m3 ONNX**: exported locally via `scripts/export_bge_m3.sh` (self-contained venv with `optimum` + `optimum-onnx`). External-data layout — `model.onnx` is the graph (~1MB), weights are sibling files. `~2.3 GB` total directory; keep it together when copying to the cluster image.
- **Auth off by default** for local e2e: both the backend (`RAG_AUTH_ENABLED=false` in Makefile) and the Next.js middleware (`frontend/proxy.ts` now respects `authEnabled`). Flip both to `true` together if testing the Zitadel flow.
- **Topology shortcut**: local Rust API talks to `10.10.10.207:5432` directly — no k8s Service in front of Postgres needed.

## Phases

### Phase 0 — Eval harness (in progress)

- 30–50 real queries from `messages` + agent traces.
- Each tagged with 1–3 expected entry IDs in top-5.
- Compute recall@5, MRR.
- Run against current SQLite system to set baseline.
- Reused unchanged across all subsequent phases.

### Phase 1 — Postgres + bge-m3 dense

- ~~`Db` trait abstraction (SQLite + Postgres impls during cutover).~~ **Done.** The repo already had `VectorStore`, `MessageStore`, `UserMemoryStore`, `AuthStore` traits with `SqliteVectorStore` implementing all four; `main.rs` constructs `Arc<dyn …>` for each. Only outstanding cleanup was the ontology worker holding `Arc<SqliteVectorStore>` directly — fixed by adding `get_items_pending_ontology` + `mark_ontology_status` to `VectorStore` (consistent with the graph methods already on it) and switching `ontology.rs` to `Arc<dyn VectorStore>`. No unifying `Db` trait was needed; the four-way split already accommodates a `PostgresVectorStore` impl beside the SQLite one.
- ~~Schema + migrations.~~ **Done.** `migrations/0001_documents_chunks.sql` plus the embedded runner in `src/db/postgres.rs`. Applied on connect; tracked in `schema_migrations`.
- ~~ONNX export of bge-m3 dense head; bake into `assets/bge-m3/`.~~ **Done.** Reproducible export script `scripts/export_bge_m3.sh` (self-contained venv with `optimum` + `optimum-onnx`). Output is fp32 with external-data ONNX layout — `model.onnx` is the graph (~1MB), weights live in sibling files in the same directory. INT8/fp16 quantization deferred until cluster deploy needs it; on Mac unified memory fp32 is fine.
- ~~`RAG_DATABASE_URL` env knob.~~ **Done.** When set, server connects + applies migrations on boot. SQLite remains active until `PostgresVectorStore` lands.
- ~~Re-ingest path (no migration of vectors needed).~~ **Done.** `src/bin/migrate_sqlite_to_pg.rs` reads a SQLite snapshot, re-embeds with the configured ONNX model, writes one document + one chunk per row. ON CONFLICT updates documents in place; chunks are replaced. Re-runnable.
- ~~Embedder support for bge-m3.~~ **Done.** `OrtBackend` introspects the session's input names and only passes `token_type_ids` when the model declares it (XLM-RoBERTa takes 2 inputs; BERT takes 3). `Pooling::Cls` plumbed through `Embedder` + `RAG_EMBEDDING_POOLING` config knob; bge-m3 default is `cls` (mean stays the default for backward-compat with the bge-small SQLite store). Unit-tested with a hand-computed CLS-pooling expectation.
- ~~`PostgresVectorStore` impl of `VectorStore`.~~ **Done.** Sync trait method → async tokio-postgres call via `Handle::block_on`. `main.rs` selects Postgres when `RAG_DATABASE_URL` is set; SQLite still owns auth/messages/user_memory until those schemas are ported. Cosine search via `<=>`, per-document `MIN(distance)` aggregation. Hybrid search falls back to dense (sparse arrives in phase 2). Graph methods stubbed out (return disabled status / empty / errors on writes).
- ~~Header-aware chunking via `text-splitter`.~~ **Done in both migration and runtime paths.** New `src/chunking_md.rs` wraps `text-splitter`'s `MarkdownSplitter` with the bge-m3 tokenizer so chunk size is measured in real tokens, not characters. `RAG_CHUNK_MAX_TOKENS=500` / `RAG_CHUNK_OVERLAP_TOKENS=50` per the plan; tunable via env. The migration binary chunks-then-embeds-each, writing N chunks per document (55 docs → 66 chunks; the longest split into 5). The runtime path (`POST /store` in `api/mod.rs`) now follows the same shape via a new `VectorStore::upsert_document` trait method that takes `Vec<DocChunk>`; `MarkdownChunker` lives on `AppState::md_chunker`, constructed in `main.rs` only when `RAG_DATABASE_URL` is set. Verified by storing a deliberately long doc (~7k chars) and confirming 4 chunks land in `chunks` with the expected ~1750 chars each. SQLite gets a default impl that uses the first chunk only — adequate for the cutover window where SQLite isn't expected to grow new content. Section-path tracking (`chunks.section_path` populated via `pulldown-cmark` heading walk) is still pending.
- ~~Eval harness.~~ **Done** (with caveats). Curated 36-query test set in `eval/queries.json` against the prod snapshot (55 items). New `make run-baseline` boots the legacy SQLite/bge-small stack against a copy of the snapshot on a different port; `make eval EVAL_LABEL=…` runs the harness. Both stacks evaluated on 2026-05-08:

  | stack | recall@1 | recall@5 | MRR | p50 (ms) |
  |---|---|---|---|---|
  | SQLite + bge-small + mean (baseline) | 0.944 | 1.000 | 0.972 | 50 |
  | Postgres + bge-m3 + CLS + chunked | **1.000** | 1.000 | **1.000** | 244 |

  New stack wins or ties on every query, no regressions. Latency is ~5× higher (expected — bge-m3 is a much larger model on CPU; on the cluster's GPU it'll be far closer). Caveats: small corpus + small test set + queries hand-curated against the same items they target means absolute scores are optimistic — what's load-bearing is the head-to-head delta. Run records are in `eval/runs/`.
- Update `Dockerfile.cuda` and `deploy/kubernetes/rust-rag-cuda.yaml` (memory limit 2 GB → 4 GB; bake `assets/bge-m3/` into the image; set `RAG_DATABASE_URL` + secret + `RAG_EMBEDDING_POOLING=cls`).
- **Auxiliary tables port** before cluster cutover. Schema-mirror approach (1:1 BIGINT timestamps + JSONB metadata) so migration is a straight copy and trait impls stay short. Schema landed as `migrations/0002_auxiliary_tables.sql`. Status:
  - ~~`messages`~~ **Done.** `MessageStore` impl on `PostgresVectorStore` covering all 10 trait methods (cf. `src/db/postgres.rs`), including the `permission_request` lookup via `metadata->>'request_id'` (indexed). Migration extended in `migrate_sqlite_to_pg`: prod snapshot's 161 messages copied (ON CONFLICT DO NOTHING; re-runnable). `main.rs` now routes `Arc<dyn MessageStore>` to Postgres when `RAG_DATABASE_URL` is set.
  - ~~`mcp_tokens` + `device_auth_requests`~~ **Done.** `AuthStore` impl on `PostgresVectorStore` covering all 12 trait methods. `interval_secs` corrected to `BIGINT` (matches `i64` field). Migration extended: snapshot's 9 tokens + 19 device_auth requests copied (ON CONFLICT DO NOTHING). `main.rs` routes `Arc<dyn AuthStore>` to Postgres when `RAG_DATABASE_URL` is set.
  - ~~`user_events` + `user_profiles`~~ **Done** (caveats below). `UserMemoryStore` impl on `PostgresVectorStore` covering all 6 trait methods; `query_embedding` and `interest_embedding` use `vector(1024)`. Old 384-d events skipped (only 2 in the snapshot, both stale). `touch_item_accesses` is intentionally a no-op on the Postgres path — `documents` doesn't carry `access_count`/`last_accessed`, and popularity boost isn't load-bearing for retrieval. `main.rs` routes `Arc<dyn UserMemoryStore>` to Postgres when `RAG_DATABASE_URL` is set.

    **Caveats** (functional, not blocking the cutover): personalization is dormant in this deployment — most traffic uses the bare frontend API key (which sets `SessionSubject(None)`) or MCP tokens issued without a subject (all 9 in the snapshot have NULL subject). Only Zitadel session-cookie users emit events, hence the 2-row history. Even when wired up, the design (centroid blend at 80/20 over a 30-event window with no decay, mixing search/view/store events) is a guess that has never been validated against an eval. Not worth tuning until there's real multi-user traffic and an eval signal.
  - ~~`manager_memory`~~ **Dropped — not ported.** The SQLite table is dead code. No live Rust code reads or writes to it; manager notes have lived as regular `items`/`documents` rows with `source_id='manager_memory'` since the design changed. The 7 leftover SQLite rows are legacy. Documented inline in `migrations/0002_auxiliary_tables.sql`.
  - ~~`graph_edges`~~ **Done.** Schema in 0002 (FKs to `documents`). All five `VectorStore` graph methods reimplemented on `PostgresVectorStore`: `graph_status`, `graph_neighborhood` (BFS in Rust), `list_graph_edges`, `add_manual_edge`, `delete_graph_edge`. `rebuild_similarity_graph` rebuilds from the 1024-d vectors via a single SQL: per-document MIN cosine distance (`<=>`) across chunk pairs, top-k per origin doc filtered by `max_distance`, canonical (a < b) ordering. Smoke-tested end-to-end: 56 docs → **204 similarity edges** in one rebuild; status, neighborhood (BFS over edges, with chunk-level pairwise distances), list, add manual edge, delete manual edge all return correct shapes. `main.rs` now triggers the rebuild on startup for the Postgres path when `graph_enabled && graph_build_on_startup` (matching the SQLite behavior).
- **Legacy `:c:N` document IDs in the snapshot**: the source SQLite `items` table contains entries like `process_rag_maintenance_v1:c:0` / `:c:1` — leftovers from the *old* char-based `src/api/chunking.rs` that chunked at the API layer and stored each chunk as a separate item. The migration treats each as its own document, which works but means we have artificial document boundaries that should ideally be re-merged into a single parent. Cleanup pass after cutover: regroup `<id>:c:N` siblings under the bare `<id>`.

### Phase 2 — Sparse + hybrid

- Add `sparse_embedding sparsevec(250002)` column.
- Either ONNX sparse head or Python sidecar (decide based on phase 1 experience).
- RRF query (CTE-based, top-50 candidates).
- Recency decay multiplier.
- Rerun eval; must beat phase 1.

### Phase 3 — Reranker

- `bge-reranker-v2-m3` cross-encoder on top-50 → top-k.
- Runs on same GPU. Latency budget ~50–150 ms for k=50.
- Rerun eval; must beat phase 2.

### Phase 4 — (deferred) Colbert

- Only if phase 3 isn't enough.
- Storage problem must be solved first (per-token vectors balloon: 1k tokens × 1024 dims ≈ 4 MB/chunk; quantization mandatory).

## Things explicitly NOT being done

- Vector migration from SQLite. Re-ingest from sources.
- Colbert in v1.
- HNSW tuning before eval shows search is the bottleneck.
- Custom chunking lib — use `text-splitter`.
- Sparse storage micro-optimization — `sparsevec` is already tight.
- Weighted-sum hybrid scoring — RRF instead.

## Open questions

- ONNX export of bge-m3 sparse head: dense head was straightforward via `optimum-cli`; sparse head needs the `SparseEmbedding` layer included. Either patch the export with the sparse linear head from `FlagEmbedding`, or accept this as the trigger to switch to Option B (Python sidecar). Decide once eval shows whether dense alone is good enough.
- Python sidecar deploy unit: separate k8s Deployment, or sidecar container in the `rust-rag-cuda` pod (sharing the GPU)?
- Auxiliary-table port shape: 1:1 schema (mirror the SQLite shape exactly) or rethink — e.g. fold `manager_memory` into `documents` with a reserved `source_id`, drop `user_events` in favor of a generic event table? Lean toward 1:1 for the cutover and rethink only if a clear win emerges.
- NetworkPolicy / firewall: confirm the cluster nodes can reach `10.10.10.207:5432`, and whether anything besides the `rust-rag-cuda` pod needs DB access.

## Next-step recommendation

**Eval cleared the gate** (see Phase 1 — new stack wins recall@1 1.0 vs 0.944, MRR 1.0 vs 0.972, no regressions). Cluster cutover is unblocked. Recommended order:

1. ~~**Auxiliary-table port**~~ **Done.** All four trait surfaces (`MessageStore`, `AuthStore`, `UserMemoryStore`, graph methods on `VectorStore`) now have Postgres impls; `manager_memory` confirmed dead and dropped from the port. The SQLite sidecar at `data/rag-m3.db` is still opened on `make run-pg` boot but is no longer read or written for any of the trait surfaces — it can be deleted from prod manifests safely once the cluster cuts over.
2. ~~**`Dockerfile.cuda` + `rust-rag-cuda.yaml` updates**~~ **Done** (file changes only — apply is gated on the rollout below). Image no longer bakes the ~2 GB bge-m3 assets — they live on a dedicated `rust-rag-cuda-models` PVC, populated on first boot by an `initContainer` that pulls the ONNX export from HuggingFace (`BAAI/bge-m3`). Skip-if-present check makes restarts no-ops. `Dockerfile.cuda` defaults env to bge-m3 (1024-d, CLS, chunk envs) and copies `migrations/` so the embedded migrations compile in. The manifest's ConfigMap targets those paths, bumps `RAG_CUDA_MEM_LIMIT_MB` to 4096, lifts the pod memory request to 4 Gi (limit 12 Gi), and references a new `rust-rag-postgres` Secret carrying `RAG_DATABASE_URL`. SQLite PVC stays mounted for now (legacy `SqliteVectorStore` still opens on boot but is unused — drop the PVC after the cutover settles).

   **Rollout playbook** (do these from a workstation with `kubectl` access):

   ```bash
   # 1. Prereqs
   make export-bge-m3                                  # local export only — needed for `migrate-prod`, not the cluster image
   make fetch-prod-snapshot                            # /tmp/rust-rag-prod-snapshot/rag.db
   make migrate-prod                                   # re-embed + write to Postgres (~17s for 55 docs / 66 chunks)
   kubectl -n home create secret generic rust-rag-postgres \
     --from-literal=RAG_DATABASE_URL="postgres://mats:jagharpostgres@10.10.10.207:5432/rust_rag_dev"

   # 2. Network reachability — confirm node `midi` can reach 10.10.10.207:5432
   #    AND huggingface.co (TCP 443) for the init container's first-boot fetch.

   # 3. Build + push. Image is now ~0.4 GB (no model assets).
   make docker-build-cuda                              # tag matst80/rust-rag:cuda
   docker push matst80/rust-rag:cuda

   # 4. Apply the manifest. `Recreate` strategy → first boot is slow (~3–5 min
   #    for the init container to download bge-m3 from HF); subsequent
   #    restarts hit the cached PVC and start in ~30s.
   kubectl apply -f deploy/kubernetes/rust-rag-cuda.yaml

   # 5. Verify
   kubectl -n home logs deploy/rust-rag-cuda -c fetch-bge-m3 --tail=20
   kubectl -n home rollout status deploy/rust-rag-cuda
   kubectl -n home logs deploy/rust-rag-cuda --tail=80   # expect "postgres: connected"
   curl -s https://rag.k6n.net/graph/status | jq .       # item_count > 0
   ```

   **Rollback**: `kubectl -n home set image deploy/rust-rag-cuda rust-rag=matst80/rust-rag:cuda@sha256:<previous>` (digest of the bge-small image — note: the prior image bakes its own assets, so the models PVC is just unused weight after rollback). The legacy SQLite at `/app/data/rag.db` on the data PVC is untouched, so the old stack reads its prior state cleanly.
3. **Section-path tracking** (`chunks.section_path` via `pulldown-cmark` heading walk) — small, mergeable independently. Do before phase 2 so sparse retrieval can boost on heading hits.
4. **Legacy `:c:N` ID cleanup** in the migrated data (regroup `<id>:c:N` siblings under a single parent doc) — one-shot script, run before/after cutover.
5. **Phase 2 sparse + RRF** (per the original plan).

HNSW indexing and reranker stay deferred until eval (with a real production-shaped query set) shows they're needed. Re-running the eval harness after step 1 is cheap insurance against accidental regressions.

The current 36-query eval set is overly optimistic (small corpus, hand-curated keywords). Expand it with mined queries from `messages` once the auxiliary-table port lands and there's a real query log to mine.
