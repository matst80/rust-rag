"use client"

import Link from "next/link"
import { useState } from "react"
import { ChevronDown, ChevronRight, Folder, FolderOpen } from "lucide-react"
import { cn } from "@/lib/utils"

export interface TreeNodeData {
  segment: string
  path: string
  /** Direct entries with this exact path. */
  count: number
  /** Total entries under this subtree (including descendants). */
  subtreeCount: number
  children: TreeNodeData[]
}

interface WikiTreeNodeProps {
  sourceId: string
  node: TreeNodeData
  selectedSourceId: string | null
  selectedPath: string | null
  depth: number
  /** Whether the parent (source root) is the active selection — controls
   *  default-open behaviour for the path that leads to the active leaf. */
  activeChain: Set<string>
}

function buildHref(sourceId: string, path?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  return `/wiki?${params.toString()}`
}

export function WikiTreeNode(props: WikiTreeNodeProps) {
  const { sourceId, node, selectedSourceId, selectedPath, depth, activeChain } = props
  const [open, setOpen] = useState(activeChain.has(node.path))

  const isSelected =
    selectedSourceId === sourceId && (selectedPath ?? null) === node.path
  const expandable = node.children.length > 0
  const padding = `calc(${depth} * 0.75rem + 0.5rem)`

  return (
    <div className="flex flex-col">
      <div className="flex items-center">
        <button
          type="button"
          onClick={() => expandable && setOpen((v) => !v)}
          className={cn(
            "shrink-0 flex items-center justify-center size-5 text-muted-foreground hover:text-foreground transition-colors",
            !expandable && "opacity-30 pointer-events-none"
          )}
          aria-label={open ? "Collapse" : "Expand"}
          style={{ marginLeft: padding }}
        >
          {open ? (
            <ChevronDown className="size-3.5" />
          ) : (
            <ChevronRight className="size-3.5" />
          )}
        </button>
        <Link
          href={buildHref(sourceId, node.path)}
          className={cn(
            "flex items-center gap-2 flex-1 min-w-0 px-2 py-1.5 font-mono text-xs transition-colors hover:bg-card",
            isSelected
              ? "bg-primary/10 text-primary border-l-2 border-primary"
              : "text-foreground border-l-2 border-transparent"
          )}
        >
          {open && expandable ? (
            <FolderOpen className="size-3.5 text-primary shrink-0" />
          ) : (
            <Folder className="size-3.5 text-muted-foreground shrink-0" />
          )}
          <span className="truncate">{node.segment}</span>
          <span className="ml-auto font-mono text-[10px] text-muted-foreground tabular-nums shrink-0">
            {node.subtreeCount}
          </span>
        </Link>
      </div>

      {open && expandable && (
        <div className="flex flex-col">
          {node.children.map((child) => (
            <WikiTreeNode
              key={child.path}
              sourceId={sourceId}
              node={child}
              selectedSourceId={selectedSourceId}
              selectedPath={selectedPath}
              depth={depth + 1}
              activeChain={activeChain}
            />
          ))}
        </div>
      )}
    </div>
  )
}
