"use client"

import Link from "next/link"
import { useCallback, useMemo, useRef, useState } from "react"
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  FileText,
  FolderInput,
  GitBranch,
  Inbox,
  Link2,
  Loader2,
  PanelLeftClose,
  Search,
  Sparkles,
  X,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card"
import { EntryPeek } from "@/components/entries/entry-peek"
import { EntryTagList } from "@/components/ui/entry-tag"
import { WikiPathTreePicker } from "@/components/entries/wiki-path-tree-picker"
import { useEdges, useItems } from "@/lib/api"
import type { Edge, Entry } from "@/lib/api"
import { useWikiMoveEntry } from "@/hooks/use-wiki-move"
import {
  clearEntryDrag,
  hasEntryDragData,
  readEntryDragData,
  setEntryDragData,
  useDraggingEntry,
} from "@/lib/drag-entry"
import { cn, edgeEndpointTitle, entryTitle, formatRelativeTime } from "@/lib/utils"

interface GraphNode {
  entry: Entry
  children: GraphNode[]
  /** Titles of connected entries that already live in the wiki structure — a filing hint. */
  linkedOrganized: { id: string; title: string }[]
}

/** Similarity edges below this weight are too weak to imply hierarchy. */
const SIMILARITY_MIN_WEIGHT = 0.6

/** Lower sorts first: deliberate links, then structural ontology relations,
 *  then strong similarity, then everything else. Within a class, heavier
 *  wins — the best edge implies parenthood, not whichever came first. */
function edgeRank(edge: Edge): number {
  if (edge.edge_type === "manual") return 0
  const rel = (edge.relationship ?? "").toLowerCase()
  if (rel === "contains" || rel === "part_of" || rel === "has_part") return 1
  if (edge.edge_type === "similarity") {
    return edge.weight >= SIMILARITY_MIN_WEIGHT ? 2 : 10
  }
  return 3
}

function buildGraphTree(entries: Entry[], edges: Edge[]): GraphNode[] {
  const byId = new Map(entries.map((e) => [e.id, e]))
  const parentOf = new Map<string, string>()
  const childrenOf = new Map<string, string[]>()
  const linkedOrganized = new Map<string, { id: string; title: string }[]>()

  const isAncestor = (candidateAncestor: string, id: string): boolean => {
    let cur: string | undefined = id
    const seen = new Set<string>()
    while (cur && !seen.has(cur)) {
      seen.add(cur)
      if (cur === candidateAncestor) return true
      cur = parentOf.get(cur)
    }
    return false
  }

  const usable = edges
    .filter((e) => edgeRank(e) < 10)
    .sort((a, b) => edgeRank(a) - edgeRank(b) || b.weight - a.weight)

  for (const edge of usable) {
    const sourceIn = byId.has(edge.source_id)
    const targetIn = byId.has(edge.target_id)

    if (sourceIn && targetIn) {
      // Both unorganized: stronger edge wins parenthood, cycles broken.
      if (
        edge.source_id !== edge.target_id &&
        !parentOf.has(edge.target_id) &&
        !isAncestor(edge.target_id, edge.source_id)
      ) {
        parentOf.set(edge.target_id, edge.source_id)
        childrenOf.set(edge.source_id, [...(childrenOf.get(edge.source_id) ?? []), edge.target_id])
      }
      continue
    }

    // One side already organized (has a wiki path) — surface as a filing hint, not a tree edge.
    if (sourceIn && !targetIn) {
      const list = linkedOrganized.get(edge.source_id) ?? []
      list.push({ id: edge.target_id, title: edgeEndpointTitle(edge.target_id, edge.target_title) })
      linkedOrganized.set(edge.source_id, list)
    } else if (targetIn && !sourceIn) {
      const list = linkedOrganized.get(edge.target_id) ?? []
      list.push({ id: edge.source_id, title: edgeEndpointTitle(edge.source_id, edge.source_title) })
      linkedOrganized.set(edge.target_id, list)
    }
  }

  const buildNode = (id: string): GraphNode => ({
    entry: byId.get(id)!,
    children: (childrenOf.get(id) ?? []).map(buildNode),
    linkedOrganized: linkedOrganized.get(id) ?? [],
  })

  const roots = entries.filter((e) => !parentOf.has(e.id)).map((e) => buildNode(e.id))
  // Clusters float to the top so the detected structure is visible first;
  // loose entries follow, newest first.
  roots.sort((a, b) => {
    const aCluster = a.children.length > 0 ? 0 : 1
    const bCluster = b.children.length > 0 ? 0 : 1
    if (aCluster !== bCluster) return aCluster - bCluster
    return b.entry.created_at - a.entry.created_at
  })
  return roots
}

/** Plain-text excerpt for the hover preview: markdown stripped, whitespace collapsed. */
function plainExcerpt(text: string, max = 340): string {
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

interface UnorganizedInboxProps {
  sourceId?: string
  className?: string
  /** When set, the header shows a button to collapse the inbox panel. */
  onCollapse?: () => void
}

/** The "inbox" of entries with no wiki path, clustered by their graph edges.
 *  Hover a row for a preview, click for a pinned one; drag a row onto a
 *  folder in the wiki tree to file it, or drop a filed entry back here to
 *  unfile it. */
export function UnorganizedInbox({ sourceId, className, onCollapse }: UnorganizedInboxProps) {
  const { data: itemsResp, isLoading } = useItems({ has_path: false, source_id: sourceId, limit: 300 })
  const { data: edges } = useEdges()
  const handleMove = useWikiMoveEntry()

  const [query, setQuery] = useState("")
  const [expandOverrides, setExpandOverrides] = useState<Record<string, boolean>>({})
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [movingId, setMovingId] = useState<string | null>(null)
  const [isUnfileTarget, setIsUnfileTarget] = useState(false)
  const [moveToEntry, setMoveToEntry] = useState<Entry | null>(null)
  const [moveToPath, setMoveToPath] = useState("")

  // Off-screen drag ghost: a styled, readable stand-in for the browser's
  // default drag snapshot. Content is set imperatively at dragstart so
  // setDragImage sees it immediately.
  const ghostRef = useRef<HTMLDivElement>(null)
  const ghostTitleRef = useRef<HTMLSpanElement>(null)

  const entries = useMemo(() => itemsResp?.items ?? [], [itemsResp])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return entries
    return entries.filter(
      (e) => entryTitle(e).toLowerCase().includes(q) || (e.text ?? "").toLowerCase().includes(q)
    )
  }, [entries, query])

  const roots = useMemo(() => buildGraphTree(filtered, edges ?? []), [filtered, edges])

  const expandableIds = useMemo(() => {
    const ids: string[] = []
    const walk = (n: GraphNode) => {
      if (n.children.length > 0) {
        ids.push(n.entry.id)
        n.children.forEach(walk)
      }
    }
    roots.forEach(walk)
    return ids
  }, [roots])

  const totalCount = itemsResp?.total_count ?? entries.length
  const capped = totalCount > entries.length

  const selectedEntry = selectedId ? (entries.find((e) => e.id === selectedId) ?? null) : null

  // Nodes default to open when they have children; overrides win.
  const isOpen = useCallback(
    (node: GraphNode) => expandOverrides[node.entry.id] ?? node.children.length > 0,
    [expandOverrides]
  )

  const toggleOpen = useCallback((id: string) => {
    setExpandOverrides((prev) => ({ ...prev, [id]: !(prev[id] ?? true) }))
  }, [])

  const moveEntry = useCallback(
    async (entry: Entry, targetPath: string) => {
      setMovingId(entry.id)
      try {
        await handleMove(entry.source_id, targetPath, entry.id)
      } finally {
        setMovingId(null)
      }
    },
    [handleMove]
  )

  const prepareGhost = useCallback((title: string): HTMLElement | null => {
    if (ghostTitleRef.current) ghostTitleRef.current.textContent = title
    return ghostRef.current
  }, [])

  if (isLoading) {
    return (
      <div className={cn("flex items-center gap-2 px-3 py-4 text-sm text-muted-foreground", className)}>
        <Loader2 className="size-4 animate-spin" /> Loading…
      </div>
    )
  }

  if (entries.length === 0) {
    return (
      <div className={cn("flex flex-col items-center gap-2 px-4 py-8 text-center", className)}>
        <GitBranch className="size-5 text-muted-foreground/30" />
        <p className="text-sm text-muted-foreground">
          Nothing unorganized — every entry has a wiki path.
        </p>
      </div>
    )
  }

  return (
    <div className={cn("flex h-full min-h-0 flex-col", className)}>
      {/* Header: count, expand/collapse all, filter */}
      <div className="shrink-0 border-b border-border px-3 pb-2.5 pt-3">
        <div className="flex items-center gap-2">
          <Inbox className="size-4 shrink-0 text-primary" />
          <span className="font-mono text-[11px] font-bold uppercase tracking-widest text-muted-foreground">
            Inbox
          </span>
          <span
            className="ml-auto font-mono text-[10px] tabular-nums text-muted-foreground"
            title={capped ? `${totalCount} unorganized, showing first ${entries.length}` : undefined}
          >
            {filtered.length === entries.length ? entries.length : `${filtered.length}/${entries.length}`}
            {capped ? ` of ${totalCount}` : ""}
          </span>
          {expandableIds.length > 0 && (
            <>
              <Button
                variant="ghost"
                size="icon"
                className="size-6"
                onClick={() => setExpandOverrides(Object.fromEntries(expandableIds.map((id) => [id, true])))}
                title="Expand all"
              >
                <ChevronsUpDown className="size-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="size-6"
                onClick={() => setExpandOverrides(Object.fromEntries(expandableIds.map((id) => [id, false])))}
                title="Collapse all"
              >
                <ChevronsDownUp className="size-3.5" />
              </Button>
            </>
          )}
          {onCollapse && (
            <Button
              variant="ghost"
              size="icon"
              className="size-6"
              onClick={onCollapse}
              title="Hide inbox"
              aria-label="Hide inbox"
            >
              <PanelLeftClose className="size-3.5" />
            </Button>
          )}
        </div>
        <div className="relative mt-2">
          <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Filter entries…"
            className="h-8 pl-7 text-sm"
          />
        </div>
        <p className="mt-2 text-[11px] leading-snug text-muted-foreground">
          Hover to preview, click to pin. Drag onto a folder in the wiki tree to file it; drop back here to unfile.
        </p>
      </div>

      {/* Drop zone: dropping a filed entry here removes its wiki path */}
      <div
        className={cn(
          "min-h-0 flex-1 overflow-y-auto py-1 transition-colors",
          isUnfileTarget && "bg-primary/5 ring-2 ring-inset ring-primary/40"
        )}
        onDragOver={(evt) => {
          if (!hasEntryDragData(evt.dataTransfer)) return
          evt.preventDefault()
          evt.dataTransfer.dropEffect = "move"
          if (!isUnfileTarget) setIsUnfileTarget(true)
        }}
        onDragLeave={(evt) => {
          if (evt.currentTarget === evt.target) setIsUnfileTarget(false)
        }}
        onDrop={async (evt) => {
          if (!hasEntryDragData(evt.dataTransfer)) return
          evt.preventDefault()
          setIsUnfileTarget(false)
          const payload = readEntryDragData(evt.dataTransfer)
          if (payload) await handleMove(payload.source_id, "", payload.id)
        }}
      >
        {isUnfileTarget && (
          <div className="pointer-events-none sticky top-0 z-10 mx-2 mb-1 rounded border border-primary/40 bg-background/95 px-2 py-1 text-center font-mono text-[10px] font-bold uppercase tracking-wider text-primary">
            Drop to unfile
          </div>
        )}

        {filtered.length === 0 && (
          <p className="px-3 py-6 text-center text-sm text-muted-foreground">
            No entries match “{query.trim()}”.
          </p>
        )}

        {roots.map((node) => (
          <InboxRow
            key={node.entry.id}
            node={node}
            depth={0}
            isOpen={isOpen}
            onToggleOpen={toggleOpen}
            selectedId={selectedId}
            onSelect={setSelectedId}
            movingId={movingId}
            onMoveTo={(entry) => {
              setMoveToEntry(entry)
              setMoveToPath(entry.path ?? "")
            }}
            prepareGhost={prepareGhost}
          />
        ))}
      </div>

      {/* Pinned preview of the selected entry */}
      {selectedEntry && (
        <div className="flex h-[45%] shrink-0 flex-col border-t border-border bg-muted/10">
          <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 px-2">
            <FileText className="size-3.5 shrink-0 text-primary" />
            <span className="min-w-0 flex-1 truncate text-xs font-semibold">
              {entryTitle(selectedEntry, 80)}
            </span>
            <Link
              href={`/entries/${encodeURIComponent(selectedEntry.id)}`}
              className="shrink-0 font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground transition-colors hover:text-primary"
            >
              Open
            </Link>
            <Button
              variant="ghost"
              size="icon"
              className="size-6 shrink-0"
              onClick={() => setSelectedId(null)}
              title="Close preview"
            >
              <X className="size-3.5" />
            </Button>
          </div>
          <EntryPeek entry={selectedEntry} className="min-h-0 flex-1" />
        </div>
      )}

      {/* Move-to-folder fallback (touch, keyboard, or when dragging is awkward) */}
      <Dialog open={moveToEntry !== null} onOpenChange={(open) => !open && setMoveToEntry(null)}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle className="truncate text-base">
              Move “{moveToEntry ? entryTitle(moveToEntry, 60) : ""}”…
            </DialogTitle>
            <DialogDescription>
              Pick a destination folder in source{" "}
              <span className="font-mono">{moveToEntry?.source_id}</span>.
            </DialogDescription>
          </DialogHeader>
          <WikiPathTreePicker
            sourceId={moveToEntry?.source_id ?? ""}
            value={moveToPath}
            onChange={setMoveToPath}
          />
          <DialogFooter>
            <Button variant="ghost" size="sm" onClick={() => setMoveToEntry(null)}>
              Cancel
            </Button>
            <Button
              size="sm"
              disabled={moveToEntry !== null && moveToPath === (moveToEntry.path ?? "")}
              onClick={async () => {
                if (!moveToEntry) return
                await moveEntry(moveToEntry, moveToPath)
                setMoveToEntry(null)
              }}
            >
              Move here
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Hidden drag image template, styled larger than tree rows */}
      <div
        ref={ghostRef}
        aria-hidden
        className="pointer-events-none fixed left-[-400px] top-[-400px] z-[-1] flex max-w-[320px] items-center gap-2 rounded-md border border-primary/50 bg-card px-3 py-2 shadow-xl"
      >
        <FileText className="size-4 shrink-0 text-primary" />
        <span ref={ghostTitleRef} className="truncate text-sm font-medium" />
      </div>
    </div>
  )
}

interface InboxRowProps {
  node: GraphNode
  depth: number
  isOpen: (node: GraphNode) => boolean
  onToggleOpen: (id: string) => void
  selectedId: string | null
  onSelect: (id: string | null) => void
  movingId: string | null
  onMoveTo: (entry: Entry) => void
  prepareGhost: (title: string) => HTMLElement | null
}

function InboxRow({
  node,
  depth,
  isOpen,
  onToggleOpen,
  selectedId,
  onSelect,
  movingId,
  onMoveTo,
  prepareGhost,
}: InboxRowProps) {
  const { entry } = node
  const open = isOpen(node)
  const expandable = node.children.length > 0
  const isSelected = selectedId === entry.id
  const isDragging = useDraggingEntry()?.id === entry.id
  const isMoving = movingId === entry.id
  const title = entryTitle(entry, 80)

  return (
    <div className="flex flex-col">
      <div
        draggable
        onDragStart={(evt) => {
          const ghost = prepareGhost(title)
          if (ghost) evt.dataTransfer.setDragImage(ghost, 24, 18)
          setEntryDragData(evt.dataTransfer, {
            id: entry.id,
            source_id: entry.source_id,
            path: entry.path,
            title,
          })
        }}
        onDragEnd={() => clearEntryDrag()}
        onClick={() => onSelect(isSelected ? null : entry.id)}
        style={{ paddingLeft: `calc(${depth} * 0.9rem + 0.25rem)` }}
        className={cn(
          "group flex cursor-grab items-start gap-1 rounded-sm pr-1 transition-colors hover:bg-card/80 active:cursor-grabbing",
          isSelected && "bg-primary/10",
          isDragging && "opacity-40"
        )}
      >
        {expandable ? (
          <button
            type="button"
            onClick={(evt) => {
              evt.stopPropagation()
              onToggleOpen(entry.id)
            }}
            className="mt-1 flex size-6 shrink-0 items-center justify-center text-muted-foreground transition-colors hover:text-foreground"
            aria-label={open ? "Collapse" : "Expand"}
          >
            {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
          </button>
        ) : (
          <span className="mt-1 flex size-6 shrink-0 items-center justify-center">
            <span className="size-1.5 rounded-full bg-muted-foreground/30" />
          </span>
        )}

        {/* Title + meta double as the hover-preview trigger */}
        <HoverCard openDelay={400} closeDelay={80}>
          <HoverCardTrigger asChild>
            <div className="flex min-w-0 flex-1 cursor-help flex-col py-1.5">
              <span className={cn("truncate text-sm leading-5", isSelected && "font-medium text-primary")}>
                {title}
              </span>
              <span className="truncate pl-0.5 text-[11px] leading-4 text-muted-foreground/80">
                {entry.source_id} · {formatRelativeTime(entry.created_at)}
                {expandable ? ` · ${node.children.length} linked` : ""}
              </span>
            </div>
          </HoverCardTrigger>
          <HoverCardContent side="right" align="start" className="w-80 p-0">
            <div className="border-b border-border/60 px-3 py-2">
              <p className="text-sm font-medium leading-snug">{entryTitle(entry, 200)}</p>
              <p className="mt-1 font-mono text-[10px] uppercase tracking-wide text-muted-foreground">
                {entry.source_id} · created {formatRelativeTime(entry.created_at)}
                {entry.updated_at > entry.created_at + 1000
                  ? ` · updated ${formatRelativeTime(entry.updated_at)}`
                  : ""}
              </p>
            </div>
            <div className="max-h-52 overflow-y-auto px-3 py-2">
              {entry.analysis?.summary && (
                <p className="mb-2 flex gap-1.5 text-xs italic leading-relaxed text-foreground/80">
                  <Sparkles className="mt-0.5 size-3 shrink-0 text-primary" />
                  {entry.analysis.summary}
                </p>
              )}
              <p className="whitespace-pre-wrap text-xs leading-relaxed text-muted-foreground">
                {plainExcerpt(entry.text) || "Empty entry."}
              </p>
            </div>
            {entry.analysis?.tags && entry.analysis.tags.length > 0 && (
              <div className="border-t border-border/60 px-3 py-1.5">
                <EntryTagList tags={entry.analysis.tags} />
              </div>
            )}
          </HoverCardContent>
        </HoverCard>

        {node.linkedOrganized.length > 0 && (
          <span
            className="mt-1.5 flex shrink-0 items-center gap-0.5 rounded border border-border/60 bg-muted/20 px-1 py-0.5 text-[9px] text-muted-foreground"
            title={`Already linked to: ${node.linkedOrganized
              .slice(0, 3)
              .map((l) => l.title)
              .join(", ")}`}
          >
            <Link2 className="size-2.5" />
            {node.linkedOrganized.length}
          </span>
        )}

        <button
          type="button"
          onClick={(evt) => {
            evt.stopPropagation()
            onMoveTo(entry)
          }}
          className="mt-1 flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground/40 opacity-0 transition-all hover:bg-muted hover:text-primary focus-visible:opacity-100 group-hover:opacity-100"
          title="Move to folder…"
        >
          {isMoving ? <Loader2 className="size-3.5 animate-spin" /> : <FolderInput className="size-3.5" />}
        </button>
      </div>

      {open && expandable && (
        <div className="flex flex-col">
          {node.children.map((child) => (
            <InboxRow
              key={child.entry.id}
              node={child}
              depth={depth + 1}
              isOpen={isOpen}
              onToggleOpen={onToggleOpen}
              selectedId={selectedId}
              onSelect={onSelect}
              movingId={movingId}
              onMoveTo={onMoveTo}
              prepareGhost={prepareGhost}
            />
          ))}
        </div>
      )}
    </div>
  )
}
