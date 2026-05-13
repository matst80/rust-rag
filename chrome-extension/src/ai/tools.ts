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
    {
      name: 'get_page_info',
      description: 'get_page_info() — gets current tab URL, title, and any text the user has selected.',
      run: async () => {
        try {
          const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
          if (!tab?.id) return JSON.stringify({ error: 'no active tab' })
          const results = await chrome.scripting.executeScript({
            target: { tabId: tab.id },
            func: () => ({
              url: window.location.href,
              title: document.title,
              selection: window.getSelection()?.toString() || '',
            }),
          })
          return JSON.stringify(results?.[0]?.result ?? { error: 'failed to get info' })
        } catch (err) {
          return JSON.stringify({ error: String(err) })
        }
      },
    },
    {
      name: 'extract_images',
      description: 'extract_images() — finds prominent images on the current page. returns src, alt, and dimensions.',
      run: async () => {
        try {
          const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
          if (!tab?.id) return JSON.stringify({ error: 'no active tab' })
          const results = await chrome.scripting.executeScript({
            target: { tabId: tab.id },
            func: () => {
              const imgs = Array.from(document.querySelectorAll('img'))
              return imgs
                .map((img) => ({
                  src: img.src,
                  alt: img.alt,
                  width: img.naturalWidth || img.width,
                  height: img.naturalHeight || img.height,
                  area: (img.naturalWidth || img.width) * (img.naturalHeight || img.height),
                }))
                .filter((img) => img.src && img.area > 5000 && !img.src.startsWith('data:')) // filter out tiny icons and data urls
                .sort((a, b) => b.area - a.area)
                .slice(0, 10)
            },
          })
          return JSON.stringify(results?.[0]?.result ?? [])
        } catch (err) {
          return JSON.stringify({ error: String(err) })
        }
      },
    },
    {
      name: 'analyze_image',
      description: 'analyze_image(url: string, prompt?: string) — uses the local multimodal LLM to describe or analyze an image from the page.',
      run: async (args) => {
        const url = String(args.url ?? '').trim()
        if (!url) return JSON.stringify({ error: 'url is required' })
        const prompt = String(args.prompt ?? 'Describe this image in detail.').trim()
        
        try {
          const { getLlmHelper } = await import('@rust-rag/llm')
          const helper = getLlmHelper()
          const description = await helper.generate({
            prompt,
            images: [url],
            maxTokens: 512,
          })
          return JSON.stringify({ url, description })
        } catch (err) {
          return JSON.stringify({ error: String(err) })
        }
      },
    },
    {
      name: 'save_note',
      description: 'save_note(title: string, text: string, tags?: string[], image_url?: string) — saves a new entry to the knowledge base.',
      run: async (args) => {
        const title = String(args.title ?? '').trim()
        let text = String(args.text ?? '').trim()
        if (!title || !text) return JSON.stringify({ error: 'title and text are required' })
        
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
        const source_url = tab?.url || ''
        
        if (args.image_url) {
          text += `\n\n![Main Image](${args.image_url})`
        }

        const payload = {
          id: `ext-${Date.now()}`,
          source_id: 'extension',
          text: `# ${title}\n\n${text}\n\n---\nSource: ${source_url}`,
          metadata: {
            author: 'chrome-extension',
            tags: Array.isArray(args.tags) ? args.tags : [],
            source_url,
          }
        }
        
        try {
          const res = await authFetch(config, '/api/store', {
            method: 'POST',
            body: JSON.stringify(payload),
          })
          return JSON.stringify(await res.json())
        } catch (err) {
          return JSON.stringify({ error: String(err) })
        }
      },
    },

  ]
}


export async function postStore(config: Config, body: Record<string, unknown>) {
  const res = await authFetch(config, '/api/store', { method: 'POST', body: JSON.stringify(body) })
  return res.json()
}
