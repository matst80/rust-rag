"use client"

import Link from "next/link"
import { useState } from "react"
import {
  ChevronRight,
  FileText,
  Folder,
  FolderTree,
  Menu,
  X,
} from "lucide-react"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { Button } from "@/components/ui/button"
import { useCategories, useEntriesTree } from "@/lib/api"
import { useIsMobile } from "@/hooks/use-mobile"
import { cn } from "@/lib/utils"
import { WikiSourceRoot } from "./wiki-source-root"

function buildHref(sourceId: string, path?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  return `/wiki?${params.toString()}`
}

interface EntryTreeProps {
  sourceId: string
  prefix?: string
}

export function EntryTree({ sourceId, prefix }: EntryTreeProps) {
  const isMobile = useIsMobile()
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false)
  const { data: categories } = useCategories()
  const { data: tree, isLoading } = useEntriesTree(sourceId, prefix)

  const segments = prefix ? prefix.split("/") : []

  // Render every category as a candidate root; WikiSourceRoot self-hides when
  // the source has no path-bearing data.
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
        {!categories && (
          <p className="font-mono text-xs text-muted-foreground px-3">Loading…</p>
        )}
        {categories
          ?.filter((c) => c.count > 0)
          .map((c) => (
            <WikiSourceRoot
              key={c.id}
              sourceId={c.id}
              selectedSourceId={sourceId}
              selectedPath={prefix ?? null}
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
        {isLoading && (
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
              Entries here
            </h2>
            <div className="flex flex-col gap-2">
              {tree.entries.map((e) => (
                <Link
                  key={e.id}
                  href={`/entries/${encodeURIComponent(e.id)}`}
                  className="flex items-center gap-3 border border-border bg-card p-3 hover:border-primary/40 transition-colors"
                >
                  <FileText className="size-4 text-muted-foreground shrink-0" />
                  <div className="flex flex-col min-w-0 flex-1">
                    <span className="font-mono text-xs font-bold truncate">
                      {e.id}
                    </span>
                    <span className="font-mono text-[10px] text-muted-foreground line-clamp-1">
                      {e.text.slice(0, 200)}
                    </span>
                  </div>
                </Link>
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
