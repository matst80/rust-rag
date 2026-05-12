import type { ToolDef } from '@rust-rag/llm'
import type { Config } from '../types'

interface SearchHit {
  id: string
  source_id: string
  score: number
  text: string
}

async function authFetch(config: Config, path: string, init?: RequestInit) {
  const headers = new Headers(init?.headers)
  if (config.apiToken) headers.set('Authorization', `Bearer ${config.apiToken}`)
  if (init?.body && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json')
  const res = await fetch(`${config.apiBaseUrl}${path}`, { ...init, headers })
  if (!res.ok) throw new Error(`${path} → ${res.status} ${res.statusText}`)
  return res
}

export function buildExtensionRagTools(config: Config): ToolDef[] {
  return [
    {
      name: 'search',
      description:
        'search(query: string, top_k?: number) — semantic search over all entries. Returns id, source_id, score, snippet.',
      run: async (args) => {
        const query = String(args.query ?? '').trim()
        if (!query) return JSON.stringify({ error: 'query is required' })
        const top_k = Math.min(8, Math.max(1, Number(args.top_k ?? 5)))
        const res = await authFetch(config, '/api/search', {
          method: 'POST',
          body: JSON.stringify({ query, top_k }),
        })
        const data = await res.json()
        const results: SearchHit[] = data.results ?? []
        return JSON.stringify(
          results.slice(0, top_k).map((r) => ({
            id: r.id,
            source_id: r.source_id,
            score: Number(r.score?.toFixed?.(3) ?? r.score),
            snippet: (r.text ?? '').slice(0, 280).replace(/\s+/g, ' '),
          })),
        )
      },
    },
    {
      name: 'get_entry',
      description: 'get_entry(id: string) — full text + metadata for one entry.',
      run: async (args) => {
        const id = String(args.id ?? '').trim()
        if (!id) return JSON.stringify({ error: 'id is required' })
        const res = await authFetch(config, `/api/items/${encodeURIComponent(id)}`)
        const entry = await res.json()
        return JSON.stringify({
          id: entry.id,
          source_id: entry.source_id,
          path: entry.path ?? null,
          text: (entry.text ?? '').slice(0, 4000),
          metadata: entry.metadata,
        })
      },
    },
    {
      name: 'list_paths',
      description: 'list_paths(source_id?: string) — list known wiki paths.',
      run: async (args) => {
        const qs =
          typeof args.source_id === 'string' && args.source_id
            ? `?source_id=${encodeURIComponent(args.source_id as string)}`
            : ''
        const res = await authFetch(config, `/api/entries/paths${qs}`)
        const data = await res.json()
        return JSON.stringify(
          (data.paths ?? []).slice(0, 80).map((p: { source_id: string; path: string; count: number }) => ({
            source_id: p.source_id,
            path: p.path,
            count: p.count,
          })),
        )
      },
    },
  ]
}

export async function postStore(config: Config, body: Record<string, unknown>) {
  const res = await authFetch(config, '/api/store', { method: 'POST', body: JSON.stringify(body) })
  return res.json()
}
