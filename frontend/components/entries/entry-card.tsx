"use client"

import Link from "next/link"
import { MoreVertical, Share2, ChevronRight, Database, Clock, Layers, ChevronsRight, Maximize2, Sparkles, ChevronDown, ChevronUp, Eye } from "lucide-react"
import { useState } from "react"
import { cn, formatRelativeTime, stringToHslColor } from "@/lib/utils"
import { ComboButton } from "@/components/ui/combo-button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { Entry, SearchResult } from "@/lib/api"
import { MarkdownView } from "./markdown-view"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { EntryPeek } from "./entry-peek"
import { EntryTag } from "@/components/ui/entry-tag"

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

function retrieverLabel(retrievers: string[]): { label: string; tone: string } {
  const has = (r: string) => retrievers.includes(r)
  const rer = has("rerank")
  if (has("dense") && has("sparse")) {
    return { label: rer ? "D+S+R" : "D+S", tone: "oklch(0.916 0.175 156.8)" }
  }
  if (has("sparse")) return { label: rer ? "S+R" : "S", tone: "oklch(0.78 0.18 65)" }
  if (has("dense"))  return { label: rer ? "D+R" : "D", tone: "oklch(0.9 0.148 196.3)" }
  if (rer)           return { label: "R",   tone: "oklch(0.916 0.175 156.8)" }
  return { label: "—", tone: "oklch(0.42 0 0)" }
}

export function EntryCard({ entry, index = 0, onDelete, showScore = false }: EntryCardProps) {
  const isSearchResult = "score" in entry
  const search = isSearchResult ? (entry as SearchResult) : null
  const score = search?.score ?? null
  const scorePercent = score !== null ? Math.round(score * 100) : null
  const sourceColor = stringToHslColor(entry.source_id, 60, 45)
  const sectionPath = search?.section_path?.filter((s) => s.length > 0) ?? []
  const retrievers = search?.retrievers ?? []
  const retrieverChip = retrievers.length > 0 ? retrieverLabel(retrievers) : null
  
  // Cast to Entry to access analysis if available
  const fullEntry = entry as Entry
  const analysis = fullEntry.analysis

  const [isExpanded, setIsExpanded] = useState(false)

  return (
    <div
      className={cn(
        "group/card relative flex flex-col transition-all duration-300",
        "animate-in fade-in slide-in-from-bottom-2",
        isExpanded && "bg-card shadow-[0_8px_40px_rgb(0,0,0,0.15)] ring-1 ring-primary/20 z-10 my-4"
      )}
      style={{ animationDelay: `${index * 25}ms`, animationFillMode: "both" }}
    >
      <div
        className={cn(
          "relative flex items-start gap-4 md:gap-6 p-4 md:p-5 overflow-hidden transition-all duration-300",
          "bg-card hover:bg-card/60 border-b border-border last:border-b-0",
          !isExpanded && "group-hover/card:translate-x-0.5"
        )}
      >
        {/* Score bar on left edge */}
        {showScore && scorePercent !== null && (
          <div
            className={cn(
              "absolute left-0 top-0 w-0.5 transition-all duration-500",
              !isExpanded && "group-hover/card:w-1"
            )}
            style={{
              height: `${scorePercent}%`,
              background: scoreColor(scorePercent),
              boxShadow: `0 0 12px ${scoreColor(scorePercent)}`,
            }}
          />
        )}

        {/* Image thumbnail */}
        {entry.metadata.source_type === "image" && entry.metadata.source_file && (
          <div className="shrink-0 w-16 h-16 border border-border overflow-hidden bg-muted rounded-sm">
            <img
              src={String(entry.metadata.source_file)}
              alt=""
              className="w-full h-full object-cover transition-transform duration-500 group-hover/card:scale-110"
            />
          </div>
        )}

        {/* Content */}
        <div className="flex-1 min-w-0 space-y-3 pl-1">
          {/* Meta row */}
          <div className="flex items-center gap-2 flex-wrap">
            <Badge
              variant="outline"
              className="h-5 px-1.5 font-mono text-[9px] font-black uppercase tracking-wider"
              style={{
                backgroundColor: `${sourceColor}08`,
                borderColor: `${sourceColor}20`,
                color: sourceColor,
              }}
            >
              <Database className="size-2.5 mr-1 opacity-70" />
              {entry.source_id}
            </Badge>

            <div className="flex items-center gap-1 font-mono text-[9px] text-muted-foreground/60 uppercase tracking-widest">
              <Clock className="size-2.5" />
              {formatRelativeTime(entry.created_at)}
            </div>

            {"type" in entry && entry.type && (
              <Badge
                variant="secondary"
                className="h-5 px-1.5 border border-primary/20 bg-primary/5 text-primary/80 font-mono text-[9px] font-black uppercase tracking-widest"
              >
                {entry.type}
              </Badge>
            )}

            {showScore && retrieverChip && (
              <Badge
                variant="outline"
                title={`Matched by: ${retrievers.join(" + ")}`}
                className="h-5 px-1.5 font-mono text-[9px] font-black uppercase tracking-wider"
                style={{
                  color: retrieverChip.tone,
                  borderColor: `${retrieverChip.tone}25`,
                  backgroundColor: `${retrieverChip.tone}05`,
                }}
              >
                <Layers className="size-2.5 mr-1 opacity-70" />
                {retrieverChip.label}
              </Badge>
            )}

            <div className="flex items-center gap-4 ml-auto">
              {showScore && scorePercent !== null && (
                <div
                  className="font-mono text-[10px] font-black uppercase tracking-widest shrink-0"
                  style={{ color: scoreColor(scorePercent) }}
                >
                  {scorePercent}%
                </div>
              )}

              {/* Integrated Action Buttons */}
              <div
                className="flex items-center gap-1.5 pl-4 border-l border-border/50"
                onClick={(e) => { e.preventDefault(); e.stopPropagation() }}
              >
                <Button
                  variant="ghost"
                  size="icon"
                  className={cn(
                    "size-7 rounded-full transition-all",
                    isExpanded ? "bg-primary/10 text-primary" : "text-muted-foreground/30 hover:text-primary hover:bg-primary/5"
                  )}
                  onClick={() => setIsExpanded(!isExpanded)}
                  title={isExpanded ? "Show less" : "Quick peek"}
                >
                  {isExpanded ? <ChevronUp className="size-3.5" /> : <Eye className="size-3.5" />}
                </Button>

                {!showScore && onDelete && (
                  <div className="flex items-center">
                    <ComboButton onConfirm={() => onDelete(entry.id)} className="size-7" />
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <button className="size-7 flex items-center justify-center text-muted-foreground/40 hover:text-foreground transition-colors">
                          <MoreVertical className="size-3.5" />
                        </button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="min-w-36 font-mono">
                        <DropdownMenuItem className="text-xs cursor-pointer">
                          <Share2 className="mr-2 size-3.5" /> Share
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                )}
                
                <Link href={`/entries/${encodeURIComponent(entry.id)}`}>
                  <div className="p-1.5 rounded-full bg-primary/0 hover:bg-primary/5 transition-colors">
                    <ChevronRight className="size-3.5 text-muted-foreground/30 hover:text-primary transition-all hover:translate-x-0.5" />
                  </div>
                </Link>
              </div>
            </div>
          </div>

          {/* Section path breadcrumb */}
          {showScore && sectionPath.length > 0 && (
            <div className="flex items-center gap-0.5 flex-wrap font-mono text-[9px] text-muted-foreground/50">
              {sectionPath.map((part, i) => (
                <span key={i} className="flex items-center gap-0.5">
                  {i > 0 && <ChevronsRight className="size-2 opacity-30" />}
                  <span className="truncate max-w-[12rem]">{part}</span>
                </span>
              ))}
            </div>
          )}

          {/* Text preview - only show when not expanded */}
          {!isExpanded && (
            <Link href={`/entries/${encodeURIComponent(entry.id)}`} className="block relative group/text">
              <p className="text-sm text-foreground/80 group-hover/card:text-foreground transition-colors leading-relaxed line-clamp-2">
                {entry.text}
              </p>
              <div className="absolute inset-0 bg-gradient-to-t from-card/40 to-transparent opacity-0 group-hover/text:opacity-100 transition-opacity" />
            </Link>
          )}

          {/* Metadata tags - also hide when expanded since EntryPeek shows them better */}
          {Object.keys(entry.metadata).length > 0 && !isExpanded && (
            <div className="flex gap-2 flex-wrap opacity-60 group-hover/card:opacity-100 transition-opacity duration-300">
              {Object.entries(entry.metadata)
                .slice(0, 5)
                .map(([key, value]) => {
                   if (key === "source_type" || key === "source_file") return null;
                   
                   // Special handling for tags field
                   if (key === "tags" && typeof value === "string") {
                     const tagList = value.split(",").map(t => t.trim()).filter(Boolean);
                     return tagList.map(tag => (
                       <EntryTag key={tag} label={tag} icon={false} className="h-4 px-1 text-[8px] bg-primary/5 border-primary/10 text-primary/70" />
                     ));
                   }

                   return (
                    <div
                      key={key}
                      className="flex items-center gap-1.5 px-1.5 py-0.5 rounded-sm bg-muted/20 border border-border/50 font-mono text-[9px]"
                    >
                      <span className="font-bold text-muted-foreground/40 uppercase tracking-tighter">{key}</span>
                      <span className="text-muted-foreground/80 truncate max-w-[120px]">{String(value)}</span>
                    </div>
                  );
                })}
            </div>
          )}
        </div>

      </div>

      {isExpanded && (
        <div className="border-t border-border bg-muted/5 animate-in slide-in-from-top-4 duration-300">
           <EntryPeek entry={fullEntry} />
           <div className="px-6 py-4 border-t border-border/50 bg-muted/20 flex justify-end">
              <Link href={`/entries/${encodeURIComponent(entry.id)}`}>
                <Button variant="outline" size="sm" className="font-mono text-[10px] uppercase tracking-widest gap-2">
                  Open Full Entry
                  <Maximize2 className="size-3" />
                </Button>
              </Link>
           </div>
        </div>
      )}
    </div>
  )
}
