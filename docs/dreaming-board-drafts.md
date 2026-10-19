# Dreaming + ranking rework — board drafts (dev-instance CLI not present here)

File these with `dev-instance board create --project <project> ...`.
Suggested project: `mats/rust-rag` if it exists, else `mats/eventsourced-ai` tooling board.

## Ticket 1 — Dreaming is now corpus-only (done, needs verify on prod)
**Column:** review — **Tags:** dreaming, corpus, cleanup
**Body:**
Corpus-only cut landed: removed per-item LLM loop (measured noise: pairwise
0.44–0.54 band, P(same project) 0.267 vs 0.263 chance, AUC 0.409).
`process_dreaming_round` now calls `run_corpus_dreaming_round` only; no LLM,
no embedder. Removed `DreamScope`, item-loop config
(batch/concurrency/target/allowlist/protected/max_actions/max_items).
Dup pairs reported as `proposed` audit rows (cap 200), never auto-applied.
Verify: `cargo check`, `cargo test --lib api::dreaming db::`.
Follow-ups: taxonomy pass is still `max_context_tokens=0` (disabled);
`review_interval_secs` parsed but unused; `docs/dreaming.md` still missing.

## Ticket 2 — BM25-first ranking (done locally, needs eval)
**Column:** review — **Tags:** search, bm25, hybrid, ranking
**Body:**
Local SQLite `search_hybrid` was equal-weight RRF (dense noise + BM25 signal)
with AND-only FTS. Changed to OR semantics (`term"* OR ...`), dropped
1-char tokens, weighted RRF `W_DENSE=0.3 / W_KEYWORD=1.0` with corrected
pseudo-distance max `(Wd+Wk)/k`.
Prod Postgres hybrid has NO BM25 — dense+sparse only, dense-only fallback
when sparse empty. Next: confirm sparse populated in prod; consider trigram/
tsvector fallback or sparse-weight boost; add `BM25 vs hybrid vs dense-only`
eval on `tests/fixtures_memory.jsonl` project labels before tuning further.

## Ticket 3 — Code search (`cs` / `/api/code/*`) gaps
**Column:** backlog — **Tags:** cs, code-search, dx
**Body:**
`cs` binary absent in this env; assessed from `src/db/code_store.rs`,
`src/api/code.rs`, `frontend/app/code`, fixtures.
Works: symbol search (BM25/trigram), callers/callees, symbol-graph BFS
depth 1–5, file browser, dead/stats/outline (per fixtures on 166k-line repo).
Gaps from fixtures + code: `cs tree` lacks `--max-depth`; scoped search
flaky (`code.search` path-scoped `pkg` returned 0); `code_tree` needs bare
symbol names not paths; `/api/code/*` 503 without `RAG_DATABASE_URL`
(SQLite dev gets no code search); no `max-depth` on graph sheet default 3.
Propose: add `--max-depth`, fix path-scoped search, accept path-or-symbol
in tree/graph, document Postgres-only limit or add SQLite fallback.

## Knowledge entry (`project:rust-rag:knowledge`, path `dreaming/corpus-cut`)
**Title:** Dreaming corpus-only cut + BM25-first ranking
**Text:**
Dreaming item loop removed after measuring dense at chance (see Ticket 1
numbers). Corpus pipeline (`src/cluster.rs` TF-IDF identifying tokens, hub
filter, mutual-kNN, cohesion gate + `migrations/0021_dream_clusters.sql`)
is the only path; it clusters, proposes dups, persists `dream_clusters`,
deletes nothing. Ranking: local hybrid is now BM25-first (OR FTS, 1.0 vs 0.3
RRF); prod hybrid still dense+sparse with no BM25 — check sparse coverage
before further tuning. Code search useful for callers/graph/outline/dead
but needs tree depth flag + scoped-search fix (Ticket 3).
