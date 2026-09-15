"use client"

import Link from "next/link"
import { useState } from "react"
import { ChevronDown, ChevronRight, FileText, Folder, FolderOpen } from "lucide-react"
import { cn, entryTitle } from "@/lib/utils"
import { useEntriesTree } from "@/lib/api"

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
  selectedEntryId?: string | null
  depth: number
  /** Whether the parent (source root) is the active selection — controls
   *  default-open behaviour for the path that leads to the active leaf. */
  activeChain: Set<string>
  onDropEntry?: (targetSourceId: string, targetPath: string, entryId: string) => void
}

function buildHref(sourceId: string, path?: string, entryId?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  if (entryId) params.set("entry", entryId)
  return `/wiki?${params.toString()}`
}

export function WikiTreeNode(props: WikiTreeNodeProps) {
  const {
    sourceId,
    node,
    selectedSourceId,
    selectedPath,
    selectedEntryId,
    depth,
    activeChain,
    onDropEntry,
  } = props
  const [open, setOpen] = useState(activeChain.has(node.path))
  const [isDragOver, setIsDragOver] = useState(false)

  const isSelected =
    selectedSourceId === sourceId && (selectedPath ?? null) === node.path
  // Expandable if it has subfolders, or pages of its own to reveal.
  const expandable = node.children.length > 0 || node.count > 0
  const padding = `calc(${depth} * 0.75rem + 0.5rem)`

  // Lazily fetch this folder's own pages only once expanded.
  const { data: ownTree } = useEntriesTree(open && node.count > 0 ? sourceId : null, node.path)
  const pages = ownTree?.entries ?? []

  return (
    <div className="flex flex-col">
      <div
        className={cn(
          "flex items-center transition-colors rounded-sm",
          isDragOver && "bg-primary/20 ring-1 ring-primary"
        )}
        onDragOver={(e) => {
          if (onDropEntry) {
            e.preventDefault()
            e.stopPropagation()
            e.dataTransfer.dropEffect = "move"
            if (!isDragOver) setIsDragOver(true)
          }
        }}
        onDragLeave={(e) => {
          e.preventDefault()
          e.stopPropagation()
          setIsDragOver(false)
        }}
        onDrop={(e) => {
          e.preventDefault()
          e.stopPropagation()
          setIsDragOver(false)
          let entryId = ""
          try {
            const raw = e.dataTransfer.getData("application/json")
            if (raw) {
              const data = JSON.parse(raw)
              entryId = data.id
            }
          } catch {
            // fallback
          }
          if (!entryId) {
            entryId = e.dataTransfer.getData("text/plain")
          }
          if (entryId && onDropEntry) {
            onDropEntry(sourceId, node.path, entryId)
          }
        }}
      >
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
          {pages.map((page) => {
            const pageSelected = selectedEntryId === page.id
            return (
              <Link
                key={page.id}
                href={buildHref(sourceId, node.path, page.id)}
                draggable
                onDragStart={(evt) => {
                  evt.dataTransfer.setData(
                    "application/json",
                    JSON.stringify({ id: page.id, source_id: page.source_id, path: page.path })
                  )
                  evt.dataTransfer.setData("text/plain", page.id)
                  evt.dataTransfer.effectAllowed = "move"
                }}
                className={cn(
                  "flex items-center gap-2 min-w-0 px-2 py-1 font-mono text-[11px] transition-colors hover:bg-card border-l-2",
                  pageSelected
                    ? "bg-primary/10 text-primary border-primary"
                    : "text-muted-foreground hover:text-foreground border-transparent"
                )}
                style={{ marginLeft: `calc(${depth + 1} * 0.75rem + 0.5rem)` }}
                title={page.id}
              >
                <FileText className="size-3 shrink-0" />
                <span className="truncate">{entryTitle(page)}</span>
              </Link>
            )
          })}
          {node.children.map((child) => (
            <WikiTreeNode
              key={child.path}
              sourceId={sourceId}
              node={child}
              selectedSourceId={selectedSourceId}
              selectedPath={selectedPath}
              selectedEntryId={selectedEntryId}
              depth={depth + 1}
              activeChain={activeChain}
              onDropEntry={onDropEntry}
            />
          ))}
        </div>
      )}
    </div>
  )
}
