"use client"

import Link from "next/link"
import { FileText, ExternalLink, Link2 } from "lucide-react"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
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
      <div className="flex flex-col items-center justify-center py-24 text-center animate-in fade-in slide-in-from-bottom-8 duration-1000">
        <div className="mb-8 flex size-24 items-center justify-center rounded-[2.5rem] bg-muted/10 border border-muted/20 shadow-inner group overflow-hidden relative">
          <div className="absolute inset-0 bg-primary/5 group-hover:bg-primary/10 transition-colors" />
          <FileText className="size-10 text-muted-foreground/30 group-hover:scale-110 transition-transform duration-500" />
        </div>
        <h3 className="mb-4 text-3xl font-extrabold tracking-tight bg-gradient-to-b from-foreground to-foreground/60 bg-clip-text text-transparent"> No results match</h3>
        <p className="text-muted-foreground max-w-sm mx-auto text-lg leading-relaxed font-medium">
          We couldn't find any intelligence fragments matching &ldquo;<span className="text-primary">{query}</span>&rdquo;
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-10 animate-in fade-in slide-in-from-bottom-4 duration-700">
      <div className="flex items-center gap-6">
        <div className="flex items-center gap-3 px-4 py-2 rounded-2xl bg-primary/5 border border-primary/10 backdrop-blur-sm shadow-sm transition-all hover:bg-primary/10">
          <div className="size-2 rounded-full bg-primary animate-pulse" />
          <p className="text-[11px] font-black uppercase tracking-[0.25em] text-primary/80 whitespace-nowrap">
            Retrieved {results.length} Memory Fragment{results.length !== 1 ? "s" : ""}
          </p>
        </div>
        <div className="h-px flex-1 bg-gradient-to-r from-muted/40 to-transparent" />
      </div>
      
      <div className="grid gap-8">
        {results.map((result, index) => (
          <EntryCard
            key={result.id}
            entry={result}
            index={index}
            showScore={true}
          />
        ))}
      </div>

      {related.length > 0 && (
        <div className="flex flex-col gap-6 pt-4">
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-3 px-4 py-2 rounded-2xl bg-amber-500/5 border border-amber-500/20 backdrop-blur-sm shadow-sm">
              <Link2 className="size-3.5 text-amber-500/80" />
              <p className="text-[11px] font-black uppercase tracking-[0.25em] text-amber-500/80 whitespace-nowrap">
                You Also Linked {related.length} Fragment{related.length !== 1 ? "s" : ""}
              </p>
            </div>
            <div className="h-px flex-1 bg-gradient-to-r from-amber-500/20 to-transparent" />
          </div>

          <div className="grid gap-4">
            {related.map((item, index) => (
              <div
                key={item.id}
                className="relative rounded-[2rem] border border-dashed border-amber-500/20 bg-amber-500/[0.02]"
              >
                {item.relation && (
                  <div className="absolute -top-2.5 left-8 z-10 px-2.5 py-0.5 rounded-md bg-background border border-amber-500/30 text-[9px] font-black uppercase tracking-wider text-amber-500/80">
                    {item.relation}
                  </div>
                )}
                <EntryCard
                  entry={item}
                  index={index}
                  showScore={true}
                />
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="flex items-center justify-center pt-10">
        <div className="h-px w-10 bg-muted/20" />
        <p className="px-6 text-[10px] font-bold uppercase tracking-[0.3em] text-muted-foreground/20">
          End of results
        </p>
        <div className="h-px w-10 bg-muted/20" />
      </div>
    </div>
  )
}


