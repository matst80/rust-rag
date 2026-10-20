"use client"

import { ListSkeleton } from "@/components/ui/list-skeleton"

import Link from "next/link"
import { useRouter } from "next/navigation"
import { useEffect, useMemo, useRef, useState, type Ref } from "react"
import {
  ArrowUpRight,
  ChevronDown,
  ChevronRight,
  Copy,
  Check,
  Database,
  Eye,
  EyeOff,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  FolderTree,
  GripVertical,
  Inbox,
  Menu,
  Search,
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
import { useEntriesPaths, useEntriesTree, type Entry } from "@/lib/api"
import { useIsMobile } from "@/hooks/use-mobile"
import { useSessionState } from "@/hooks/use-session-state"
import { useWikiMoveEntry } from "@/hooks/use-wiki-move"
import {
  clearEntryDrag,
  hasEntryDragData,
  readEntryDragData,
  setEntryDragData,
} from "@/lib/drag-entry"
import { cn, entryTitle, formatRelativeTime } from "@/lib/utils"
import {
  getShowHiddenSources,
  isHiddenByDefault,
  setShowHiddenSources,
} from "@/lib/sources"
import { EntryTagList } from "@/components/ui/entry-tag"
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
  const [inboxOpen, setInboxOpen] = useSessionState("wiki-inbox-open-v2", false)
  const [sourceFilter, setSourceFilter] = useState("")
  const [showHidden, setShowHidden] = useState(false)
  useEffect(() => {
    setShowHidden(getShowHiddenSources())
  }, [])
  const toggleShowHidden = () => {
    const next = !showHidden
    setShowHidden(next)
    setShowHiddenSources(next)
  }
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
  const { visibleSources, hiddenSources } = useMemo(() => {
    const q = sourceFilter.trim().toLowerCase()
    const matches = (s: SourceTree) => {
      if (!q) return true
      if (s.sourceId.toLowerCase().includes(q)) return true
      return s.roots.some((r) => r.path.toLowerCase().includes(q))
    }
    const visible: SourceTree[] = []
    const hidden: SourceTree[] = []
    for (const s of sourceTrees) {
      if (!matches(s)) continue
      if (isHiddenByDefault(s.sourceId)) hidden.push(s)
      else visible.push(s)
    }
    return { visibleSources: visible, hiddenSources: hidden }
  }, [sourceTrees, sourceFilter])
  const listedSources = showHidden ? [...visibleSources, ...hiddenSources] : visibleSources
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
      <div className="shrink-0 border-b border-border px-3 py-2">
        <div className="relative">
          <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            value={sourceFilter}
            onChange={(e) => setSourceFilter(e.target.value)}
            placeholder="Filter collections / folders…"
            className="h-8 w-full rounded border border-border bg-background pl-7 pr-2 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
        {hiddenSources.length > 0 && (
          <button
            type="button"
            onClick={toggleShowHidden}
            className="mt-1.5 flex w-full items-center gap-1.5 font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground transition-colors hover:text-primary"
            title={showHidden ? "Hide agent memories & todos" : `Show ${hiddenSources.length} hidden (memories, todos)`}
          >
            {showHidden ? <Eye className="size-3" /> : <EyeOff className="size-3" />}
            <span className="flex-1 text-left">
              {showHidden ? "Hide memories" : `Hidden · ${hiddenSources.length}`}
            </span>
          </button>
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
        {pathsResp && sourceTrees.length > 0 && listedSources.length === 0 && (
          <p className="font-mono text-xs text-muted-foreground px-3">
            {sourceFilter.trim()
              ? `No collections match “${sourceFilter.trim()}”.`
              : `${hiddenSources.length} hidden collections (memories, todos). Toggle “Hidden” above to show them.`}
          </p>
        )}
        {listedSources.map((s) => (
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
          <ListSkeleton compact />
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
                <WikiEntryCard
                  key={e.id}
                  entry={e}
                  selected={e.id === selectedEntryId}
                  selectedRef={e.id === selectedEntryId ? selectedEntryRef : undefined}
                />
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

/** Plain-text excerpt for collapsed wiki cards: markdown stripped, whitespace collapsed. */
function cardExcerpt(text: string, max = 420): string {
  const joined = (text ?? "")
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/[*_>]+/g, "")
    .replace(/^\s*[-*]\s+/gm, "• ")
    .replace(/\s+/g, " ")
    .trim()
  return joined.length > max ? `${joined.slice(0, max)}…` : joined
}

function WikiEntryCard({
  entry,
  selected,
  selectedRef,
}: {
  entry: Entry
  selected: boolean
  selectedRef: Ref<HTMLElement> | undefined
}) {
  const [copied, setCopied] = useState(false)
  // Long entries render collapsed (fast folders) unless deep-linked.
  const [expanded, setExpanded] = useState(selected)
  useEffect(() => {
    if (selected) setExpanded(true)
  }, [selected])
  const tags = entry.analysis?.tags ?? []
  const excerpt = cardExcerpt(entry.text)
  const isLong = entry.text.length > 600
  const copyLink = async () => {
    try {
      const url = `${window.location.origin}/entries/${encodeURIComponent(entry.id)}`
      await navigator.clipboard.writeText(url)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      // clipboard unavailable
    }
  }
  return (
    <article
      id={`entry-${entry.id}`}
      ref={selectedRef}
      className={cn(
        "border bg-card transition-colors",
        selected ? "border-primary ring-2 ring-primary/20" : "border-border hover:border-primary/30"
      )}
    >
      <header className="flex items-center justify-between gap-3 border-b border-border bg-muted/20 px-4 py-2.5">
        <div className="flex items-center gap-2 min-w-0 flex-1">
          <div
            draggable
            onDragStart={(evt) => {
              setEntryDragData(evt.dataTransfer, {
                id: entry.id,
                source_id: entry.source_id,
                path: entry.path,
                title: entryTitle(entry, 80),
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
            href={`/entries/${encodeURIComponent(entry.id)}`}
            className="text-sm font-semibold truncate hover:text-primary transition-colors"
            title={entry.id}
          >
            {entryTitle(entry)}
          </Link>
          <span className="font-mono text-[9px] font-bold uppercase tracking-wider px-1.5 py-0.5 border border-border text-muted-foreground shrink-0">
            {entry.source_id}
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={copyLink}
            title="Copy link to this entry"
            className="flex size-7 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-primary"
          >
            {copied ? <Check className="size-3.5 text-emerald-500" /> : <Copy className="size-3.5" />}
          </button>
          <Link
            href={`/entries/${encodeURIComponent(entry.id)}?tab=graph`}
            className="hidden sm:flex items-center gap-1 font-mono text-[10px] font-bold uppercase tracking-[1px] text-muted-foreground hover:text-primary transition-colors px-1.5"
            title="Open links / graph for this entry"
          >
            Links
          </Link>
          <Link
            href={`/entries/${encodeURIComponent(entry.id)}`}
            className="flex items-center gap-1 font-mono text-[10px] font-bold uppercase tracking-[1px] text-muted-foreground hover:text-primary transition-colors"
            title="Open in detail view"
          >
            Open <ArrowUpRight className="size-3" />
          </Link>
        </div>
      </header>
      {(tags.length > 0 || entry.path) && (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-border/60 px-4 py-1.5">
          {entry.path && (
            <span className="font-mono text-[10px] text-muted-foreground" title="Wiki path">
              {entry.path}
            </span>
          )}
          {tags.length > 0 && <EntryTagList tags={tags.slice(0, 6)} />}
          <span className="ml-auto font-mono text-[10px] text-muted-foreground/70" title={`Updated ${new Date(entry.updated_at).toLocaleString()}`}>
            {formatRelativeTime(entry.updated_at)}
          </span>
        </div>
      )}
      <div className="px-5 py-4">
        {expanded || !isLong ? (
          <MarkdownView content={entry.text} />
        ) : (
          <p className="whitespace-pre-wrap text-sm leading-relaxed text-foreground/80">{excerpt}</p>
        )}
        {isLong && (
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            className="mt-3 font-mono text-[10px] font-bold uppercase tracking-[1px] text-muted-foreground transition-colors hover:text-primary"
          >
            {expanded ? "Show less" : "Read full entry"}
          </button>
        )}
      </div>
    </article>
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
          "flex items-center transition-colors rounded-sm border-l-2",
          isSelected ? "bg-primary/10 border-primary" : "border-transparent",
          isDragOver && "bg-primary/10 ring-2 ring-primary"
        )}
        style={{ paddingLeft: "0.25rem" }}
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
          className="shrink-0 flex items-center justify-center size-5 text-muted-foreground hover:text-foreground transition-colors"
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
            "flex items-center gap-2 flex-1 min-w-0 px-2 py-1.5 font-mono text-xs font-bold uppercase tracking-wider transition-colors rounded-sm hover:bg-card",
            isSelected ? "text-primary" : "text-foreground"
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
