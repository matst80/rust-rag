"use client"

import Link from "next/link"
import { useRouter } from "next/navigation"
import { useEffect, useMemo, useRef, useState } from "react"
import {
  ArrowUpRight,
  ChevronDown,
  ChevronRight,
  Database,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  FolderTree,
  GripVertical,
  Inbox,
  Menu,
  X,
} from "lucide-react"
import { MarkdownView } from "@/components/entries/markdown-view"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import {
  Sheet,
  SheetContent,
  SheetTitle,
} from "@/components/ui/sheet"
import { Button } from "@/components/ui/button"
import { useEntriesPaths, useEntriesTree } from "@/lib/api"
import { useIsMobile } from "@/hooks/use-mobile"
import { useSessionState } from "@/hooks/use-session-state"
import { useWikiMoveEntry } from "@/hooks/use-wiki-move"
import {
  clearEntryDrag,
  hasEntryDragData,
  readEntryDragData,
  setEntryDragData,
} from "@/lib/drag-entry"
import { cn, entryTitle } from "@/lib/utils"
import {
  ancestorChain,
  buildSourceTrees,
  wikiHref,
  type SourceTree,
} from "@/lib/wiki-tree"
import { WikiTreeNode, type TreeNodeData } from "./wiki-tree-node"
import { UnorganizedInbox } from "./unorganized-inbox"
import { EntryDragBanner } from "./entry-drag-banner"

interface EntryTreeProps {
  sourceId: string
  prefix?: string
  selectedEntryId?: string
}

export function EntryTree({ sourceId, prefix, selectedEntryId }: EntryTreeProps) {
  const router = useRouter()
  const isMobile = useIsMobile()
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false)
  const [mobileInboxOpen, setMobileInboxOpen] = useState(false)
  const [inboxOpen, setInboxOpen] = useSessionState("wiki-inbox-open", true)
  const { data: pathsResp } = useEntriesPaths()
  const { data: tree, isLoading: treeLoading } = useEntriesTree(sourceId, prefix)
  const [activeDropTarget, setActiveDropTarget] = useState<string | null>(null)
  const [isCreatingFolder, setIsCreatingFolder] = useState(false)
  const [newFolderName, setNewFolderName] = useState("")
  const [isPageDropTarget, setIsPageDropTarget] = useState(false)
  const handleDropEntry = useWikiMoveEntry()

  const sourceTrees = useMemo(
    () => (pathsResp ? buildSourceTrees(pathsResp.paths) : []),
    [pathsResp]
  )
  const activeChain = useMemo(() => ancestorChain(prefix), [prefix])
  const segments = prefix ? prefix.split("/") : []

  const selectedEntryRef = useRef<HTMLElement | null>(null)
  useEffect(() => {
    if (selectedEntryId && tree) {
      selectedEntryRef.current?.scrollIntoView({ behavior: "smooth", block: "start" })
    }
  }, [selectedEntryId, tree])

  const handleCreateFolder = () => {
    const slug = newFolderName.trim().replace(/^\/+|\/+$/g, "")
    if (!slug) return
    const newPath = prefix ? `${prefix}/${slug}` : slug
    setIsCreatingFolder(false)
    setNewFolderName("")
    // Folders are virtual — navigating here is enough; it becomes real (and
    // shows up in the tree) the moment an entry is dropped onto it.
    router.push(wikiHref(sourceId, newPath))
  }

  const wikiSidebar = (
    <div className="flex h-full flex-col bg-background">
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
        <FolderTree className="size-4 text-primary" />
        <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
          Wiki
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
      <div className="min-h-0 flex-1 overflow-y-auto py-2">
        {!pathsResp && (
          <p className="font-mono text-xs text-muted-foreground px-3">Loading…</p>
        )}
        {pathsResp && sourceTrees.length === 0 && (
          <p className="font-mono text-xs text-muted-foreground px-3">
            No entries with paths yet. Drag entries here or set a `path` on an entry to populate the wiki.
          </p>
        )}
        {sourceTrees.map((s) => (
          <SourceRoot
            key={s.sourceId}
            tree={s}
            selectedSourceId={sourceId}
            selectedPath={prefix ?? null}
            selectedEntryId={selectedEntryId ?? null}
            activeChain={activeChain}
            onDropEntry={handleDropEntry}
          />
        ))}
      </div>
    </div>
  )

  const content = (
    <div className="flex h-full flex-col bg-background">
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        {isMobile && (
          <>
            <Button
              variant="ghost"
              size="icon"
              className="size-8"
              onClick={() => setMobileSidebarOpen(true)}
              aria-label="Open sidebar"
            >
              <Menu className="size-4" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="size-8"
              onClick={() => setMobileInboxOpen(true)}
              aria-label="Open unorganized inbox"
            >
              <Inbox className="size-4" />
            </Button>
          </>
        )}
        <nav className="flex items-center gap-1 font-mono text-xs flex-wrap min-w-0">
          <Link
            href={wikiHref(sourceId)}
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
                    href={wikiHref(sourceId, sub)}
                    className="text-muted-foreground hover:text-primary"
                  >
                    {seg}
                  </Link>
                )}
              </span>
            )
          })}
        </nav>

        <div className="ml-auto flex items-center gap-2">
          {isCreatingFolder ? (
            <form
              className="flex items-center gap-1.5"
              onSubmit={(e) => {
                e.preventDefault()
                handleCreateFolder()
              }}
            >
              <span className="font-mono text-xs text-muted-foreground">
                {prefix ? `${prefix}/` : ""}
              </span>
              <input
                autoFocus
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    setIsCreatingFolder(false)
                    setNewFolderName("")
                  }
                }}
                onBlur={() => {
                  if (!newFolderName.trim()) setIsCreatingFolder(false)
                }}
                placeholder="folder-name"
                className="h-7 w-36 rounded border border-border bg-background px-2 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
              />
              <Button type="submit" size="sm" className="h-7 px-2 text-xs">
                Create
              </Button>
            </form>
          ) : (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground"
              onClick={() => setIsCreatingFolder(true)}
            >
              <FolderPlus className="size-3.5 mr-1.5" />
              New folder
            </Button>
          )}
        </div>
      </div>

      <div
        className={cn(
          "flex-1 overflow-y-auto p-4 transition-colors",
          isPageDropTarget && "bg-primary/5 ring-1 ring-inset ring-primary/30",
        )}
        onDragOver={(e) => {
          if (!hasEntryDragData(e.dataTransfer)) return
          e.preventDefault()
          e.dataTransfer.dropEffect = "move"
          if (!isPageDropTarget) setIsPageDropTarget(true)
        }}
        onDragLeave={(e) => {
          e.preventDefault()
          if (e.currentTarget === e.target) setIsPageDropTarget(false)
        }}
        onDrop={(e) => {
          e.preventDefault()
          setIsPageDropTarget(false)
          const payload = readEntryDragData(e.dataTransfer)
          if (payload) handleDropEntry(sourceId, prefix ?? "", payload.id)
        }}
      >
        {treeLoading && (
          <p className="font-mono text-xs text-muted-foreground">Loading…</p>
        )}

        {tree && tree.children.length === 0 && tree.entries.length === 0 && (
          <div className="flex flex-col items-center justify-center py-16 text-center pointer-events-none">
            <Folder className="size-8 mb-3 text-muted-foreground/30" />
            <p className="font-mono text-xs text-muted-foreground">
              {prefix
                ? "No entries here yet. Drag an unorganized entry anywhere in this view to file it here."
                : "No entries under this path. Drag an entry onto a folder or source to organize it here."}
            </p>
          </div>
        )}

        {tree && tree.children.length > 0 && (
          <div className="mb-6">
            <h2 className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground mb-3">
              Folders
            </h2>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
              {tree.children.map((c) => {
                const isTarget = activeDropTarget === c.path
                return (
                  <Link
                    key={c.path}
                    href={wikiHref(sourceId, c.path)}
                    onDragOver={(e) => {
                      if (!hasEntryDragData(e.dataTransfer)) return
                      e.preventDefault()
                      e.stopPropagation()
                      e.dataTransfer.dropEffect = "move"
                      if (activeDropTarget !== c.path) setActiveDropTarget(c.path)
                    }}
                    onDragLeave={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      if (activeDropTarget === c.path) setActiveDropTarget(null)
                    }}
                    onDrop={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      setActiveDropTarget(null)
                      const payload = readEntryDragData(e.dataTransfer)
                      if (payload) handleDropEntry(sourceId, c.path, payload.id)
                    }}
                    className={cn(
                      "flex items-center gap-3 border border-border bg-card p-3 hover:border-primary/40 transition-all",
                      isTarget && "border-primary ring-2 ring-primary/20 bg-primary/5 scale-[1.01]"
                    )}
                  >
                    <Folder className={cn("size-4 shrink-0", isTarget ? "text-primary animate-bounce" : "text-primary")} />
                    <div className="flex flex-col min-w-0 flex-1">
                      <span className="font-mono text-xs font-bold truncate">
                        {c.segment}
                      </span>
                      <span className="font-mono text-[10px] text-muted-foreground">
                        {c.count} entr{c.count === 1 ? "y" : "ies"}
                        {c.has_children ? " · subfolders" : ""}
                      </span>
                      {isTarget && (
                        <span className="font-mono text-[9px] font-bold uppercase tracking-wider text-primary">
                          Move here
                        </span>
                      )}
                    </div>
                  </Link>
                )
              })}
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
                  id={`entry-${e.id}`}
                  ref={e.id === selectedEntryId ? selectedEntryRef : undefined}
                  className={cn(
                    "border bg-card transition-colors",
                    e.id === selectedEntryId
                      ? "border-primary ring-2 ring-primary/20"
                      : "border-border hover:border-primary/30"
                  )}
                >
                  <header className="flex items-center justify-between gap-3 border-b border-border bg-muted/20 px-4 py-2.5">
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      <div
                        draggable
                        onDragStart={(evt) => {
                          setEntryDragData(evt.dataTransfer, {
                            id: e.id,
                            source_id: e.source_id,
                            path: e.path,
                            title: entryTitle(e, 80),
                          })
                        }}
                        onDragEnd={() => clearEntryDrag()}
                        className="cursor-grab active:cursor-grabbing text-muted-foreground/40 hover:text-primary transition-colors p-0.5"
                        title="Drag entry to re-organize into another Wiki folder"
                      >
                        <GripVertical className="size-3.5" />
                      </div>
                      <FileText className="size-3.5 text-muted-foreground shrink-0" />
                      <Link
                        href={`/entries/${encodeURIComponent(e.id)}`}
                        className="text-sm font-semibold truncate hover:text-primary transition-colors"
                        title={e.id}
                      >
                        {entryTitle(e)}
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
        <EntryDragBanner />
        {content}
        {mobileSidebarOpen && (
          <div className="fixed inset-0 z-50 flex">
            <div
              className="absolute inset-0 bg-background/80 backdrop-blur-sm"
              onClick={() => setMobileSidebarOpen(false)}
            />
            <div className="relative w-72 max-w-[80%] h-full border-r border-border shadow-lg">
              {wikiSidebar}
            </div>
          </div>
        )}
        <Sheet open={mobileInboxOpen} onOpenChange={setMobileInboxOpen}>
          <SheetContent side="left" className="flex flex-col gap-0 p-0 sm:max-w-sm">
            <SheetTitle className="sr-only">Unorganized inbox</SheetTitle>
            <UnorganizedInbox className="min-h-0 flex-1" />
          </SheetContent>
        </Sheet>
      </div>
    )
  }

  return (
    <div className="flex h-[calc(100vh-3rem)] bg-background">
      <EntryDragBanner />
      {!inboxOpen && (
        <button
          type="button"
          onClick={() => setInboxOpen(true)}
          title="Show unorganized inbox"
          className="flex w-9 shrink-0 flex-col items-center gap-3 border-r border-border pt-3 text-muted-foreground transition-colors hover:text-primary"
        >
          <Inbox className="size-4" />
          <span className="font-mono text-[9px] font-bold uppercase tracking-widest [writing-mode:vertical-rl]">
            Inbox
          </span>
        </button>
      )}
      <ResizablePanelGroup direction="horizontal" className="h-full min-w-0 flex-1">
        {inboxOpen && (
          <>
            <ResizablePanel id="inbox" order={1} defaultSize={26} minSize={16} maxSize={45}>
              <UnorganizedInbox onCollapse={() => setInboxOpen(false)} className="h-full" />
            </ResizablePanel>
            <ResizableHandle withHandle />
          </>
        )}
        <ResizablePanel id="tree" order={2} defaultSize={inboxOpen ? 20 : 24} minSize={14} maxSize={40}>
          {wikiSidebar}
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel id="content" order={3} defaultSize={inboxOpen ? 54 : 50} minSize={40}>
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
  selectedEntryId: string | null
  activeChain: Set<string>
  onDropEntry?: (targetSourceId: string, targetPath: string, entryId: string) => void
}

function SourceRoot({
  tree,
  selectedSourceId,
  selectedPath,
  selectedEntryId,
  activeChain,
  onDropEntry,
}: SourceRootProps) {
  const isActive = tree.sourceId === selectedSourceId
  const [open, setOpen] = useState(isActive)
  const [isDragOver, setIsDragOver] = useState(false)
  const autoExpandTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const isSelected = isActive && (selectedPath ?? null) === null

  const startAutoExpand = () => {
    if (autoExpandTimer.current) return
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
          // Drop onto root of source
          if (payload && onDropEntry) onDropEntry(tree.sourceId, "", payload.id)
        }}
      >
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
          href={wikiHref(tree.sourceId)}
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
          {isDragOver ? (
            <span className="ml-auto shrink-0 rounded bg-primary px-1.5 py-0.5 font-mono text-[9px] font-bold uppercase text-primary-foreground">
              Move here
            </span>
          ) : (
            <span className="ml-auto font-mono text-[10px] text-muted-foreground tabular-nums shrink-0">
              {tree.totalCount}
            </span>
          )}
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
              selectedEntryId={selectedEntryId}
              depth={1}
              activeChain={activeChain}
              onDropEntry={onDropEntry}
            />
          ))}
        </div>
      )}
    </div>
  )
}
