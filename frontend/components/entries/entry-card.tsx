"use client"

import Link from "next/link"
import { Share2, ChevronRight, Database, Clock } from "lucide-react"
import { cn, formatRelativeTime, stringToHslColor } from "@/lib/utils"
import { ComboButton } from "@/components/ui/combo-button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { MoreVertical } from "lucide-react"
import type { Entry, SearchResult } from "@/lib/api"

interface EntryCardProps {
  entry: Entry | SearchResult
  index?: number
  onDelete?: (id: string) => Promise<void> | void
  showScore?: boolean
}

function scoreColor(pct: number) {
  if (pct >= 80) return "oklch(0.916 0.175 156.8)"  // success green
  if (pct >= 50) return "oklch(0.9 0.148 196.3)"    // cyan
  return "oklch(0.42 0 0)"                           // muted
}

export function EntryCard({ entry, index = 0, onDelete, showScore = false }: EntryCardProps) {
  const isSearchResult = "score" in entry
  const score = isSearchResult ? (entry as SearchResult).score : null
  const scorePercent = score !== null ? Math.round(score * 100) : null
  const sourceColor = stringToHslColor(entry.source_id, 60, 45)

  return (
    <Link
      href={`/entries/${encodeURIComponent(entry.id)}`}
      className={cn(
        "group relative flex items-start gap-6 p-5 overflow-hidden transition-colors duration-200",
        "bg-card hover:bg-card/80",
        "animate-in fade-in slide-in-from-bottom-2"
      )}
      style={{ animationDelay: `${index * 25}ms`, animationFillMode: "both" }}
    >
      {/* Score bar on left edge */}
      {showScore && scorePercent !== null && (
        <div
          className="absolute left-0 top-0 w-0.5 transition-all duration-500"
          style={{
            height: `${scorePercent}%`,
            background: scoreColor(scorePercent),
            boxShadow: `0 0 8px ${scoreColor(scorePercent)}`,
          }}
        />
      )}

      {/* Content */}
      <div className="flex-1 min-w-0 space-y-2.5 pl-2">
        {/* Meta row */}
        <div className="flex items-center gap-3 flex-wrap">
          <div
            className="flex items-center gap-1 px-1.5 py-0.5 border font-mono text-[10px] font-bold uppercase tracking-wider"
            style={{
              backgroundColor: `${sourceColor}12`,
              borderColor: `${sourceColor}28`,
              color: sourceColor,
            }}
          >
            <Database className="size-2.5" />
            {entry.source_id}
          </div>

          <div className="flex items-center gap-1 font-mono text-[10px] text-muted-foreground uppercase tracking-wider">
            <Clock className="size-2.5" />
            {formatRelativeTime(entry.created_at)}
          </div>

          {showScore && scorePercent !== null && (
            <div
              className="font-mono text-[10px] font-bold uppercase tracking-wider ml-auto"
              style={{ color: scoreColor(scorePercent) }}
            >
              {scorePercent}%
            </div>
          )}
        </div>

        {/* Text */}
        <p className="line-clamp-2 text-sm text-foreground/80 group-hover:text-foreground transition-colors leading-relaxed">
          {entry.text}
        </p>

        {/* Metadata tags */}
        {Object.keys(entry.metadata).length > 0 && (
          <div className="flex gap-1.5 flex-wrap">
            {Object.entries(entry.metadata)
              .slice(0, 3)
              .map(([key, value]) => (
                <div
                  key={key}
                  className="flex items-center border border-border px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                >
                  <span className="opacity-40 mr-1">{key}:</span>
                  <span className="truncate max-w-28">{String(value)}</span>
                </div>
              ))}
            {Object.keys(entry.metadata).length > 3 && (
              <div className="border border-border px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                +{Object.keys(entry.metadata).length - 3}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Actions */}
      <div
        className="flex items-center gap-1 shrink-0"
        onClick={(e) => { e.preventDefault(); e.stopPropagation() }}
      >
        {!showScore && onDelete && (
          <div className="flex items-center opacity-0 group-hover:opacity-100 transition-opacity">
            <ComboButton onConfirm={() => onDelete(entry.id)} className="size-8" />
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button className="size-8 flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors">
                  <MoreVertical className="size-4" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-36">
                <DropdownMenuItem className="font-mono text-xs cursor-pointer">
                  <Share2 className="mr-2 size-3.5" /> Share
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )}

        <ChevronRight className="size-4 text-muted-foreground/30 group-hover:text-primary transition-colors" />
      </div>
    </Link>
  )
}
