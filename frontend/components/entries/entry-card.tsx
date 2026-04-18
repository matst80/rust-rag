"use client"

import Link from "next/link"
import { 
  MoreVertical, 
  Share2, 
  Trash2, 
  ChevronRight,
  Database,
  Clock,
  Layers
} from "lucide-react"
import { cn, formatRelativeTime, stringToHslColor } from "@/lib/utils"
import { ComboButton } from "@/components/ui/combo-button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { Entry, SearchResult } from "@/lib/api"

interface EntryCardProps {
  entry: Entry | SearchResult
  index?: number
  onDelete?: (id: string) => Promise<void> | void
  showScore?: boolean
}

export function EntryCard({ 
  entry, 
  index = 0, 
  onDelete, 
  showScore = false 
}: EntryCardProps) {
  const isSearchResult = 'score' in entry
  const score = isSearchResult ? (entry as SearchResult).score : null
  const scorePercent = score !== null ? Math.round(score * 100) : null

  const sourceColor = stringToHslColor(entry.source_id, 60, 45)

  return (
    <Link
      href={`/entries/${encodeURIComponent(entry.id)}`}
      className={cn(
        "group relative flex items-center gap-8 p-6 md:p-8 overflow-hidden transition-all duration-500",
        "bg-card/30 hover:bg-card/60",
        "border-b border-muted/10 last:border-0",
        "hover:shadow-[0_20px_50px_-20px_rgba(0,0,0,0.3)]",
        "rounded-[2rem] md:rounded-[2.5rem] backdrop-blur-md",
        "animate-in fade-in slide-in-from-bottom-4"
      )}
      style={{ animationDelay: `${index * 30}ms`, animationFillMode: 'both' }}
    >
      {/* Main Content Column */}
      <div className="flex-1 min-w-0 z-20 space-y-3">
        <div className="flex items-center gap-3 text-muted-foreground/60">
          {/* Color-coded Source Badge */}
          <div 
            className="flex items-center gap-1.5 px-2 py-0.5 rounded-lg border text-[9px] font-black uppercase tracking-wider"
            style={{ 
              backgroundColor: `${sourceColor}15`, 
              borderColor: `${sourceColor}30`,
              color: sourceColor 
            }}
          >
            <Database className="size-2.5" />
            <span>{entry.source_id}</span>
          </div>

          <div className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider">
            <Clock className="size-3" />
            <span>{formatRelativeTime(entry.created_at)}</span>
          </div>

          {showScore && score !== null && (
            <div className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider text-primary">
              <Layers className="size-3" />
              <span>{scorePercent}% Relevance</span>
            </div>
          )}
        </div>
        
        <p className="line-clamp-2 text-sm md:text-base text-foreground/85 group-hover:text-foreground transition-colors leading-relaxed">
          {entry.text}
        </p>

        {/* Dynamic Metadata Row */}
        {Object.keys(entry.metadata).length > 0 && (
          <div className="flex gap-2 flex-wrap pt-0.5">
            {Object.entries(entry.metadata)
              .slice(0, 3)
              .map(([key, value]) => (
                <div 
                  key={key} 
                  className="flex items-center rounded-lg bg-primary/5 border border-primary/10 px-2 py-0.5 text-[9px] font-bold text-primary/60"
                >
                  <span className="opacity-40 mr-1">{key}:</span>
                  <span className="truncate max-w-[120px]">{String(value)}</span>
                </div>
              ))}
            {Object.keys(entry.metadata).length > 3 && (
              <div className="flex items-center rounded-lg bg-muted/20 px-2 py-0.5 text-[9px] font-bold text-muted-foreground/60 transition-colors group-hover:bg-muted/30">
                +{Object.keys(entry.metadata).length - 3}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Actions Column */}
      <div className="flex items-center gap-3 z-30" onClick={(e) => {
        e.preventDefault()
        e.stopPropagation()
      }}>
        {!showScore && onDelete && (
          <div className="flex items-center opacity-0 group-hover:opacity-100 transition-all duration-300">
            <ComboButton 
              onConfirm={() => onDelete(entry.id)}
              className="size-10 rounded-2xl hover:bg-destructive/5"
            />
            
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button 
                  className="size-10 flex items-center justify-center rounded-2xl transition-all hover:bg-primary/10 text-muted-foreground hover:text-primary"
                  onClick={(e) => e.stopPropagation()}
                >
                  <MoreVertical className="size-5" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="rounded-[1.2rem] border-muted/40 shadow-2xl p-1.5 min-w-[160px] backdrop-blur-xl bg-background/80" onClick={(e) => e.stopPropagation()}>
                <DropdownMenuItem className="text-xs font-bold rounded-lg px-3 py-2.5 cursor-pointer">
                  <Share2 className="mr-2.5 size-4 opacity-70" /> Share
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )}
        
        <div className="size-10 flex items-center justify-center rounded-2xl text-primary/40 group-hover:text-primary group-hover:bg-primary/5 transition-all -translate-x-1 group-hover:translate-x-0 opacity-0 group-hover:opacity-100">
          <ChevronRight className="size-5" />
        </div>
      </div>

      {/* Subtle Background accent */}
      <div className="absolute top-0 right-0 w-1/3 h-full bg-gradient-to-l from-primary/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-1000 pointer-events-none" />
    </Link>
  )
}

