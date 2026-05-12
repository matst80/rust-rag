"use client"

import type { ToolDef } from "@rust-rag/llm"
import { api } from "@/lib/api"

export function buildRagTools(): ToolDef[] {
  return [
    {
      name: "search",
      description:
        "search(query: string, top_k?: number) — semantic search over all entries. Returns id, source_id, score, snippet.",
      run: async (args) => {
        const query = String(args.query ?? "").trim()
        if (!query) return JSON.stringify({ error: "query is required" })
        const top_k = Math.min(8, Math.max(1, Number(args.top_k ?? 5)))
        const bundle = await api.search({ query, top_k })
        return JSON.stringify(
          bundle.results.slice(0, top_k).map((r) => ({
            id: r.id,
            source_id: r.source_id,
            score: Number(r.score.toFixed(3)),
            snippet: (r.text ?? "").slice(0, 280).replace(/\s+/g, " "),
          }))
        )
      },
    },
    {
      name: "get_entry",
      description: "get_entry(id: string) — full text + metadata for one entry.",
      run: async (args) => {
        const id = String(args.id ?? "").trim()
        if (!id) return JSON.stringify({ error: "id is required" })
        const entry = await api.items.get(id)
        return JSON.stringify({
          id: entry.id,
          source_id: entry.source_id,
          path: entry.path ?? null,
          text: (entry.text ?? "").slice(0, 4000),
          metadata: entry.metadata,
        })
      },
    },
    {
      name: "list_paths",
      description: "list_paths(source_id?: string) — list known wiki paths.",
      run: async (args) => {
        const source_id =
          typeof args.source_id === "string" && args.source_id
            ? args.source_id
            : undefined
        const data = await api.tree.paths(source_id)
        return JSON.stringify(
          data.paths.slice(0, 80).map((p) => ({
            source_id: p.source_id,
            path: p.path,
            count: p.count,
          }))
        )
      },
    },
  ]
}
