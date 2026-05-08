# Retrieval Eval Harness

System-agnostic eval that calls the running rust-rag HTTP API at `/search` and
scores ranked results against a hand-curated test set. Reuse it unchanged
across SQLite/bge-small (baseline), bge-m3 dense, hybrid, and reranker phases.

## Files

- `queries.json` — test set: `{ query, expected_ids[], source_id?, notes? }`. Edit by hand.
- `run_eval.py` — runner. Posts each query, computes recall@k and MRR, writes a run record under `runs/`.
- `mine_queries.py` — helper. Dumps recent human `kind='text'` messages from the SQLite `messages` table as candidate query stubs.

## Usage

```bash
# 1. Make sure the API is running (default http://localhost:4001).
make run

# 2. (Optional) seed candidate queries from the messages table.
python3 eval/mine_queries.py --db rag.db --limit 50 > eval/candidates.json

# 3. Curate eval/queries.json by hand: pick 30–50 queries, fill in expected_ids
#    by running the search yourself or grepping the items table.

# 4. Run eval. Saves a timestamped record to eval/runs/.
python3 eval/run_eval.py --base-url http://localhost:4001 --label "baseline-bge-small"

# 5. After each phase (m3 dense, hybrid, reranker), rerun with a new --label and
#    diff the metrics. A phase that doesn't beat the prior label doesn't merge.
```

## Metrics

- **recall@k** — fraction of queries where ANY expected id is in top-k.
- **MRR** — mean reciprocal rank of the first hit. 0 if no expected id appears.
- Reported at k=1, 5, 10. Top_k=10 is fetched for every query.

## Test set construction tips

- Mine `messages` (sender_kind='human', kind='text') for real questions.
- For each query, set `expected_ids` to 1–3 entry IDs that *should* appear in top-5. Use the items table to find them: `SELECT id, substr(text,1,80) FROM items WHERE text LIKE '%keyword%';`.
- Keep a mix: precise keyword/ID lookups, fuzzy semantic queries, multi-concept queries.
- Set `source_id` for namespace-scoped tests (e.g. `project:rust-rag:knowledge`).
- Don't iterate the test set against the system you're tuning — you'll overfit. Freeze it.
