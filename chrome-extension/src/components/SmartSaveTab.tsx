import { useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  getLlmClient,
  formatLoadProgress,
  useLlmStatus,
  isWebGpuAvailable,
} from '@rust-rag/llm'
import { postStore } from '../ai/tools'
import type { Config } from '../types'

interface Props {
  config: Config
}

const PROMPT_HEADER = `You are summarizing a web page for a personal knowledge base.

Output strict JSON only (no markdown fences) with this shape:
{
  "title": "short title (4-10 words)",
  "summary": "3-5 sentence markdown summary of the page",
  "tags": ["lowercase", "comma-separated"],
  "source_id": "guess one: knowledge | notes | bookmarks | reference"
}

If the page is paywalled, navigational, or too thin to summarize, return:
{"error": "insufficient content"}
`

async function grabPageContent(): Promise<{ url: string; title: string; text: string }> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (!tab?.id) throw new Error('No active tab')
  const [{ result }] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: () => {
      const article = document.querySelector('article') as HTMLElement | null
      const main = document.querySelector('main') as HTMLElement | null
      const root = article || main || document.body
      return {
        title: document.title,
        url: location.href,
        text: (root.innerText || '').slice(0, 12000),
      }
    },
  })
  return result as { url: string; title: string; text: string }
}

interface Draft {
  title: string
  summary: string
  tags: string[]
  source_id: string
  url: string
  pageTitle: string
}

function tryParseDraft(raw: string, url: string, pageTitle: string): Draft | null {
  try {
    const cleaned = raw.replace(/```json|```/g, '').trim()
    const obj = JSON.parse(cleaned)
    if (obj.error) return null
    return {
      title: String(obj.title ?? pageTitle).slice(0, 200),
      summary: String(obj.summary ?? ''),
      tags: Array.isArray(obj.tags) ? obj.tags.map(String).slice(0, 8) : [],
      source_id: String(obj.source_id ?? 'bookmarks'),
      url,
      pageTitle,
    }
  } catch {
    return null
  }
}

export function SmartSaveTab({ config }: Props) {
  const [webgpu] = useState(() => isWebGpuAvailable())
  const status = useLlmStatus()
  const [running, setRunning] = useState(false)
  const [rawOutput, setRawOutput] = useState('')
  const [draft, setDraft] = useState<Draft | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)

  const run = async () => {
    setRunning(true)
    setRawOutput('')
    setDraft(null)
    setError(null)
    setSaved(false)
    try {
      const { url, title, text } = await grabPageContent()
      const prompt = `${PROMPT_HEADER}\n\nURL: ${url}\nTitle: ${title}\n\nPage content:\n${text}\n\nJSON:`
      const client = getLlmClient('text')
      await client.generate(prompt, (partial) => setRawOutput(partial))
      // After generation completes, try to parse from rawOutput accumulated above.
      // Use a setTimeout to grab the latest committed state from the closure.
      setTimeout(() => {
        const final = (document.querySelector('[data-smart-save-raw]')?.textContent ?? rawOutput).trim()
        const parsed = tryParseDraft(final, url, title)
        if (parsed) setDraft(parsed)
        else setError('Could not parse model output as JSON')
      }, 50)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setRunning(false)
    }
  }

  const save = async () => {
    if (!draft) return
    setSaving(true)
    try {
      await postStore(config, {
        text: `# ${draft.title}\n\n${draft.summary}`,
        source_id: draft.source_id,
        metadata: {
          url: draft.url,
          title: draft.pageTitle,
          tags: draft.tags.join(','),
          captured_at: new Date().toISOString(),
          captured_by: 'extension-sidepanel-local',
        },
      })
      setSaved(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }

  if (!webgpu) {
    return (
      <div style={{ padding: 16, fontSize: 12, color: 'var(--text-2)' }}>
        WebGPU is not available. Smart save needs the on-device model.
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 12 }}>
      <button className="btn-primary" disabled={running} onClick={run}>
        {running ? 'Summarizing…' : 'Smart save this page'}
      </button>
      {status.kind === 'loading' && (
        <p style={{ fontSize: 10, fontFamily: 'var(--mono)', color: 'var(--text-2)' }}>
          {formatLoadProgress(status)}
        </p>
      )}
      {status.kind === 'error' && (
        <p style={{ fontSize: 10, color: 'tomato' }}>{status.message}</p>
      )}

      {rawOutput && !draft && (
        <pre
          data-smart-save-raw
          style={{
            fontSize: 11,
            fontFamily: 'var(--mono)',
            padding: 8,
            border: '1px solid var(--border, #333)',
            background: 'rgba(99,102,241,0.04)',
            maxHeight: 200,
            overflow: 'auto',
          }}
        >
          {rawOutput}
        </pre>
      )}

      {error && <p style={{ color: 'tomato', fontSize: 12 }}>{error}</p>}

      {draft && (
        <div style={{ border: '1px solid var(--accent, #6366f1)', padding: 12 }}>
          <p style={{ fontSize: 10, fontFamily: 'var(--mono)', textTransform: 'uppercase', letterSpacing: 2, color: 'var(--text-2)' }}>
            Draft → {draft.source_id}
          </p>
          <h3 style={{ margin: '8px 0', fontSize: 14 }}>{draft.title}</h3>
          <div style={{ fontSize: 12, lineHeight: 1.5 }}>
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{draft.summary}</ReactMarkdown>
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, margin: '8px 0' }}>
            {draft.tags.map((t) => (
              <span
                key={t}
                style={{
                  fontSize: 10,
                  fontFamily: 'var(--mono)',
                  padding: '2px 6px',
                  border: '1px solid var(--border, #333)',
                }}
              >
                {t}
              </span>
            ))}
          </div>
          <p style={{ fontSize: 10, color: 'var(--text-2)' }}>
            {draft.url}
          </p>
          <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
            <button className="btn-primary" disabled={saving || saved} onClick={save}>
              {saved ? 'Saved ✓' : saving ? 'Saving…' : 'Save to RAG'}
            </button>
            <button onClick={() => { setDraft(null); setRawOutput('') }}>
              Discard
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
