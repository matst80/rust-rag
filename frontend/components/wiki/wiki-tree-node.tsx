"use client"

import Link from "next/link"
import { useEffect, useRef, useState } from "react"
import { ChevronDown, ChevronRight, FileText, Folder, FolderOpen } from "lucide-react"
import { cn, entryTitle } from "@/lib/utils"
import { useEntriesTree } from "@/lib/api"
import {
  clearEntryDrag,
  hasEntryDragData,
  readEntryDragData,
  setEntryDragData,
} from "@/lib/drag-entry"

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
  const autoExpandTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const isSelected =
    selectedSourceId === sourceId && (selectedPath ?? null) === node.path
  // Expandable if it has subfolders, or pages of its own to reveal.
  const expandable = node.children.length > 0 || node.count > 0
  const padding = `calc(${depth} * 0.75rem + 0.5rem)`

  // Lazily fetch this folder's own pages only once expanded.
  const { data: ownTree } = useEntriesTree(open && node.count > 0 ? sourceId : null, node.path)
  const pages = ownTree?.entries ?? []

  // Hovering a collapsed folder during a drag opens it so deeper targets show.
  const startAutoExpand = () => {
    if (!expandable || open || autoExpandTimer.current) return
    autoExpandTimer.current = setTimeout(() => {
      autoExpandTimer.current = null
      setOpen(true)
    }, 500)
  }

  const cancelAutoExpand = () => {
    if (autoExpandTimer.current) {
      clearTimeout(autoExpandTimer.current)
      autoExpandTimer.current = null
    }
  }

  useEffect(() => cancelAutoExpand, [])

  return (
    <div className="flex flex-col">
      <div
        className={cn(
          "flex items-center transition-colors rounded-sm",
          isDragOver && "bg-primary/10 ring-2 ring-primary"
        )}
        onDragOver={(e) => {
          if (!onDropEntry || !hasEntryDragData(e.dataTransfer)) return
          e.preventDefault()
          e.stopPropagation()
          e.dataTransfer.dropEffect = "move"
          if (!isDragOver) setIsDragOver(true)
          startAutoExpand()
        }}
        onDragLeave={(e) => {
          e.preventDefault()
          e.stopPropagation()
          setIsDragOver(false)
          cancelAutoExpand()
        }}
        onDrop={(e) => {
          e.preventDefault()
          e.stopPropagation()
          setIsDragOver(false)
          cancelAutoExpand()
          const payload = readEntryDragData(e.dataTransfer)
          if (payload && onDropEntry) {
            onDropEntry(sourceId, node.path, payload.id)
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
          {isDragOver ? (
            <span className="ml-auto shrink-0 rounded bg-primary px-1.5 py-0.5 font-mono text-[9px] font-bold uppercase text-primary-foreground">
              Move here
            </span>
          ) : (
            <span className="ml-auto font-mono text-[10px] text-muted-foreground tabular-nums shrink-0">
              {node.subtreeCount}
            </span>
          )}
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
                  setEntryDragData(evt.dataTransfer, {
                    id: page.id,
                    source_id: page.source_id,
                    path: page.path,
                    title: entryTitle(page, 80),
                  })
                }}
                onDragEnd={() => clearEntryDrag()}
                className={cn(
                  "flex items-center gap-2 min-w-0 px-2 py-1 font-mono text-xs transition-colors hover:bg-card border-l-2",
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
