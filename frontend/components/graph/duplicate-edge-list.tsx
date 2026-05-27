"use client"

import * as React from "react"
import { useSWRConfig } from "swr"
import { Loader2, Trash2, ShieldAlert, Check, GitBranch } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { toast } from "sonner"
import { useDuplicateEdges, useDeleteEdge } from "@/lib/api"
import { RELATION_STYLES } from "./relation-item"
import { cn } from "@/lib/utils"

interface DuplicateEdgeListProps {
  onFocusNode?: (id: string) => void
}

export function DuplicateEdgeList({ onFocusNode }: DuplicateEdgeListProps) {
  const { data: duplicateGroups, isLoading, mutate } = useDuplicateEdges()
  const { trigger: deleteEdge, isMutating: isDeleting } = useDeleteEdge()
  const { mutate: globalMutate } = useSWRConfig()

  const handleDelete = async (edgeId: string) => {
    try {
      await deleteEdge(edgeId)
      await mutate()
      globalMutate("edges")
      toast.success("Edge deleted")
    } catch (err) {
      toast.error("Failed to delete edge")
    }
  }

  if (isLoading) {
    return (
      <div className="flex h-40 items-center justify-center">
        <Loader2 className="size-6 animate-spin text-muted-foreground/30" />
      </div>
    )
  }

  if (!duplicateGroups || duplicateGroups.length === 0) {
    return (
      <div className="flex h-40 flex-col items-center justify-center text-center gap-2">
        <ShieldAlert className="size-8 text-emerald-500/50" />
        <p className="text-sm font-medium text-muted-foreground">No Duplicate Edges</p>
        <p className="text-xs text-muted-foreground/70">
          The ontology constraint is active and no redundant multi-edges exist between identical node pairs.
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-6 overflow-y-auto pr-2 custom-scrollbar h-full">
      <div className="flex items-center justify-between p-4 rounded-xl bg-amber-500/10 border border-amber-500/20">
        <div className="flex items-center gap-3">
          <ShieldAlert className="size-5 text-amber-500" />
          <div className="space-y-0.5">
            <h4 className="text-sm font-bold text-amber-700 dark:text-amber-400">
              Duplicates Found
            </h4>
            <p className="text-xs text-amber-700/70 dark:text-amber-400/70">
              {duplicateGroups.length} node pair(s) have multiple edges connecting them. Review and delete redundant relationshipships.
            </p>
          </div>
        </div>
      </div>

      <div className="space-y-4">
        {duplicateGroups.map((group, idx) => (
          <div
            key={idx}
            className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4 overflow-hidden relative"
          >
            <div className="absolute top-0 left-0 w-1 h-full bg-border/50" />
            
            <div className="flex flex-wrap items-center gap-2 pl-2">
              <button
                className="font-mono text-xs font-bold text-foreground hover:text-primary transition-colors underline-offset-4 hover:underline"
                onClick={() => onFocusNode?.(group.from_item_id)}
              >
                {group.from_item_id}
              </button>
              <GitBranch className="size-3 text-muted-foreground/40" />
              <button
                className="font-mono text-xs font-bold text-foreground hover:text-primary transition-colors underline-offset-4 hover:underline"
                onClick={() => onFocusNode?.(group.to_item_id)}
              >
                {group.to_item_id}
              </button>
            </div>

            <div className="flex flex-col gap-2 pl-2">
              {group.edges.map((edge) => {
                const relationshipStyle = edge.relationship 
                  ? RELATION_STYLES[edge.relationship.toLowerCase()] 
                  : ""
                return (
                  <div
                    key={edge.id}
                    className="flex items-center justify-between gap-3 p-2 rounded-lg border border-border/50 bg-background/50 hover:bg-muted/30 transition-colors"
                  >
                    <div className="flex items-center gap-3 min-w-0">
                      <Badge
                        variant="outline"
                        className={cn(
                          "h-5 px-1.5 font-mono text-[10px] font-black uppercase tracking-wider border-border/60 text-muted-foreground/80 bg-muted/5 leading-none",
                          relationshipStyle
                        )}
                      >
                        {edge.relationship || edge.edge_type}
                      </Badge>
                      <div className="flex flex-col min-w-0">
                         <span className="text-[10px] font-mono text-muted-foreground/60 font-medium">Weight: {edge.weight.toFixed(2)}</span>
                         <span className="text-[9px] font-mono text-muted-foreground/40 truncate" title={edge.id}>ID: {edge.id}</span>
                      </div>
                    </div>
                    
                    <Button
                      variant="ghost"
                      size="icon"
                      disabled={isDeleting}
                      onClick={() => handleDelete(edge.id)}
                      className="size-7 rounded-md text-red-500/70 hover:text-red-500 hover:bg-red-500/10 transition-colors shrink-0"
                      title="Delete this edge"
                    >
                      <Trash2 className="size-3.5" />
                    </Button>
                  </div>
                )
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
