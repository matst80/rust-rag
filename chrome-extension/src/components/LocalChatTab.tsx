import { useEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  runLocalChat,
  formatLoadProgress,
  useLlmStatus,
  isWebGpuAvailable,
  requestPersistentStorage,
  type LocalChatMessage,
  type LocalToolCall,
} from '@rust-rag/llm'
import { buildExtensionRagTools } from '../ai/tools'
import type { Config } from '../types'

interface Msg {
  role: 'user' | 'assistant'
  content: string
  tools?: LocalToolCall[]
}

interface Props {
  config: Config
}

export function LocalChatTab({ config }: Props) {
  const [messages, setMessages] = useState<Msg[]>([])
  const [input, setInput] = useState('')
  const [busy, setBusy] = useState(false)
  const [webgpu, setWebgpu] = useState(false)
  const abortRef = useRef<AbortController | null>(null)
  const status = useLlmStatus()

  useEffect(() => { setWebgpu(isWebGpuAvailable()) }, [])

  const send = async () => {
    const text = input.trim()
    if (!text || busy) return
    setInput('')
    const history: LocalChatMessage[] = [
      ...messages
        .filter((m) => m.role === 'user' || m.role === 'assistant')
        .map((m) => ({ role: m.role, content: m.content })),
      { role: 'user' as const, content: text },
    ]
    setMessages((prev) => [
      ...prev,
      { role: 'user', content: text },
      { role: 'assistant', content: '' },
    ])
    setBusy(true)
    requestPersistentStorage().catch(() => {})
    abortRef.current = new AbortController()
    try {
      await runLocalChat({
        history,
        tools: buildExtensionRagTools(config),
        signal: abortRef.current.signal,
        onUpdate: ({ partialAnswer, toolCalls }) => {
          setMessages((prev) => {
            const next = [...prev]
            const last = next[next.length - 1]
            if (last.role === 'assistant') {
              last.content = partialAnswer ?? ''
              if (toolCalls) last.tools = toolCalls
            }
            return next
          })
        },
      })
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      if (msg !== 'aborted') {
        setMessages((prev) => {
          const next = [...prev]
          const last = next[next.length - 1]
          if (last.role === 'assistant') last.content = `Local chat error: ${msg}`
          return next
        })
      }
    } finally {
      setBusy(false)
    }
  }

  if (!webgpu) {
    return (
      <div style={{ padding: 16, fontSize: 12, color: 'var(--text-2)' }}>
        WebGPU is not available in this browser. Local chat is disabled.
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
      <div style={{ padding: '8px 12px', borderBottom: '1px solid var(--border, #333)', fontSize: 10, fontFamily: 'var(--mono)', textTransform: 'uppercase', letterSpacing: 2, color: 'var(--text-2)' }}>
        Local · Gemma{' '}
        {status.kind === 'loading' && (
          <span style={{ marginLeft: 8, opacity: 0.7 }}>{formatLoadProgress(status)}</span>
        )}
        {status.kind === 'error' && (
          <span style={{ marginLeft: 8, color: 'tomato' }}>{status.message}</span>
        )}
      </div>

      <div className="results-list" style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        {messages.length === 0 && (
          <p className="placeholder" style={{ opacity: 0.5 }}>
            Ask anything — runs entirely on this device, hits the RAG via tools.
          </p>
        )}
        {messages.map((m, i) => (
          <div key={i} style={{ marginBottom: 12 }}>
            <div style={{ fontSize: 9, fontFamily: 'var(--mono)', textTransform: 'uppercase', letterSpacing: 2, opacity: 0.5, marginBottom: 4 }}>
              {m.role}
            </div>
            {m.tools && m.tools.length > 0 && (
              <div style={{ marginBottom: 8 }}>
                {m.tools.map((t) => (
                  <div
                    key={t.id}
                    style={{
                      fontSize: 10,
                      fontFamily: 'var(--mono)',
                      padding: '4px 8px',
                      border: '1px solid var(--accent, #6366f1)',
                      background: 'rgba(99,102,241,0.08)',
                      marginBottom: 4,
                    }}
                    title={t.result ?? t.error ?? ''}
                  >
                    {t.name}({JSON.stringify(t.args)}) {t.error ? '⚠' : t.result ? '✓' : '…'}
                  </div>
                ))}
              </div>
            )}
            <div style={{ fontSize: 13, lineHeight: 1.5 }}>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{m.content || '…'}</ReactMarkdown>
            </div>
          </div>
        ))}
      </div>

      <div className="search-box" style={{ borderTop: '1px solid var(--border, #333)' }}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') send() }}
          disabled={busy}
          placeholder="Ask Gemma…"
        />
        <button onClick={busy ? () => abortRef.current?.abort() : send}>
          {busy ? '×' : '→'}
        </button>
      </div>
    </div>
  )
}
