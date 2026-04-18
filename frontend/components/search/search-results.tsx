"use client"

import Link from "next/link"
import { FileText, ExternalLink } from "lucide-react"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type { SearchResult } from "@/lib/api"

interface SearchResultsProps {
  results: SearchResult[]
  query: string
}

export function SearchResults({ results, query }: SearchResultsProps) {
  if (results.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <FileText className="mb-4 size-12 text-muted-foreground" />
        <h3 className="mb-2 text-lg font-medium">No results found</h3>
        <p className="text-sm text-muted-foreground">
          No entries match your search for &ldquo;{query}&rdquo;
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        Found {results.length} result{results.length !== 1 ? "s" : ""} for &ldquo;{query}&rdquo;
      </p>
      {results.map((result) => (
        <SearchResultCard key={result.id} result={result} />
      ))}
    </div>
  )
}

function SearchResultCard({ result }: { result: SearchResult }) {
  const scorePercent = Math.round(result.score * 100)
  
  const getScoreColor = (score: number) => {
    if (score > 0.8) return 'bg-emerald-500'
    if (score > 0.5) return 'bg-amber-500'
    return 'bg-muted-foreground'
  }

  const getSourceVariant = (source: string) => {
    const s = source.toLowerCase()
    if (s.includes('manual')) return 'warning'
    if (s.includes('auto')) return 'info'
    if (s.includes('imported')) return 'purple'
    return 'outline'
  }

  return (
    <Card className="group relative overflow-hidden transition-all hover:shadow-lg border-muted/60">
      <div className={cn("absolute top-0 left-0 h-1 transition-all duration-500", getScoreColor(result.score))} style={{ width: `${scorePercent}%` }} />
      <CardHeader className="pb-3 pt-6">
        <div className="flex items-start justify-between gap-4">
          <div className="flex flex-col gap-1">
            <CardTitle className="line-clamp-1 text-base font-bold tracking-tight">
              <Link href={`/entries/${encodeURIComponent(result.id)}`} className="hover:text-primary transition-colors">
                {result.id}
              </Link>
            </CardTitle>
            <div className="flex items-center gap-2">
              <span className="text-[10px] font-bold uppercase text-muted-foreground/70">Relevance</span>
              <span className={cn("text-xs font-bold", result.score > 0.5 ? "text-primary" : "text-muted-foreground")}>
                {scorePercent}%
              </span>
            </div>
          </div>
          <Badge variant={getSourceVariant(result.source_id)} className="px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider">
            {result.source_id}
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <p className="mb-4 line-clamp-3 text-sm text-muted-foreground leading-relaxed">
          {result.text}
        </p>
        
        {Object.keys(result.metadata).length > 0 && (
          <div className="mb-4 flex flex-wrap gap-1.5">
            {Object.entries(result.metadata)
              .slice(0, 5)
              .map(([key, value]) => (
                <Badge key={key} variant="secondary" className="px-1.5 py-0 text-[10px] bg-secondary/50 border-none font-medium">
                  <span className="opacity-60">{key}:</span> {String(value)}
                </Badge>
              ))}
          </div>
        )}
        
        <div className="flex justify-start border-t border-muted/30 pt-3">
          <Button variant="link" size="sm" asChild className="h-auto p-0 text-primary font-semibold">
            <Link href={`/entries/${encodeURIComponent(result.id)}`}>
              View details
              <ExternalLink className="ml-1.5 size-3" />
            </Link>
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
