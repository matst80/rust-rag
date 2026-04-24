import { useState } from "react"
import { useSWRConfig } from "swr"
import { AlertTriangle, Scissors, Sparkles, ChevronLeft, ChevronRight, Loader2, ExternalLink } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { useLargeItems, useRechunkItem, useLlmRechunkItem } from "@/lib/api"
import type { Entry } from "@/lib/api"

const PAGE_SIZE = 20

function ItemActions({ item, onDone }: { item: Entry; onDone: () => void }) {
  const { trigger: rechunk, isMutating: isRechunking } = useRechunkItem(item.id)
  const { trigger: llmRechunk, isMutating: isLlmRechunking } = useLlmRechunkItem(item.id)
  const isBusy = isRechunking || isLlmRechunking

  return (
    <div className="flex items-center gap-2 shrink-0">
      <Button
        size="sm"
        variant="ghost"
        className="rounded-xl gap-1.5 h-8 px-3 text-xs"
        asChild
      >
        <a href={`/entries/?id=${encodeURIComponent(item.id)}`}>
          <ExternalLink className="size-3" />
          Edit
        </a>
      </Button>

      <Button
        size="sm"
        variant="outline"
        className="rounded-xl gap-1.5 h-8 px-3 text-xs"
        disabled={isBusy}
        onClick={async () => {
          await rechunk({})
          onDone()
        }}
      >
        {isRechunking ? (
          <Loader2 className="size-3 animate-spin" />
        ) : (
          <Scissors className="size-3" />
        )}
        {isRechunking ? "Chunking…" : "Split"}
      </Button>

      <Button
        size="sm"
        variant="outline"
        className="rounded-xl gap-1.5 h-8 px-3 text-xs text-purple-600 border-purple-500/30 hover:bg-purple-500/5"
        disabled={isBusy}
        onClick={async () => {
          await llmRechunk({})
          onDone()
        }}
      >
        {isLlmRechunking ? (
          <Loader2 className="size-3 animate-spin" />
        ) : (
          <Sparkles className="size-3" />
        )}
        {isLlmRechunking ? "Thinking…" : "LLM Split"}
      </Button>
    </div>
  )
}

export function LargeItemsPanel() {
  const [page, setPage] = useState(1)
  const { mutate } = useSWRConfig()

  const { data, isLoading } = useLargeItems({
    limit: PAGE_SIZE,
    offset: (page - 1) * PAGE_SIZE,
  })

  const items = data?.items ?? []
  const totalCount = data?.total_count ?? 0
  const totalPages = Math.ceil(totalCount / PAGE_SIZE)

  const handleDone = () => {
    mutate((key) => Array.isArray(key) && key[0] === "large-items")
    mutate("categories")
    mutate((key) => Array.isArray(key) && key[0] === "items")
  }

  if (!isLoading && totalCount === 0) return null

  return (
    <div className="flex flex-col gap-4 px-8 pt-8 md:px-10 md:pt-10">
      <div className="flex items-center gap-3">
        <div className="flex size-9 items-center justify-center rounded-xl bg-amber-500/10 border border-amber-500/20">
          <AlertTriangle className="size-4 text-amber-500" />
        </div>
        <div>
          <h2 className="text-lg font-bold tracking-tight">Oversized Entries</h2>
          <p className="text-xs text-muted-foreground">
            {isLoading
              ? "Loading…"
              : `${totalCount} oversized entr${totalCount === 1 ? "y" : "ies"} — split for better retrieval`}
          </p>
        </div>
      </div>

      {isLoading ? (
        <div className="flex flex-col gap-2">
          {[1, 2, 3].map((i) => (
            <div key={i} className="h-16 animate-pulse rounded-2xl bg-muted/10 border border-muted/5" />
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          {items.map((item) => (
            <div
              key={item.id}
              className="flex items-center gap-4 rounded-2xl border border-amber-500/10 bg-amber-500/5 px-5 py-3"
            >
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1 flex-wrap">
                  <span className="font-mono text-xs font-semibold text-muted-foreground truncate max-w-[200px]">
                    {item.id}
                  </span>
                  <Badge variant="secondary" className="shrink-0 text-[10px] font-bold">
                    {item.source_id}
                  </Badge>
                  <Badge variant="outline" className="shrink-0 text-[10px] font-bold text-amber-600 border-amber-500/30 bg-amber-500/5">
                    {item.text.length.toLocaleString()} chars
                  </Badge>
                  <span className="text-[10px] text-muted-foreground/50 shrink-0">
                    oversized
                  </span>
                </div>
                <p className="text-xs text-muted-foreground line-clamp-1">
                  {item.text}
                </p>
              </div>
              <ItemActions item={item} onDone={handleDone} />
            </div>
          ))}
        </div>
      )}

      {totalPages > 1 && (
        <div className="flex items-center justify-between pt-1 pb-2">
          <span className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
            Page {page} of {totalPages}
          </span>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="icon"
              className="size-8 rounded-xl"
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={page === 1}
            >
              <ChevronLeft className="size-3.5" />
            </Button>
            <Button
              variant="outline"
              size="icon"
              className="size-8 rounded-xl"
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              disabled={page === totalPages}
            >
              <ChevronRight className="size-3.5" />
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}
