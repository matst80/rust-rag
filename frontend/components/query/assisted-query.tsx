"use client"

import { useRef, useState } from "react"
import { Loader2, Sparkles, Search, ListOrdered, Hash } from "lucide-react"
import { api } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Card } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
import type { AssistedQueryRawResult } from "@/lib/api/types"

interface QueryBlock {
  index: number
  query: string
  results?: AssistedQueryRawResult[]
  status: "pending" | "done"
}

export function AssistedQueryView() {
  const [query, setQuery] = useState("")
  const [sourceId, setSourceId] = useState("")
  const [topK, setTopK] = useState(5)
  const [isStreaming, setIsStreaming] = useState(false)
  const [queries, setQueries] = useState<QueryBlock[]>([])
  const [merged, setMerged] = useState<AssistedQueryRawResult[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const abortRef = useRef<AbortController | null>(null)

  const run = async () => {
    if (!query.trim() || isStreaming) return
    setIsStreaming(true)
    setQueries([])
    setMerged(null)
    setError(null)

    abortRef.current = new AbortController()
    try {
      await api.query.assisted(
        {
          query: query.trim(),
          source_id: sourceId.trim() || undefined,
          top_k: topK,
        },
        {
          onQueries: (event) => {
            setQueries(
              event.queries.map((q, index) => ({
                index,
                query: q,
                status: "pending",
              }))
            )
          },
          onResult: (event) => {
            setQueries((prev) => {
              const next = [...prev]
              const target = next.find((item) => item.index === event.index)
              if (target) {
                target.results = event.results
                target.status = "done"
              }
              return next
            })
          },
          onMerged: (event) => {
            setMerged(event.results)
          },
          onError: (err) => {
            setError(err.error.message)
          },
          onDone: () => {
            setIsStreaming(false)
          },
        },
        { signal: abortRef.current.signal }
      )
    } catch (err: any) {
      if (err?.name !== "AbortError") {
        setError(err?.message ?? String(err))
      }
      setIsStreaming(false)
    }
  }

  const cancel = () => {
    abortRef.current?.abort()
    setIsStreaming(false)
  }

  return (
    <div className="flex flex-col gap-6 max-w-5xl mx-auto w-full">
      <Card className="p-4 md:p-6 flex flex-col gap-4">
        <div className="flex items-center gap-2">
          <Sparkles className="size-5 text-primary" />
          <h2 className="font-semibold tracking-tight">LLM-assisted query</h2>
          <span className="text-xs text-muted-foreground ml-2">
            Expands your question into multiple focused searches
          </span>
        </div>
        <form
          onSubmit={(e) => {
            e.preventDefault()
            run()
          }}
          className="flex flex-col gap-3"
        >
          <Input
            placeholder="What do you want to find?"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            disabled={isStreaming}
            className="h-11"
          />
          <div className="flex flex-col sm:flex-row gap-3">
            <Input
              placeholder="source_id (optional)"
              value={sourceId}
              onChange={(e) => setSourceId(e.target.value)}
              disabled={isStreaming}
              className="flex-1"
            />
            <Input
              type="number"
              min={1}
              max={25}
              value={topK}
              onChange={(e) => setTopK(Number(e.target.value) || 5)}
              disabled={isStreaming}
              className="w-24"
            />
            {isStreaming ? (
              <Button type="button" variant="secondary" onClick={cancel}>
                Cancel
              </Button>
            ) : (
              <Button type="submit" disabled={!query.trim()}>
                <Search className="size-4 mr-2" /> Run
              </Button>
            )}
          </div>
        </form>
        {error && (
          <div className="text-sm text-destructive border border-destructive/40 bg-destructive/5 rounded-md px-3 py-2">
            {error}
          </div>
        )}
      </Card>

      {(isStreaming || queries.length > 0) && (
        <Card className="p-4 md:p-6 flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <ListOrdered className="size-4 text-primary" />
            <h3 className="font-semibold text-sm">Generated sub-queries</h3>
            {isStreaming && queries.length === 0 && (
              <Loader2 className="size-4 animate-spin ml-auto text-muted-foreground" />
            )}
          </div>
          {queries.length === 0 ? (
            <p className="text-xs text-muted-foreground">Waiting for the model…</p>
          ) : (
            <ol className="flex flex-col gap-2">
              {queries.map((block) => (
                <li
                  key={block.index}
                  className={cn(
                    "flex items-start gap-3 rounded-md border p-3 text-sm",
                    block.status === "done"
                      ? "border-muted bg-muted/30"
                      : "border-primary/20 bg-primary/5"
                  )}
                >
                  <Badge variant="secondary" className="mt-0.5">
                    #{block.index + 1}
                  </Badge>
                  <div className="flex-1 min-w-0">
                    <p className="font-mono">{block.query}</p>
                    {block.status === "pending" ? (
                      <div className="flex items-center gap-2 text-xs text-muted-foreground mt-1">
                        <Loader2 className="size-3 animate-spin" /> Searching…
                      </div>
                    ) : block.results && block.results.length > 0 ? (
                      <p className="text-xs text-muted-foreground mt-1">
                        {block.results.length} hit{block.results.length === 1 ? "" : "s"}
                      </p>
                    ) : (
                      <p className="text-xs text-muted-foreground mt-1">No hits</p>
                    )}
                  </div>
                </li>
              ))}
            </ol>
          )}
        </Card>
      )}

      {merged && (
        <Card className="p-4 md:p-6 flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <Hash className="size-4 text-primary" />
            <h3 className="font-semibold text-sm">
              Merged top {merged.length} result{merged.length === 1 ? "" : "s"}
            </h3>
          </div>
          {merged.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              No results below the distance threshold.
            </p>
          ) : (
            <ul className="flex flex-col gap-3">
              {merged.map((hit) => (
                <li
                  key={hit.id}
                  className="rounded-md border bg-muted/20 p-3 flex flex-col gap-2"
                >
                  <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
                    <div className="flex items-center gap-2 min-w-0">
                      <Badge variant="outline" className="font-mono">
                        {hit.source_id}
                      </Badge>
                      <span className="font-mono truncate" title={hit.id}>
                        {hit.id}
                      </span>
                    </div>
                    <span>distance {hit.distance.toFixed(3)}</span>
                  </div>
                  <p className="text-sm whitespace-pre-wrap">{hit.text}</p>
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}
    </div>
  )
}
