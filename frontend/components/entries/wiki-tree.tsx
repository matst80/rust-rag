"use client"

import { ChevronDown, ChevronRight, Folder, FolderOpen, Loader2 } from "lucide-react"
import { useEntriesTree } from "@/lib/api"
import type { TreeChild } from "@/lib/api"
import { cn } from "@/lib/utils"

/**
 * Real wiki-path tree, shared by the path picker (entry-form) and the
 * entries browser sidebar. Fetches each level lazily from
 * `/api/entries/tree` on expand — never guesses structure client-side.
 */
export function WikiTree({
  sourceId,
  depth = 0,
  prefix,
  children: nodes,
  value,
  expanded,
  onToggle,
  onSelect,
}: {
  sourceId: string
  depth?: number
  prefix: string
  children: TreeChild[]
  value: string | null
  expanded: Set<string>
  onToggle: (path: string) => void
  onSelect: (path: string) => void
}) {
  return (
    <>
      {nodes.map((node) => (
        <WikiTreeRow
          key={node.path}
          sourceId={sourceId}
          node={node}
          depth={depth}
          value={value}
          expanded={expanded}
          onToggle={onToggle}
          onSelect={onSelect}
        />
      ))}
    </>
  )
}

function WikiTreeRow({
  sourceId,
  node,
  depth,
  value,
  expanded,
  onToggle,
  onSelect,
}: {
  sourceId: string
  node: TreeChild
  depth: number
  value: string | null
  expanded: Set<string>
  onToggle: (path: string) => void
  onSelect: (path: string) => void
}) {
  const isOpen = expanded.has(node.path)
  const isSelected = value === node.path
  const { data: childData, isLoading } = useEntriesTree(isOpen ? sourceId : null, node.path)

  return (
    <div className="flex flex-col">
      <div
        className={cn(
          "flex items-center gap-1 rounded-md pr-2 transition-colors group",
          isSelected ? "bg-primary/10" : "hover:bg-muted/50"
        )}
        style={{ paddingLeft: `${depth * 16 + 4}px` }}
      >
        <button
          type="button"
          onClick={() => onToggle(node.path)}
          className={cn(
            "shrink-0 flex items-center justify-center size-6 text-muted-foreground hover:text-foreground transition-colors",
            !node.has_children && "invisible"
          )}
          tabIndex={-1}
        >
          {isOpen ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        </button>
        <button
          type="button"
          onClick={() => onSelect(node.path)}
          className={cn(
            "flex flex-1 items-center gap-1.5 min-w-0 py-1.5 text-left font-mono text-sm",
            isSelected ? "text-primary font-semibold" : "text-foreground"
          )}
        >
          {isOpen ? (
            <FolderOpen className="size-4 text-primary shrink-0" />
          ) : (
            <Folder className="size-4 text-muted-foreground shrink-0" />
          )}
          <span className="truncate">{node.segment}</span>
          <span className="ml-auto font-mono text-xs text-muted-foreground tabular-nums shrink-0">
            {node.count > 0 ? node.count : ""}
          </span>
        </button>
      </div>
      {isOpen && (
        <div className="flex flex-col">
          {isLoading && (
            <div
              className="flex items-center gap-1.5 py-1 text-xs text-muted-foreground"
              style={{ paddingLeft: `${(depth + 1) * 16 + 28}px` }}
            >
              <Loader2 className="size-3 animate-spin" /> loading…
            </div>
          )}
          {childData && childData.children.length > 0 && (
            <WikiTree
              sourceId={sourceId}
              prefix={node.path}
              depth={depth + 1}
              value={value}
              expanded={expanded}
              onToggle={onToggle}
              onSelect={onSelect}
            >
              {childData.children}
            </WikiTree>
          )}
        </div>
      )}
    </div>
  )
}

export function ancestorChain(path: string): Set<string> {
  if (!path) return new Set()
  const segs = path.split("/")
  const set = new Set<string>()
  for (let i = 1; i <= segs.length; i++) set.add(segs.slice(0, i).join("/"))
  return set
}
