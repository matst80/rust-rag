#!/usr/bin/env python3
"""Retrieval eval runner. POSTs each query in queries.json to /search and
scores the ranked result list against expected_ids. Writes a timestamped run
record under eval/runs/."""

import argparse
import datetime as dt
import json
import os
import sys
import time
import urllib.request
import urllib.error


def post_search(base_url, query, top_k, source_id, hybrid, max_distance, api_key, timeout):
    body = {"query": query, "top_k": top_k, "hybrid": hybrid, "max_distance": max_distance}
    if source_id:
        body["source_id"] = source_id
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        f"{base_url.rstrip('/')}/search",
        data=data,
        method="POST",
        headers={"content-type": "application/json"},
    )
    if api_key:
        req.add_header("authorization", f"Bearer {api_key}")
    t0 = time.perf_counter()
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        payload = json.loads(resp.read())
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    return payload, elapsed_ms


def first_hit_rank(result_ids, expected_ids):
    expected = set(expected_ids)
    for i, rid in enumerate(result_ids, start=1):
        if rid in expected:
            return i
    return None


def score_query(result_ids, expected_ids, ks):
    rank = first_hit_rank(result_ids, expected_ids)
    out = {"first_hit_rank": rank, "rr": (1.0 / rank) if rank else 0.0}
    for k in ks:
        out[f"recall@{k}"] = 1 if (rank is not None and rank <= k) else 0
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base-url", default=os.environ.get("RAG_BASE_URL", "http://localhost:4001"))
    ap.add_argument("--queries", default=os.path.join(os.path.dirname(__file__), "queries.json"))
    ap.add_argument("--label", required=True, help="Run label, e.g. 'baseline-bge-small' or 'm3-dense-phase1'")
    ap.add_argument("--top-k", type=int, default=10)
    ap.add_argument("--ks", default="1,5,10", help="Comma-separated k values to report recall@")
    ap.add_argument("--hybrid", default="true")
    ap.add_argument("--max-distance", type=float, default=0.8)
    ap.add_argument("--api-key", default=os.environ.get("RAG_API_KEY"))
    ap.add_argument("--timeout", type=float, default=20.0)
    ap.add_argument("--runs-dir", default=os.path.join(os.path.dirname(__file__), "runs"))
    args = ap.parse_args()

    ks = [int(k) for k in args.ks.split(",") if k.strip()]
    hybrid = args.hybrid.lower() in ("1", "true", "yes")

    with open(args.queries) as f:
        cases = json.load(f).get("queries", [])
    cases = [c for c in cases if not c.get("query", "").startswith("EXAMPLE:")]
    if not cases:
        print("No non-example queries in queries.json. Edit it first.", file=sys.stderr)
        sys.exit(2)

    per_query = []
    failures = 0
    for case in cases:
        try:
            payload, elapsed_ms = post_search(
                args.base_url, case["query"], args.top_k,
                case.get("source_id"), hybrid, args.max_distance,
                args.api_key, args.timeout,
            )
            ids = [r["id"] for r in payload.get("results", [])]
            metrics = score_query(ids, case.get("expected_ids", []), ks)
            per_query.append({
                "query": case["query"],
                "expected_ids": case.get("expected_ids", []),
                "source_id": case.get("source_id"),
                "result_ids": ids,
                "elapsed_ms": round(elapsed_ms, 2),
                **metrics,
            })
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError) as e:
            failures += 1
            per_query.append({"query": case.get("query"), "error": str(e)})

    scored = [r for r in per_query if "error" not in r]
    n = len(scored)
    aggregate = {"n": n, "failures": failures, "mrr": round(sum(r["rr"] for r in scored) / n, 4) if n else 0.0}
    for k in ks:
        aggregate[f"recall@{k}"] = round(sum(r[f"recall@{k}"] for r in scored) / n, 4) if n else 0.0
    aggregate["p50_ms"] = round(sorted(r["elapsed_ms"] for r in scored)[n // 2], 2) if n else 0.0
    aggregate["p95_ms"] = round(sorted(r["elapsed_ms"] for r in scored)[max(0, int(n * 0.95) - 1)], 2) if n else 0.0

    record = {
        "label": args.label,
        "timestamp": dt.datetime.now(dt.timezone.utc).isoformat(),
        "base_url": args.base_url,
        "config": {"top_k": args.top_k, "hybrid": hybrid, "max_distance": args.max_distance, "ks": ks},
        "aggregate": aggregate,
        "per_query": per_query,
    }

    os.makedirs(args.runs_dir, exist_ok=True)
    safe_label = "".join(c if c.isalnum() or c in "-_" else "_" for c in args.label)
    out_path = os.path.join(args.runs_dir, f"{dt.datetime.now().strftime('%Y%m%d-%H%M%S')}-{safe_label}.json")
    with open(out_path, "w") as f:
        json.dump(record, f, indent=2)

    print(f"\nLabel: {args.label}")
    print(f"Run:   {out_path}")
    print(f"N: {n}  failures: {failures}")
    for k, v in aggregate.items():
        if k in ("n", "failures"):
            continue
        print(f"  {k:>10s}: {v}")
    print()
    print(f"{'rank':>4s}  {'rr':>5s}  query")
    for r in per_query:
        if "error" in r:
            print(f"  ERR  {r.get('error','')[:60]}  {r.get('query','')[:60]}")
            continue
        rk = r["first_hit_rank"] or "-"
        print(f"  {str(rk):>3s}  {r['rr']:>5.2f}  {r['query'][:90]}")


if __name__ == "__main__":
    main()
