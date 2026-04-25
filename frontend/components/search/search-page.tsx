"use client"

import { useState, useCallback, useRef, useEffect } from "react"
import { Brain, Sparkles, X } from "lucide-react"
import { SearchInput } from "./search-input"
import { SearchResults } from "./search-results"
import { useSearch, api } from "@/lib/api"
import { cn } from "@/lib/utils"
import type { AssistedQueryRawResult, SearchResult } from "@/lib/api/types"

interface QueryBlock {
  index: number
  query: string
  results?: AssistedQueryRawResult[]
  status: "pending" | "done"
}

export function SearchPage({ defaultAssisted = false }: { defaultAssisted?: boolean }) {
  const [searchQuery, setSearchQuery] = useState("")
  const [submittedQuery, setSubmittedQuery] = useState("")
  const [categoryFilter, setCategoryFilter] = useState<string | null>(null)
  const [isAssisted, setIsAssisted] = useState(defaultAssisted)

  // Assisted mode state
  const [isStreaming, setIsStreaming] = useState(false)
  const [queries, setQueries] = useState<QueryBlock[]>([])
  const [merged, setMerged] = useState<SearchResult[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const abortRef = useRef<AbortController | null>(null)

  // Basic search hook
  const { data: basicResults, isLoading: isBasicLoading } = useSearch(
    !isAssisted ? submittedQuery : "",
    categoryFilter ?? undefined
  )

  const runAssisted = async (q: string) => {
    if (!q.trim() || isStreaming) return
    setIsStreaming(true)
    setQueries([])
    setMerged(null)
    setError(null)
    setSubmittedQuery(q)

    abortRef.current = new AbortController()
    try {
      await api.query.assisted(
        { query: q, top_k: 8 },
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
          onMerged: (event) => {
            const normalized: SearchResult[] = event.results.map(r => ({
              ...r,
              score: Math.max(0, 1 - r.distance)
            }))
            setMerged(normalized)
          },
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

  const cancelAssisted = () => {
    abortRef.current?.abort()
    setIsStreaming(false)
  }

  const handleSubmit = useCallback(() => {
    const q = searchQuery.trim()
    if (!q) return

    if (isAssisted) {
      runAssisted(q)
    } else {
      setSubmittedQuery(q)
      setMerged(null)
      setQueries([])
    }
  }, [searchQuery, isAssisted])

  const isLoading = isAssisted ? isStreaming : isBasicLoading
  const hasResults = isAssisted ? merged !== null : !!basicResults

  return (
    <div className="relative flex w-full min-h-[calc(100vh-3rem)] flex-col overflow-hidden">
      <div className="mx-auto w-full max-w-3xl flex-1 flex flex-col px-6">
        {!submittedQuery ? (
          <div className="flex flex-1 flex-col items-center justify-center -mt-16">

            <div className="animate-in fade-in zoom-in duration-700 fill-mode-both mb-8">
              {isAssisted ? (
                <Sparkles
                  className="size-16 text-primary"
                  style={{ filter: "drop-shadow(0 0 20px oklch(0.9 0.148 196.3 / 0.5))" }}
                />
              ) : (
                <Brain
                  className="size-16 text-primary"
                  style={{ filter: "drop-shadow(0 0 20px oklch(0.9 0.148 196.3 / 0.5))" }}
                />
              )}
            </div>

            <p className="mb-2 font-mono text-[10px] font-black uppercase tracking-[5px] text-primary animate-in fade-in duration-500 delay-100 fill-mode-both">
              {isAssisted ? "AI-Assisted" : "rust-rag"}
            </p>

            <h1 className="mb-4 text-center text-4xl md:text-5xl font-extrabold tracking-tight text-foreground animate-in fade-in slide-in-from-bottom-4 duration-700 delay-200 fill-mode-both">
              {isAssisted ? "Deep Search" : "Search Intelligence"}
            </h1>

            <p className="mb-12 text-center text-muted-foreground text-base max-w-md animate-in fade-in slide-in-from-bottom-4 duration-700 delay-300 fill-mode-both leading-relaxed">
              {isAssisted
                ? "Your question is expanded into multiple sub-queries and merged into a single ranked result."
                : "Explore your knowledge base with semantic precision."}
            </p>

            <div className="w-full animate-in fade-in slide-in-from-bottom-8 duration-700 delay-500 fill-mode-both">
              <SearchInput
                query={searchQuery}
                onQueryChange={setSearchQuery}
                categoryFilter={categoryFilter}
                onCategoryFilterChange={setCategoryFilter}
                isAssisted={isAssisted}
                onAssistedChange={setIsAssisted}
                onSubmit={handleSubmit}
                isLoading={isLoading}
              />
            </div>
          </div>
        ) : (
          <div className="flex flex-1 flex-col gap-6 py-8 animate-in fade-in slide-in-from-bottom-4 duration-500 fill-mode-both">
            <div className="sticky top-12 z-40 pb-6 pt-2 -mx-6 px-6 border-b border-border bg-background/95 backdrop-blur">
              <SearchInput
                query={searchQuery}
                onQueryChange={setSearchQuery}
                categoryFilter={categoryFilter}
                onCategoryFilterChange={setCategoryFilter}
                isAssisted={isAssisted}
                onAssistedChange={setIsAssisted}
                onSubmit={handleSubmit}
                isLoading={isLoading}
              />
            </div>

            <div className="flex-1 w-full space-y-6">
              {/* Assisted Sub-queries */}
              {isAssisted && (isStreaming || queries.length > 0) && (
                <div className="border border-border bg-background mb-0 animate-in fade-in duration-500">
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
                    {isStreaming && (
                      <button
                        onClick={cancelAssisted}
                        className="ml-2 text-muted-foreground hover:text-destructive transition-colors"
                        title="Cancel"
                      >
                        <X className="size-3" />
                      </button>
                    )}
                  </div>

                  <div className="flex flex-col divide-y divide-border max-h-48 overflow-y-auto">
                    {queries.map((block) => {
                      const hits = block.results?.length ?? 0
                      return (
                        <div
                          key={block.index}
                          className={cn(
                            "flex items-center gap-3 px-4 py-2 transition-colors",
                            block.status === "pending" ? "bg-primary/3" : "bg-transparent"
                          )}
                        >
                          <span className="font-mono text-[10px] text-muted-foreground/80 w-5 shrink-0 text-right">
                            {block.index + 1}
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
                  </div>
                </div>
              )}

              {error && (
                <div className="px-4 py-3 font-mono text-[11px] text-destructive border border-destructive/20 bg-destructive/5 animate-in fade-in">
                  Error: {error}
                </div>
              )}

              {/* Final Results */}
              {isLoading && !isAssisted ? (
                <div className="flex flex-col items-center justify-center py-24 gap-4">
                  <div className="size-10 animate-spin border-2 border-border border-t-primary" />
                  <p className="font-mono text-[10px] font-black uppercase tracking-[4px] text-muted-foreground animate-pulse">
                    Scanning...
                  </p>
                </div>
              ) : isAssisted ? (
                merged && (
                  <SearchResults
                    results={merged}
                    query={submittedQuery}
                    isAssisted={true}
                  />
                )
              ) : basicResults ? (
                <SearchResults
                  results={basicResults.results}
                  related={basicResults.related}
                  query={submittedQuery}
                />
              ) : null}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
