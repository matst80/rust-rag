"use client"

import { FileText, Link2 } from "lucide-react"
import type { SearchResult, RelatedResult } from "@/lib/api"
import { EntryCard } from "../entries/entry-card"

interface SearchResultsProps {
  results: SearchResult[]
  related?: RelatedResult[]
  query: string
}

export function SearchResults({ results, related = [], query }: SearchResultsProps) {
  if (results.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-24 text-center animate-in fade-in slide-in-from-bottom-4 duration-700">
        <div className="mb-6 flex size-20 items-center justify-center border border-border bg-card">
          <FileText className="size-8 text-muted-foreground/60" />
        </div>
        <h3 className="mb-3 text-2xl font-extrabold tracking-tight">No results match</h3>
        <p className="text-muted-foreground max-w-sm text-sm leading-relaxed">
          No fragments matching{" "}
          <span className="font-mono text-primary">&ldquo;{query}&rdquo;</span>
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-8 animate-in fade-in slide-in-from-bottom-4 duration-700">
      {/* Results header */}
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-2 px-3 py-1.5 border border-primary/20 bg-primary/5">
          <div className="size-1.5 bg-primary animate-pulse" />
          <p className="font-mono text-[10px] font-black uppercase tracking-[3px] text-primary/80">
            {results.length} fragment{results.length !== 1 ? "s" : ""}
          </p>
        </div>
        <div className="h-px flex-1 bg-border" />
      </div>

      <div className="flex flex-col divide-y divide-border border border-border">
        {results.map((result, index) => (
          <EntryCard key={result.id} entry={result} index={index} showScore />
        ))}
      </div>

      {/* Related */}
      {related.length > 0 && (
        <div className="flex flex-col gap-4">
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2 px-3 py-1.5 border border-border bg-card">
              <Link2 className="size-3 text-muted-foreground" />
              <p className="font-mono text-[10px] font-black uppercase tracking-[3px] text-muted-foreground">
                {related.length} linked
              </p>
            </div>
            <div className="h-px flex-1 bg-border" />
          </div>

          <div className="flex flex-col divide-y divide-border border border-dashed border-border">
            {related.map((item, index) => (
              <div key={item.id} className="relative">
                {item.relation && (
                  <div className="absolute top-3 right-3 z-10 px-2 py-0.5 border border-border bg-background font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                    {item.relation}
                  </div>
                )}
                <EntryCard entry={item} index={index} showScore />
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="flex items-center justify-center py-6">
        <div className="h-px w-8 bg-border" />
        <p className="px-4 font-mono text-[10px] font-bold uppercase tracking-[4px] text-muted-foreground/60">
          end
        </p>
        <div className="h-px w-8 bg-border" />
      </div>
    </div>
  )
}
