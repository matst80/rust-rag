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
        "rounded-lg border bg-card px-3 py-2 shadow-sm transition-all",
        nodeData.isCenter && "border-primary/60 bg-primary/5",
        nodeData.isSelected && "ring-2 ring-primary"
      )}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!size-2 !border-none !bg-muted-foreground"
      />
      <div className="max-w-40">
        <p className="truncate text-xs font-medium">{nodeData.label}</p>
        <p className="truncate text-xs text-muted-foreground">
          {nodeData.isCenter
            ? `${nodeData.sourceId} · root`
            : `${nodeData.sourceId}${nodeData.depth ? ` · depth ${nodeData.depth}` : ""}`}
        </p>
      </div>
      <Handle
        type="source"
        position={Position.Right}
        className="!size-2 !border-none !bg-muted-foreground"
      />
    </div>
  )
}

export const EntryNode = memo(EntryNodeComponent)
