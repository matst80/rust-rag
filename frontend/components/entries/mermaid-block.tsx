"use client"

import { useEffect, useId, useRef, useState } from "react"
import mermaid from "mermaid"

let initialized = false
function ensureInit() {
  if (initialized) return
  mermaid.initialize({
    startOnLoad: false,
    theme: "dark",
    securityLevel: "loose",
    fontFamily: "var(--font-mono), ui-monospace, monospace",
  })
  initialized = true
}

interface MermaidBlockProps {
  code: string
}

export function MermaidBlock({ code }: MermaidBlockProps) {
  const reactId = useId()
  const id = `mmd-${reactId.replace(/[^a-zA-Z0-9_-]/g, "")}`
  const ref = useRef<HTMLDivElement>(null)
  const [svg, setSvg] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    ensureInit()
    mermaid
      .render(id, code)
      .then(({ svg }) => {
        if (!cancelled) {
          setSvg(svg)
          setError(null)
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e))
        }
      })
    return () => {
      cancelled = true
    }
  }, [code, id])

  if (error) {
    return (
      <div className="my-8 overflow-hidden rounded-2xl border border-red-500/30 bg-red-950/30 p-4">
        <div className="mb-2 text-xs font-black uppercase tracking-[0.2em] text-red-400/70">
          Mermaid error
        </div>
        <pre className="overflow-x-auto whitespace-pre-wrap text-xs text-red-200/80">
          {error}
        </pre>
        <pre className="mt-3 overflow-x-auto whitespace-pre-wrap text-xs text-white/40">
          {code}
        </pre>
      </div>
    )
  }

  return (
    <div
      ref={ref}
      className="my-8 flex justify-center overflow-x-auto rounded-2xl border border-white/10 bg-[#0d1117]/60 p-6 shadow-xl backdrop-blur-sm"
      dangerouslySetInnerHTML={svg ? { __html: svg } : undefined}
    />
  )
}
