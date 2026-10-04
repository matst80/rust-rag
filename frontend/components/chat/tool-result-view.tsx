"use client"

import { FileText, Search, ListTree, Database, Tag, CheckCircle2, AlertTriangle } from "lucide-react"
import { MarkdownView } from "@/components/entries/markdown-view"
import { Badge } from "@/components/ui/badge"

interface SearchHit {
  id: string
  source_id: string
  score: number
  snippet: string
}

interface EntryResult {
  id: string
  source_id: string
  path: string | null
  text: string
  metadata: Record<string, any>
}

interface ServerSearchHit {
  id: string
  text: string
  metadata: Record<string, any>
  source_id: string
  created_at: number
  distance: number
  retrievers: string[]
}

interface PathResult {
  source_id: string
  path: string
  count: number
}

export function ToolResultView({ name, result }: { name: string; result: string }) {
  if (!result) return null

  try {
    const data = JSON.parse(result)

    const isSearch = name === "search" || name === "search_entries"
    const searchHits = Array.isArray(data) ? data : data?.results

    if (isSearch && Array.isArray(searchHits)) {
      return (
        <div className="flex flex-col gap-3 py-1">
          {searchHits.map((r: any, i: number) => {
            const id = r.id
            const sourceId = r.source_id
            const score = r.score !== undefined ? r.score : (r.distance !== undefined ? 1 - r.distance : null)
            const snippet = r.snippet || (r.text ? r.text.slice(0, 280).replace(/\s+/g, " ") : "")
            const retrievers = r.retrievers

            return (
              <div key={i} className="group relative flex flex-col gap-2 p-3 bg-card border border-border/50 hover:border-primary/30 transition-all duration-200">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Database className="size-3 text-primary opacity-70" />
                    <span className="text-[10px] font-mono font-black text-primary uppercase tracking-wider">{id}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <Badge variant="outline" className="text-[9px] font-mono py-0 h-4 px-1.5 opacity-60">
                      {sourceId}
                    </Badge>
                    {score !== null && (
                      <span className="text-[9px] font-mono text-muted-foreground tabular-nums">
                        {(score * 100).toFixed(1)}% match
                      </span>
                    )}
                  </div>
                </div>
                <p className="text-[11px] leading-relaxed text-muted-foreground/80 italic line-clamp-3 pl-2 border-l-2 border-muted">
                  "{snippet}..."
                </p>
                {retrievers && retrievers.length > 0 && (
                  <div className="flex gap-1 mt-1">
                    {retrievers.map((ret: string) => (
                      <span key={ret} className="text-[8px] font-mono uppercase bg-primary/5 text-primary/60 px-1 border border-primary/10 rounded-[2px]">
                        {ret}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            )
          })}
          {searchHits.length === 0 && (
            <div className="p-4 text-center border border-dashed border-border rounded-sm">
              <p className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">No matching intelligence found</p>
            </div>
          )}
        </div>
      )
    }

    if (["get_entry", "get_item"].includes(name) && data.id) {
      const entry = data as EntryResult
      return (
        <div className="flex flex-col gap-3 p-4 bg-card border border-border/50">
          <div className="flex items-start justify-between pb-3 border-b border-border/30">
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-2">
                <FileText className="size-3.5 text-primary" />
                <span className="text-xs font-mono font-black uppercase tracking-widest">{entry.id}</span>
              </div>
              {entry.path && (
                <div className="flex items-center gap-1 text-[10px] text-muted-foreground opacity-60 font-mono">
                  <ListTree className="size-3" />
                  {entry.path}
                </div>
              )}
            </div>
            <Badge variant="secondary" className="text-[9px] font-mono uppercase tracking-tighter px-2 py-0.5">
              {entry.source_id}
            </Badge>
          </div>

          <div className="prose prose-sm dark:prose-invert max-w-none max-h-[400px] overflow-y-auto pr-2 custom-scrollbar">
            <MarkdownView content={entry.text} />
          </div>

          {(() => {
            const visibleMetadata = Object.entries(entry.metadata || {}).filter(
              ([k, v]) =>
                k !== "source_type" &&
                k !== "source_file" &&
                k !== "projection" &&
                typeof v !== "object"
            );
            if (visibleMetadata.length === 0) return null;
            return (
              <div className="mt-2 pt-3 border-t border-border/30">
                <div className="flex flex-wrap gap-1.5">
                  {visibleMetadata.map(([k, v], idx) => (
                    <div key={idx} className="flex items-center gap-1.5 px-2 py-0.5 bg-muted/50 border border-border/50 rounded-full">
                      <Tag className="size-2.5 text-primary/60" />
                      <span className="text-[9px] font-mono font-bold opacity-70">{k}:</span>
                      <span className="text-[9px] font-mono opacity-90 truncate max-w-[120px]">{String(v)}</span>
                    </div>
                  ))}
                </div>
              </div>
            );
          })()}
        </div>
      )
    }

    if (name === "list_categories" && Array.isArray(data?.categories)) {
      const categories: { source_id: string; item_count: number }[] = data.categories
      return (
        <div className="flex flex-wrap gap-1.5 py-1">
          {categories.map((c) => (
            <div
              key={c.source_id}
              className="flex items-center gap-1.5 border border-primary/20 bg-primary/5 px-2 py-1 transition-colors hover:border-primary/40"
            >
              <Tag className="size-2.5 text-primary" />
              <span className="font-mono text-[10px] font-bold uppercase tracking-wider">{c.source_id}</span>
              <span className="border-l border-border pl-1.5 font-mono text-[9px] tabular-nums text-primary/70">
                {c.item_count}
              </span>
            </div>
          ))}
          {categories.length === 0 && (
            <p className="w-full py-1 text-center font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
              No categories yet
            </p>
          )}
        </div>
      )
    }

    if (name === "list_items" && Array.isArray(data?.items)) {
      const items: (EntryResult & { created_at?: number; path?: string | null })[] = data.items
      const shown = items.slice(0, 25)
      return (
        <div className="flex flex-col gap-1.5 py-1">
          {shown.map((item) => (
            <div
              key={item.id}
              className="group flex flex-col gap-1 border border-border/50 bg-card p-2 transition-colors hover:border-primary/30"
            >
              <div className="flex items-center gap-2">
                <FileText className="size-3 shrink-0 text-primary/70" />
                <span className="truncate font-mono text-[10px] font-bold text-primary/90">{item.id}</span>
                <Badge variant="outline" className="ml-auto h-4 shrink-0 px-1.5 py-0 font-mono text-[9px] opacity-60">
                  {item.source_id}
                </Badge>
              </div>
              {item.path && (
                <div className="flex items-center gap-1 pl-5 font-mono text-[9px] text-muted-foreground">
                  <ListTree className="size-2.5" />
                  {item.path}
                </div>
              )}
              <p className="line-clamp-2 pl-5 text-[10px] leading-relaxed text-muted-foreground/80">
                {item.text?.slice(0, 240)}
              </p>
            </div>
          ))}
          {items.length > shown.length && (
            <p className="pt-1 text-center font-mono text-[9px] uppercase tracking-widest text-muted-foreground/60">
              showing {shown.length} of {data.total_count ?? items.length} items
            </p>
          )}
          {items.length === 0 && (
            <p className="w-full py-1 text-center font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
              No items found
            </p>
          )}
        </div>
      )
    }

    if ((name === "store_entry" || name === "update_item") && typeof data?.id === "string") {
      const chunkIds: string[] | undefined = data.chunk_ids
      return (
        <div className="flex flex-col gap-2 py-1">
          <div className="flex items-center gap-2 border border-green-500/25 bg-green-500/5 px-2.5 py-2">
            <CheckCircle2 className="size-3.5 shrink-0 text-green-500" />
            <span className="font-mono text-[10px] font-bold uppercase tracking-wider text-green-500">
              {name === "store_entry" ? "Entry stored" : "Entry updated"}
            </span>
            <span className="ml-auto truncate font-mono text-[10px] text-primary" title={data.id}>
              {data.id}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-1.5 pl-1">
            {data.source_id && (
              <Badge variant="outline" className="h-4 px-1.5 py-0 font-mono text-[9px] opacity-60">
                {data.source_id}
              </Badge>
            )}
            {chunkIds && chunkIds.length > 0 && (
              <span className="font-mono text-[9px] text-muted-foreground">
                split into {chunkIds.length} chunks
              </span>
            )}
          </div>
        </div>
      )
    }

    if (name === "delete_item" && typeof data?.id === "string") {
      return (
        <div className="flex items-center gap-2 py-1">
          {data.deleted ? (
            <>
              <CheckCircle2 className="size-3.5 shrink-0 text-green-500" />
              <span className="font-mono text-[10px] font-bold uppercase tracking-wider text-green-500">Deleted</span>
            </>
          ) : (
            <>
              <AlertTriangle className="size-3.5 shrink-0 text-muted-foreground" />
              <span className="font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                Not found
              </span>
            </>
          )}
          <span className="ml-auto truncate font-mono text-[10px] text-muted-foreground" title={data.id}>
            {data.id}
          </span>
        </div>
      )
    }

    if (name === "list_paths" && Array.isArray(data)) {
      return (
        <div className="grid grid-cols-1 gap-1.5 py-1">
          {data.map((p: PathResult, i) => (
            <div key={i} className="flex items-center justify-between p-2 bg-muted/20 border border-border/30 hover:bg-muted/40 transition-colors group">
              <div className="flex items-center gap-2">
                <ListTree className="size-3 text-muted-foreground group-hover:text-primary transition-colors" />
                <span className="text-[10px] font-mono text-muted-foreground group-hover:text-foreground transition-colors">{p.path}</span>
              </div>
              <div className="flex items-center gap-2">
                <Badge variant="outline" className="text-[8px] font-mono opacity-40 group-hover:opacity-100">{p.source_id}</Badge>
                <span className="text-[9px] font-mono font-bold text-primary tabular-nums">{p.count} items</span>
              </div>
            </div>
          ))}
        </div>
      )
    }

    // Default pretty JSON for unknown tools
    return (
      <div className="p-3 bg-muted/20 border border-border/30 overflow-auto max-h-[300px]">
        <pre className="text-[10px] leading-relaxed font-mono text-muted-foreground whitespace-pre-wrap">
          {JSON.stringify(data, null, 2)}
        </pre>
      </div>
    )
  } catch (e) {
    return (
      <div className="p-3 bg-muted/20 border border-border/30 overflow-auto max-h-[300px]">
        <pre className="text-[10px] leading-relaxed font-mono text-muted-foreground whitespace-pre-wrap">{result}</pre>
      </div>
    )
  }
}
