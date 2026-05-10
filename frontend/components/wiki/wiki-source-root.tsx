"use client"

import Link from "next/link"
import { useState } from "react"
import { ChevronDown, ChevronRight, Database, FolderOpen, Folder } from "lucide-react"
import { useEntriesTree } from "@/lib/api"
import { cn } from "@/lib/utils"
import { WikiTreeNode } from "./wiki-tree-node"

function buildHref(sourceId: string, path?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  return `/wiki?${params.toString()}`
}

interface WikiSourceRootProps {
  sourceId: string
  selectedSourceId: string | null
  selectedPath: string | null
}

/**
 * One source_id rendered as a top-level tree root in the wiki sidebar. Hides
 * itself when the source has no entries with `path` set — keeps the sidebar
 * focused on sources that actually have wiki structure.
 */
export function WikiSourceRoot({
  sourceId,
  selectedSourceId,
  selectedPath,
}: WikiSourceRootProps) {
  const isSelected = sourceId === selectedSourceId
  // Default open when this source is the active selection so the user lands
  // inside its tree without an extra click.
  const [open, setOpen] = useState(isSelected)

  // Eager top-level fetch so we can hide sources with no path-bearing data.
  const { data: tree, isLoading } = useEntriesTree(sourceId, undefined)

  // Hide if no wiki content (no folders + no leaves at top level).
  if (tree && tree.children.length === 0 && tree.entries.length === 0) {
    return null
  }

  return (
    <div className="flex flex-col">
      <div className="flex items-center">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="shrink-0 flex items-center justify-center size-5 ml-1 text-muted-foreground hover:text-foreground transition-colors"
          aria-label={open ? "Collapse" : "Expand"}
        >
          {open ? (
            <ChevronDown className="size-3.5" />
          ) : (
            <ChevronRight className="size-3.5" />
          )}
        </button>
        <Link
          href={buildHref(sourceId)}
          className={cn(
            "flex items-center gap-2 flex-1 min-w-0 px-2 py-1.5 font-mono text-xs font-bold uppercase tracking-wider transition-colors hover:bg-card",
            isSelected && (selectedPath ?? null) === null
              ? "bg-primary/10 text-primary border-l-2 border-primary"
              : "text-foreground border-l-2 border-transparent"
          )}
        >
          {open ? (
            <FolderOpen className="size-3.5 text-primary shrink-0" />
          ) : (
            <Database className="size-3.5 text-muted-foreground shrink-0" />
          )}
          <span className="truncate">{sourceId}</span>
        </Link>
      </div>

      {open && (
        <div className="flex flex-col">
          {isLoading && !tree && (
            <p className="font-mono text-[10px] text-muted-foreground/60 py-1 ml-9">
              loading…
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
              depth={1}
            />
          ))}
          {tree && tree.entries.length > 0 && tree.children.length === 0 && (
            <p className="font-mono text-[10px] text-muted-foreground/60 py-1 ml-9">
              {tree.entries.length} root entr{tree.entries.length === 1 ? "y" : "ies"}
            </p>
          )}
        </div>
      )}
    </div>
  )
}
