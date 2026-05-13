"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { Badge } from "@/components/ui/badge"
import { ComboButton } from "@/components/ui/combo-button"
import { cn } from "@/lib/utils"
import { getNodeTitle } from "./clusters"
import type { Edge, Entry } from "@/lib/api/types"

interface RelationItemProps {
  edge: Edge
  neighborId: string
  neighborEntry?: Entry
  onDelete?: (edgeId: string) => void
}

const RELATION_STYLES: Record<string, string> = {
  // Canonical predicates from analysis UI
  agrees: "text-emerald-500 border-emerald-500/30 bg-emerald-500/5",
  refines: "text-sky-500 border-sky-500/30 bg-sky-500/5",
  supersedes: "text-amber-500 border-amber-500/30 bg-amber-500/5",
  contradicts: "text-red-500 border-red-500/30 bg-red-500/5",
  duplicates: "text-fuchsia-500 border-fuchsia-500/30 bg-fuchsia-500/5",
  unrelated: "text-muted-foreground border-border bg-muted/5",
  
  // Graph-specific canonical predicates
  is_a: "text-indigo-500 border-indigo-500/30 bg-indigo-500/5",
  part_of: "text-indigo-500 border-indigo-500/30 bg-indigo-500/5",
  contains: "text-indigo-500 border-indigo-500/30 bg-indigo-500/5",
  implemented_by: "text-emerald-500 border-emerald-500/30 bg-emerald-500/5",
  depends_on: "text-amber-500 border-amber-500/30 bg-amber-500/5",
  caused_by: "text-amber-500 border-amber-500/30 bg-amber-500/5",
  works_for: "text-sky-500 border-sky-500/30 bg-sky-500/5",
}

export function RelationItem({ edge, neighborId, neighborEntry, onDelete }: RelationItemProps) {
  const router = useRouter()
  const rel = edge.relationship?.toLowerCase()
  const styleClass = RELATION_STYLES[rel] || "text-primary/70 border-primary/20 bg-primary/5"

  return (
    <div
      className="group relative rounded-2xl border border-muted-foreground/10 bg-background/40 backdrop-blur-sm p-4 transition-all hover:border-primary/30 hover:bg-primary/5 hover:shadow-[0_8px_30px_rgb(0,0,0,0.12)] cursor-pointer"
      onClick={() => router.push(`/entries/${encodeURIComponent(neighborId)}`)}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1 space-y-1">
          <p className="font-bold text-sm text-foreground/90 leading-tight group-hover:text-primary transition-colors">
            {neighborEntry ? getNodeTitle(neighborEntry) : neighborId}
          </p>
          
          <div className="flex flex-wrap items-center gap-2 mt-1.5">
            <Badge 
              variant="outline" 
              className={cn(
                "text-[8px] font-black uppercase py-0 px-1.5 transition-colors",
                styleClass
              )}
            >
              {edge.relationship}
            </Badge>
            
            <span className="text-[9px] font-bold uppercase tracking-widest text-muted-foreground/40 group-hover:text-muted-foreground/60 transition-colors">
              {edge.edge_type === "similarity"
                ? `Dist: ${edge.distance?.toFixed(3) ?? "N/A"}`
                : `Wt: ${edge.weight?.toFixed(2) ?? "N/A"}`}
            </span>
          </div>
          
          {neighborEntry && neighborEntry.id !== getNodeTitle(neighborEntry) && (
            <p className="text-[9px] text-muted-foreground/30 font-mono truncate mt-2 group-hover:text-muted-foreground/50 transition-colors">
              {neighborId}
            </p>
          )}
        </div>
        
        {edge.edge_type === "manual" && onDelete ? (
          <div 
            onClick={(e) => {
              e.stopPropagation()
            }}
            className="shrink-0"
          >
            <ComboButton
              onConfirm={() => onDelete(edge.id)}
              className="size-8 rounded-full opacity-0 group-hover:opacity-100 transition-opacity"
            />
          </div>
        ) : null}
      </div>
      
      {/* Decorative hover glow */}
      <div className="absolute inset-0 bg-primary/[0.02] opacity-0 group-hover:opacity-100 rounded-2xl pointer-events-none transition-opacity" />
    </div>
  )
}
