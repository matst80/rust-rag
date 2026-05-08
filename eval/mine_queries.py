#!/usr/bin/env python3
"""Dump candidate human-issued queries from the rust-rag SQLite messages table
for hand-curation into eval/queries.json. Output is a queries.json-shaped
skeleton with empty expected_ids that you fill in by hand."""

import argparse
import json
import sqlite3
import sys


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--db", default="rag.db")
    ap.add_argument("--limit", type=int, default=50)
    ap.add_argument("--min-len", type=int, default=12, help="Skip messages shorter than this many chars")
    ap.add_argument("--max-len", type=int, default=400, help="Skip messages longer than this many chars")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    rows = con.execute(
        """
        SELECT id, channel, sender, text, created_at
        FROM messages
        WHERE kind='text' AND sender_kind='human'
          AND length(text) BETWEEN ? AND ?
        ORDER BY created_at DESC
        LIMIT ?
        """,
        (args.min_len, args.max_len, args.limit),
    ).fetchall()

    out = {
        "_comment": "Candidate queries mined from messages table. Fill in expected_ids and prune to a frozen test set.",
        "queries": [
            {
                "query": text.strip(),
                "expected_ids": [],
                "source_id": None,
                "notes": f"mined: msg={mid} channel={channel} sender={sender} ts={ts}",
            }
            for (mid, channel, sender, text, ts) in rows
        ],
    }
    json.dump(out, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
