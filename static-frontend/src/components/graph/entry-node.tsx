"use client"

import { memo } from "react"
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react"
import { cn } from "@/lib/utils"

export interface EntryNodeData extends Record<string, unknown> {
  label: string
  sourceId: string
  text: string
  depth?: number
  isCenter?: boolean
  isSelected?: boolean
}

function EntryNodeComponent({ data }: NodeProps<Node<EntryNodeData>>) {
  const nodeData = data
  return (
    <div
      className={cn(
        "rounded-xl border bg-card/80 backdrop-blur-md px-4 py-3 shadow-lg transition-all duration-500",
        "hover:shadow-primary/5 hover:border-primary/30 group",
        nodeData.isCenter && "border-primary/40 bg-primary/10 shadow-[0_0_20px_rgba(var(--primary),0.1)] ring-1 ring-primary/20",
        nodeData.isSelected && "ring-2 ring-primary border-transparent shadow-primary/20"
      )}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!size-1.5 !border-none !bg-primary/40 group-hover:!bg-primary transition-colors"
      />
      <div className="min-w-32 max-w-48 space-y-1">
        <div className="flex items-center justify-between gap-2">
          <p className="truncate text-[11px] font-black uppercase tracking-wider text-primary/80">
            {nodeData.label}
          </p>
          {nodeData.isCenter && (
            <div className="size-1.5 rounded-full bg-primary animate-pulse" />
          )}
        </div>
        <p className="truncate text-[10px] font-medium text-muted-foreground/60 italic">
          {nodeData.isCenter
            ? `${nodeData.sourceId} • root`
            : `${nodeData.sourceId}${nodeData.depth ? ` • depth ${nodeData.depth}` : ""}`}
        </p>
      </div>
      <Handle
        type="source"
        position={Position.Right}
        className="!size-1.5 !border-none !bg-primary/40 group-hover:!bg-primary transition-colors"
      />
    </div>
  )
}

export const EntryNode = memo(EntryNodeComponent)
