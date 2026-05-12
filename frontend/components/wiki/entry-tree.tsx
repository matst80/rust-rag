"use client"

import Link from "next/link"
import { useMemo, useState } from "react"
import {
  ArrowUpRight,
  ChevronDown,
  ChevronRight,
  Database,
  FileText,
  Folder,
  FolderOpen,
  FolderTree,
  Menu,
  X,
} from "lucide-react"
import { MarkdownView } from "@/components/entries/markdown-view"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { Button } from "@/components/ui/button"
import { useEntriesPaths, useEntriesTree } from "@/lib/api"
import { useIsMobile } from "@/hooks/use-mobile"
import { cn } from "@/lib/utils"
import type { PathRow } from "@/lib/api"
import { WikiTreeNode, type TreeNodeData } from "./wiki-tree-node"

function buildHref(sourceId: string, path?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  return `/wiki?${params.toString()}`
}

interface EntryTreeProps {
  sourceId: string
  prefix?: string
}

interface SourceTree {
  sourceId: string
  totalCount: number
  roots: TreeNodeData[]
}

/** Build per-source nested trees from a flat list of (source, path, count). */
function buildSourceTrees(rows: PathRow[]): SourceTree[] {
  const bySource = new Map<string, PathRow[]>()
  for (const r of rows) {
    if (!bySource.has(r.source_id)) bySource.set(r.source_id, [])
    bySource.get(r.source_id)!.push(r)
  }
  const out: SourceTree[] = []
  for (const [sourceId, sourceRows] of bySource) {
    // Build a node for each path. Map by full path. Insert intermediate
    // nodes for any segment that isn't itself a stored path (e.g. only
    // `a/b/c` exists → still want `a` and `a/b` as folder nodes).
    const byPath = new Map<string, TreeNodeData>()
    const ensure = (path: string): TreeNodeData => {
      const existing = byPath.get(path)
      if (existing) return existing
      const segment = path.includes("/") ? path.slice(path.lastIndexOf("/") + 1) : path
      const node: TreeNodeData = {
        segment,
        path,
        count: 0,
        subtreeCount: 0,
        children: [],
      }
      byPath.set(path, node)
      return node
    }
    for (const r of sourceRows) {
      const segs = r.path.split("/")
      // create chain of ancestors
      for (let i = 1; i <= segs.length; i++) {
        ensure(segs.slice(0, i).join("/"))
      }
      ensure(r.path).count = r.count
    }
    // Wire parent → children
    for (const node of byPath.values()) {
      const idx = node.path.lastIndexOf("/")
      if (idx === -1) continue
      const parentPath = node.path.slice(0, idx)
      const parent = byPath.get(parentPath)
      if (parent) parent.children.push(node)
    }
    // Sort children alphabetically per node
    for (const node of byPath.values()) {
      node.children.sort((a, b) => a.segment.localeCompare(b.segment))
    }
    // Compute subtree counts via post-order walk on roots.
    const roots = Array.from(byPath.values()).filter((n) => !n.path.includes("/"))
    roots.sort((a, b) => a.segment.localeCompare(b.segment))
    const fillSubtree = (n: TreeNodeData): number => {
      n.subtreeCount = n.count + n.children.reduce((s, c) => s + fillSubtree(c), 0)
      return n.subtreeCount
    }
    let total = 0
    for (const r of roots) total += fillSubtree(r)
    out.push({ sourceId, totalCount: total, roots })
  }
  out.sort((a, b) => a.sourceId.localeCompare(b.sourceId))
  return out
}

/** Active path → set of every ancestor path along the chain (inclusive). */
function ancestorChain(prefix?: string): Set<string> {
  const set = new Set<string>()
  if (!prefix) return set
  const segs = prefix.split("/")
  for (let i = 1; i <= segs.length; i++) {
    set.add(segs.slice(0, i).join("/"))
  }
  return set
}

export function EntryTree({ sourceId, prefix }: EntryTreeProps) {
  const isMobile = useIsMobile()
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false)
  const { data: pathsResp } = useEntriesPaths()
  const { data: tree, isLoading: treeLoading } = useEntriesTree(sourceId, prefix)

  const sourceTrees = useMemo(
    () => (pathsResp ? buildSourceTrees(pathsResp.paths) : []),
    [pathsResp]
  )
  const activeChain = useMemo(() => ancestorChain(prefix), [prefix])
  const segments = prefix ? prefix.split("/") : []

  const sidebar = (
    <div className="flex h-full flex-col bg-background">
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
        <FolderTree className="size-4 text-primary" />
        <span className="font-mono text-[11px] font-bold uppercase tracking-[2px]">
          Sources
        </span>
        {isMobile && (
          <Button
            variant="ghost"
            size="icon"
            className="ml-auto size-7"
            onClick={() => setMobileSidebarOpen(false)}
            aria-label="Close sidebar"
          >
            <X className="size-4" />
          </Button>
        )}
      </div>
      <div className="flex-1 overflow-y-auto py-2">
        {!pathsResp && (
          <p className="font-mono text-xs text-muted-foreground px-3">Loading…</p>
        )}
        {pathsResp && sourceTrees.length === 0 && (
          <p className="font-mono text-xs text-muted-foreground px-3">
            No entries with paths yet. Set a `path` on an entry to populate the wiki.
          </p>
        )}
        {sourceTrees.map((s) => (
          <SourceRoot
            key={s.sourceId}
            tree={s}
            selectedSourceId={sourceId}
            selectedPath={prefix ?? null}
            activeChain={activeChain}
          />
        ))}
      </div>
    </div>
  )

  const content = (
    <div className="flex h-full flex-col bg-background">
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        {isMobile && (
          <Button
            variant="ghost"
            size="icon"
            className="size-8"
            onClick={() => setMobileSidebarOpen(true)}
            aria-label="Open sidebar"
          >
            <Menu className="size-4" />
          </Button>
        )}
        <nav className="flex items-center gap-1 font-mono text-xs flex-wrap min-w-0">
          <Link
            href={buildHref(sourceId)}
            className="font-bold uppercase tracking-wider text-muted-foreground hover:text-primary"
          >
            {sourceId}
          </Link>
          {segments.map((seg, i) => {
            const sub = segments.slice(0, i + 1).join("/")
            const isLast = i === segments.length - 1
            return (
              <span key={sub} className="flex items-center gap-1">
                <ChevronRight className="size-3 text-muted-foreground" />
                {isLast ? (
                  <span className="text-foreground">{seg}</span>
                ) : (
                  <Link
                    href={buildHref(sourceId, sub)}
                    className="text-muted-foreground hover:text-primary"
                  >
                    {seg}
                  </Link>
                )}
              </span>
            )
          })}
        </nav>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {treeLoading && (
          <p className="font-mono text-xs text-muted-foreground">Loading…</p>
        )}

        {tree && tree.children.length === 0 && tree.entries.length === 0 && (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <Folder className="size-8 mb-3 text-muted-foreground/30" />
            <p className="font-mono text-xs text-muted-foreground">
              No entries under this path.
            </p>
          </div>
        )}

        {tree && tree.children.length > 0 && (
          <div className="mb-6">
            <h2 className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground mb-3">
              Folders
            </h2>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
              {tree.children.map((c) => (
                <Link
                  key={c.path}
                  href={buildHref(sourceId, c.path)}
                  className="flex items-center gap-3 border border-border bg-card p-3 hover:border-primary/40 transition-colors"
                >
                  <Folder className="size-4 text-primary shrink-0" />
                  <div className="flex flex-col min-w-0 flex-1">
                    <span className="font-mono text-xs font-bold truncate">
                      {c.segment}
                    </span>
                    <span className="font-mono text-[10px] text-muted-foreground">
                      {c.count} entr{c.count === 1 ? "y" : "ies"}
                      {c.has_children ? " · subfolders" : ""}
                    </span>
                  </div>
                </Link>
              ))}
            </div>
          </div>
        )}

        {tree && tree.entries.length > 0 && (
          <div>
            <h2 className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground mb-3">
              Entries here — {tree.entries.length}
            </h2>
            <div className="flex flex-col gap-6">
              {tree.entries.map((e) => (
                <article
                  key={e.id}
                  className="border border-border bg-card transition-colors hover:border-primary/30"
                >
                  <header className="flex items-center justify-between gap-3 border-b border-border bg-muted/20 px-4 py-2.5">
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      <FileText className="size-3.5 text-muted-foreground shrink-0" />
                      <Link
                        href={`/entries/${encodeURIComponent(e.id)}`}
                        className="font-mono text-xs font-bold truncate hover:text-primary transition-colors"
                      >
                        {e.id}
                      </Link>
                      <span className="font-mono text-[9px] font-bold uppercase tracking-wider px-1.5 py-0.5 border border-border text-muted-foreground shrink-0">
                        {e.source_id}
                      </span>
                    </div>
                    <Link
                      href={`/entries/${encodeURIComponent(e.id)}`}
                      className="flex items-center gap-1 font-mono text-[10px] font-bold uppercase tracking-[1px] text-muted-foreground hover:text-primary transition-colors shrink-0"
                      title="Open in detail view"
                    >
                      Open <ArrowUpRight className="size-3" />
                    </Link>
                  </header>
                  <div className="px-5 py-4">
                    <MarkdownView content={e.text} />
                  </div>
                </article>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )

  if (isMobile) {
    return (
      <div className="flex h-[calc(100vh-3rem)] flex-col bg-background">
        {content}
        {mobileSidebarOpen && (
          <div className="fixed inset-0 z-50 flex">
            <div
              className="absolute inset-0 bg-background/80 backdrop-blur-sm"
              onClick={() => setMobileSidebarOpen(false)}
            />
            <div className="relative w-72 max-w-[80%] h-full border-r border-border shadow-lg">
              {sidebar}
            </div>
          </div>
        )}
      </div>
    )
  }

  return (
    <div className="h-[calc(100vh-3rem)] bg-background">
      <ResizablePanelGroup direction="horizontal" className="h-full">
        <ResizablePanel defaultSize={22} minSize={15} maxSize={40}>
          {sidebar}
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={78} minSize={40}>
          {content}
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  )
}

interface SourceRootProps {
  tree: SourceTree
  selectedSourceId: string
  selectedPath: string | null
  activeChain: Set<string>
}

function SourceRoot({
  tree,
  selectedSourceId,
  selectedPath,
  activeChain,
}: SourceRootProps) {
  const isActive = tree.sourceId === selectedSourceId
  const [open, setOpen] = useState(isActive)
  const isSelected = isActive && (selectedPath ?? null) === null

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
          href={buildHref(tree.sourceId)}
          className={cn(
            "flex items-center gap-2 flex-1 min-w-0 px-2 py-1.5 font-mono text-xs font-bold uppercase tracking-wider transition-colors hover:bg-card",
            isSelected
              ? "bg-primary/10 text-primary border-l-2 border-primary"
              : "text-foreground border-l-2 border-transparent"
          )}
        >
          {open ? (
            <FolderOpen className="size-3.5 text-primary shrink-0" />
          ) : (
            <Database className="size-3.5 text-muted-foreground shrink-0" />
          )}
          <span className="truncate">{tree.sourceId}</span>
          <span className="ml-auto font-mono text-[10px] text-muted-foreground tabular-nums shrink-0">
            {tree.totalCount}
          </span>
        </Link>
      </div>

      {open && (
        <div className="flex flex-col">
          {tree.roots.map((node) => (
            <WikiTreeNode
              key={node.path}
              sourceId={tree.sourceId}
              node={node}
              selectedSourceId={selectedSourceId}
              selectedPath={selectedPath}
              depth={1}
              activeChain={activeChain}
            />
          ))}
        </div>
      )}
    </div>
  )
}
