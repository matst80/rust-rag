"use client"

import { useEffect, useRef, useState } from "react"
import { Sparkles, X, ChevronRight, Wand2 } from "lucide-react"
import { api } from "@/lib/api"
import { cn } from "@/lib/utils"
import type { AssistedQueryRawResult } from "@/lib/api/types"
import Link from "next/link"

interface QueryBlock {
  index: number
  query: string
  results?: AssistedQueryRawResult[]
  status: "pending" | "done"
}

function distanceToScore(d: number) {
  return Math.max(0, Math.round((1 - d) * 100))
}

function scoreColor(pct: number) {
  if (pct >= 80) return "oklch(0.916 0.175 156.8)"
  if (pct >= 50) return "oklch(0.9 0.148 196.3)"
  return "oklch(0.42 0 0)"
}

const SUGGESTIONS = [
  "Recent research notes",
  "Connections between rust and rag",
  "How does vector search work?",
]

export function AssistedQueryView() {
  const [query, setQuery] = useState("")
  const [isStreaming, setIsStreaming] = useState(false)
  const [queries, setQueries] = useState<QueryBlock[]>([])
  const [merged, setMerged] = useState<AssistedQueryRawResult[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [mounted, setMounted] = useState(false)
  const abortRef = useRef<AbortController | null>(null)

  useEffect(() => setMounted(true), [])

  const run = async (q?: string) => {
    const q_ = (q ?? query).trim()
    if (!q_ || isStreaming) return
    setIsStreaming(true)
    setQueries([])
    setMerged(null)
    setError(null)

    abortRef.current = new AbortController()
    try {
      await api.query.assisted(
        { query: q_, top_k: 8 },
        {
          onQueries: (event) => {
            setQueries(event.queries.map((q, index) => ({ index, query: q, status: "pending" })))
          },
          onResult: (event) => {
            setQueries((prev) => {
              const next = [...prev]
              const target = next.find((item) => item.index === event.index)
              if (target) { target.results = event.results; target.status = "done" }
              return next
            })
          },
          onMerged: (event) => { setMerged(event.results) },
          onError: (err) => { setError(err.error.message) },
          onDone: () => { setIsStreaming(false) },
        },
        { signal: abortRef.current.signal }
      )
    } catch (err: unknown) {
      if ((err as { name?: string })?.name !== "AbortError") {
        setError((err as { message?: string })?.message ?? String(err))
      }
      setIsStreaming(false)
    }
  }

  const cancel = () => { abortRef.current?.abort(); setIsStreaming(false) }

  const hasActivity = isStreaming || queries.length > 0 || merged !== null

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      run()
    }
  }

  const inputBox = (
    <div
      className={cn(
        "relative flex flex-col w-full border border-border bg-card transition-all duration-200",
        "focus-within:border-primary focus-within:[box-shadow:0_0_0_1px_oklch(0.9_0.148_196.3/0.15),inset_0_0_30px_oklch(0.9_0.148_196.3/0.03)]"
      )}
    >
      {/* Header label */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border">
        <Sparkles className="size-3.5 text-primary" />
        <span className="font-mono text-[10px] font-black uppercase tracking-[3px] text-primary">
          AI-Assisted Search
        </span>
        <span className="font-mono text-[10px] text-muted-foreground ml-2 hidden sm:inline">
          — expands your question into sub-queries
        </span>
      </div>

      {/* Textarea */}
      <div className="flex-1 px-4 pt-4 pb-2">
        <textarea
          placeholder="What do you want to find?"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={isStreaming}
          autoFocus
          rows={1}
          className="w-full min-h-13 max-h-65 border-none bg-transparent outline-none p-0 text-base font-medium resize-none placeholder:text-muted-foreground/60 text-foreground"
        />
      </div>

      {/* Controls */}
      <div className="flex items-center justify-end px-3 pb-3 gap-2">
        {isStreaming ? (
          <button
            type="button"
            onClick={cancel}
            className="flex items-center gap-1.5 px-4 h-8 border border-border font-mono text-[10px] uppercase tracking-[1.5px] text-muted-foreground hover:border-destructive hover:text-destructive transition-colors"
          >
            <X className="size-3" /> Cancel
          </button>
        ) : (
          <button
            type="button"
            onClick={() => run()}
            disabled={!query.trim()}
            className={cn(
              "h-8 px-4 flex items-center gap-1.5 font-mono text-[10px] font-black uppercase tracking-[2px] transition-all",
              query.trim()
                ? "bg-primary text-primary-foreground shadow-[0_0_14px_oklch(0.9_0.148_196.3/0.3)] hover:shadow-[0_0_20px_oklch(0.9_0.148_196.3/0.4)]"
                : "bg-muted text-muted-foreground/30 cursor-not-allowed"
            )}
          >
            <Wand2 className="size-3.5" />
            Run
          </button>
        )}
      </div>

      {error && (
        <div className="px-4 py-2 font-mono text-[11px] text-destructive border-t border-destructive/20 bg-destructive/5">
          Error: {error}
        </div>
      )}
    </div>
  )

  return (
    <div className="relative flex w-full min-h-[calc(100vh-3rem)] flex-col overflow-hidden">
      <div className="mx-auto w-full max-w-3xl flex-1 flex flex-col px-6">

        {!hasActivity ? (
          /* ── Hero state ── */
          <div className="flex flex-1 flex-col items-center justify-center -mt-16">
            <div className="animate-in fade-in zoom-in duration-700 fill-mode-both mb-8">
              <Sparkles
                className="size-16 text-primary"
                style={{ filter: "drop-shadow(0 0 20px oklch(0.9 0.148 196.3 / 0.5))" }}
              />
            </div>

            <p className="mb-2 font-mono text-[10px] font-black uppercase tracking-[5px] text-primary animate-in fade-in duration-500 delay-100 fill-mode-both">
              AI-Assisted
            </p>

            <h1 className="mb-4 text-center text-4xl md:text-5xl font-extrabold tracking-tight text-foreground animate-in fade-in slide-in-from-bottom-4 duration-700 delay-200 fill-mode-both">
              Deep Search
            </h1>

            <p className="mb-12 text-center text-muted-foreground text-base max-w-md animate-in fade-in slide-in-from-bottom-4 duration-700 delay-300 fill-mode-both leading-relaxed">
              Your question is expanded into multiple sub-queries and merged into a single ranked result.
            </p>

            <div className="w-full animate-in fade-in slide-in-from-bottom-8 duration-700 delay-500 fill-mode-both">
              {inputBox}
            </div>

            {!query && mounted && (
              <div className="flex flex-wrap justify-center gap-2 mt-5 animate-in fade-in slide-in-from-top-2 duration-500 fill-mode-both">
                {SUGGESTIONS.map((s) => (
                  <button
                    key={s}
                    onClick={() => { setQuery(s); run(s) }}
                    className="px-3 py-1.5 border border-border bg-card font-mono text-[10px] uppercase tracking-[1px] text-muted-foreground transition-all hover:border-primary/50 hover:text-primary"
                  >
                    {s}
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : (
          /* ── Active state ── */
          <div className="flex flex-1 flex-col gap-0 py-8 animate-in fade-in slide-in-from-bottom-4 duration-500 fill-mode-both">
            {/* Sticky input */}
            <div className="sticky top-12 z-40 pb-6 pt-2 -mx-6 px-6 border-b border-border bg-background/95 backdrop-blur">
              {inputBox}
            </div>

            <div className="flex-1 w-full mt-6">
              {/* Sub-query feed */}
              {(isStreaming || queries.length > 0) && (
                <div className="border border-border bg-background mb-0">
                  <div className="flex items-center gap-2 px-4 py-2 border-b border-border bg-card">
                    <span className="font-mono text-[10px] font-black uppercase tracking-[3px] text-muted-foreground">
                      Sub-queries
                    </span>
                    {isStreaming && queries.length === 0 && (
                      <span className="font-mono text-[10px] text-muted-foreground animate-pulse ml-2">
                        generating...
                      </span>
                    )}
                    {queries.length > 0 && (
                      <span className="font-mono text-[10px] text-muted-foreground ml-auto">
                        {queries.filter((q) => q.status === "done").length}/{queries.length}
                      </span>
                    )}
                  </div>

                  <div className="flex flex-col divide-y divide-border">
                    {queries.map((block) => {
                      const hits = block.results?.length ?? 0
                      return (
                        <div
                          key={block.index}
                          className={cn(
                            "flex items-center gap-3 px-4 py-2.5 transition-colors",
                            block.status === "pending" ? "bg-primary/3" : "bg-transparent"
                          )}
                        >
                          <span className="font-mono text-[10px] text-muted-foreground/80 w-5 shrink-0">
                            {String(block.index + 1).padStart(2, "0")}
                          </span>
                          <div className="shrink-0">
                            {block.status === "pending" ? (
                              <div className="size-1.5 rounded-full bg-primary animate-pulse" />
                            ) : (
                              <div className="size-1.5 rounded-full bg-muted-foreground/30" />
                            )}
                          </div>
                          <span className="font-mono text-[11px] text-foreground/80 flex-1 min-w-0 truncate">
                            {block.query}
                          </span>
                          <span
                            className={cn(
                              "font-mono text-[10px] shrink-0 ml-2",
                              block.status === "pending"
                                ? "text-muted-foreground/30 animate-pulse"
                                : hits > 0
                                ? "text-primary"
                                : "text-muted-foreground/30"
                            )}
                          >
                            {block.status === "pending" ? "···" : `${hits} hit${hits !== 1 ? "s" : ""}`}
                          </span>
                        </div>
                      )
                    })}

                    {isStreaming && queries.length === 0 && (
                      <div className="flex items-center gap-3 px-4 py-2.5">
                        <span className="font-mono text-[10px] text-muted-foreground/80 w-5">__</span>
                        <div className="size-1.5 rounded-full bg-primary animate-pulse" />
                        <span className="font-mono text-[11px] text-muted-foreground/70">
                          waiting for model<span className="animate-pulse">...</span>
                        </span>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {/* Merged results */}
              {merged !== null && (
                <div className="border border-border border-t-0">
                  <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border bg-card">
                    <div className="size-1.5 bg-primary" />
                    <span className="font-mono text-[10px] font-black uppercase tracking-[3px] text-primary">
                      {merged.length} merged result{merged.length !== 1 ? "s" : ""}
                    </span>
                    <div className="h-px flex-1 bg-border" />
                  </div>

                  {merged.length === 0 ? (
                    <div className="px-4 py-6 text-center font-mono text-[11px] text-muted-foreground">
                      No results below the distance threshold.
                    </div>
                  ) : (
                    <div className="flex flex-col divide-y divide-border">
                      {merged.map((hit, i) => {
                        const score = distanceToScore(hit.distance)
                        const color = scoreColor(score)
                        return (
                          <Link
                            key={hit.id}
                            href={`/entries/${encodeURIComponent(hit.id)}`}
                            className="group relative flex items-start gap-4 px-4 py-4 bg-card hover:bg-card/80 transition-colors animate-in fade-in slide-in-from-bottom-2"
                            style={{ animationDelay: `${i * 30}ms`, animationFillMode: "both" }}
                          >
                            <div
                              className="absolute left-0 top-0 w-0.5 transition-all duration-700"
                              style={{
                                height: `${score}%`,
                                background: color,
                                boxShadow: `0 0 6px ${color}`,
                              }}
                            />
                            <div className="flex-1 min-w-0 pl-2 space-y-2">
                              <div className="flex items-center gap-3">
                                <span className="font-mono text-[10px] font-bold uppercase tracking-wider px-1.5 py-0.5 border border-border text-muted-foreground">
                                  {hit.source_id}
                                </span>
                                <span className="font-mono text-[10px] text-muted-foreground/70 truncate flex-1">
                                  {hit.id}
                                </span>
                                <div className="flex flex-col items-end gap-1 shrink-0">
                                  <span className="font-mono text-[10px] font-bold" style={{ color }}>
                                    {score}%
                                  </span>
                                  <div className="w-14 h-0.5 bg-border overflow-hidden">
                                    <div
                                      className="h-full transition-all duration-700"
                                      style={{ width: `${score}%`, background: color }}
                                    />
                                  </div>
                                </div>
                              </div>
                              <p className="text-sm text-foreground/80 group-hover:text-foreground leading-relaxed line-clamp-3 transition-colors">
                                {hit.text}
                              </p>
                            </div>
                            <ChevronRight className="size-4 text-muted-foreground/20 group-hover:text-primary transition-colors shrink-0 mt-0.5" />
                          </Link>
                        )
                      })}
                    </div>
                  )}
                </div>
              )}

            </div>
          </div>
        )}

      </div>
    </div>
  )
}
