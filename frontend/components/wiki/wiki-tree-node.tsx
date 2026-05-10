"use client"

import Link from "next/link"
import { useState } from "react"
import { ChevronDown, ChevronRight, Folder, FolderOpen } from "lucide-react"
import { useEntriesTree } from "@/lib/api"
import { cn } from "@/lib/utils"

interface WikiTreeNodeProps {
  sourceId: string
  /** undefined = source root (top-level for this source). */
  path?: string
  /** Display label. For source roots, the source_id; for folders, the segment. */
  label: string
  /** Folder-segment node only: hint that a deeper level exists. Source roots are
   * always treated as expandable since their content is unknown until expanded. */
  hasChildren?: boolean
  /** Whether this node should render as initially expanded. */
  defaultOpen?: boolean
  /** Currently selected path within `sourceId`, used to highlight active node. */
  selectedSourceId: string | null
  selectedPath: string | null
  depth: number
}

function buildHref(sourceId: string, path?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  return `/wiki?${params.toString()}`
}

export function WikiTreeNode(props: WikiTreeNodeProps) {
  const {
    sourceId,
    path,
    label,
    hasChildren,
    defaultOpen,
    selectedSourceId,
    selectedPath,
    depth,
  } = props
  // Source roots default open when they're the active source so the user lands
  // inside the relevant tree without an extra click.
  const [open, setOpen] = useState(
    defaultOpen ?? (path === undefined && selectedSourceId === sourceId)
  )

  // Fetch when open. Source root passes undefined prefix → top-level segments.
  const { data: tree, isLoading } = useEntriesTree(open ? sourceId : null, path)

  const isSelected =
    selectedSourceId === sourceId && (selectedPath ?? null) === (path ?? null)
  const expandable = path === undefined ? true : hasChildren ?? false

  // Render: row with disclosure + link, plus children when open.
  const padding = `calc(${depth} * 0.75rem + 0.5rem)`

  return (
    <div className="flex flex-col">
      <div className="flex items-center">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
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
          href={buildHref(sourceId, path)}
          className={cn(
            "flex items-center gap-2 flex-1 min-w-0 px-2 py-1.5 font-mono text-xs transition-colors hover:bg-card",
            isSelected
              ? "bg-primary/10 text-primary border-l-2 border-primary"
              : "text-foreground border-l-2 border-transparent"
          )}
          onClick={() => {
            if (!open && expandable) setOpen(true)
          }}
        >
          {open ? (
            <FolderOpen className="size-3.5 text-primary shrink-0" />
          ) : (
            <Folder className="size-3.5 text-muted-foreground shrink-0" />
          )}
          <span className="truncate">{label}</span>
        </Link>
      </div>

      {open && (
        <div className="flex flex-col">
          {isLoading && (
            <p
              className="font-mono text-[10px] text-muted-foreground/60 py-1"
              style={{ marginLeft: `calc(${padding} + 1.25rem)` }}
            >
              loading…
            </p>
          )}
          {tree && tree.children.length === 0 && (
            <p
              className="font-mono text-[10px] text-muted-foreground/40 py-1"
              style={{ marginLeft: `calc(${padding} + 1.25rem)` }}
            >
              {tree.entries.length === 0 ? "(empty)" : ""}
            </p>
          )}
          {tree?.children.map((child) => (
            <WikiTreeNode
              key={child.path}
              sourceId={sourceId}
              path={child.path}
              label={child.segment}
              hasChildren={child.has_children}
              selectedSourceId={selectedSourceId}
              selectedPath={selectedPath}
              depth={depth + 1}
            />
          ))}
        </div>
      )}
    </div>
  )
}
