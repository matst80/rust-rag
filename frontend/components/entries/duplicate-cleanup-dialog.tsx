"use client"

import { useEffect, useMemo, useState } from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useSWRConfig } from "swr"
import {
  ArrowLeftRight,
  Crown,
  ExternalLink,
  ListChecks,
  Loader2,
  Trash2,
} from "lucide-react"
import { toast } from "sonner"
import { api, type DuplicateTitleGroup, type Entry } from "@/lib/api"
import { disambiguator } from "./duplicate-title-badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { MarkdownView } from "./markdown-view"
import { cn } from "@/lib/utils"

interface LinkInfo {
  backlinks: number
  incomingEdges: number
  total: number
  loading: boolean
}

const emptyInfo = (loading = true): LinkInfo => ({
  backlinks: 0,
  incomingEdges: 0,
  total: 0,
  loading,
})

function formatDate(ms: number) {
  try {
    return new Date(ms).toLocaleDateString([], {
      year: "numeric",
      month: "short",
      day: "numeric",
    })
  } catch {
    return "—"
  }
}

/**
 * Cleanup dialog for one duplicate-title group.
 *
 * - List view: checkbox per entry to mark for deletion, link counts
 *   (wikilink backlinks + incoming manual edges) so the entry with the
 *   most links is the obvious keeper.
 * - Compare view: side-by-side full-text preview of every member.
 * - Delete runs through the existing bulk-delete endpoint.
 */
export function DuplicateCleanupDialog({
  group,
  selfId,
  open,
  onOpenChange,
}: {
  group: DuplicateTitleGroup
  selfId: string
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const router = useRouter()
  const { mutate } = useSWRConfig()
  const [view, setView] = useState<"list" | "compare">("list")
  const [entries, setEntries] = useState<Record<string, Entry | null>>({})
  const [links, setLinks] = useState<Record<string, LinkInfo>>({})
  const [toDelete, setToDelete] = useState<Set<string>>(new Set())
  const [keeperId, setKeeperId] = useState<string | null>(null)
  const [deleting, setDeleting] = useState(false)
  const [confirming, setConfirming] = useState(false)

  // Every id we need to show: group members + self (group may truncate at 10).
  const ids = useMemo(() => {
    const all = group.items.map((i) => i.id)
    if (!all.includes(selfId)) all.push(selfId)
    return all
  }, [group, selfId])

  // Fetch full entries + link counts when the dialog opens.
  useEffect(() => {
    if (!open) return
    setConfirming(false)
    let cancelled = false
    setLinks(Object.fromEntries(ids.map((id) => [id, emptyInfo(true)])))
    setEntries({})

    void (async () => {
      const results = await Promise.all(
        ids.map(async (id) => {
          try {
            const [entry, backlinks, edges] = await Promise.all([
              api.items.get(id).catch(() => null),
              api.backlinks.get(id).catch(() => null),
              api.edges.listForItem(id).catch(() => []),
            ])
            if (cancelled) return null
            const incoming = (edges ?? []).filter(
              (e) => e.edge_type === "manual" && e.source_id !== id,
            )
            const incomingIds = new Set(incoming.map((e) => e.source_id))
            const mentions = (backlinks?.backlinks ?? []).filter(
              (b) => !incomingIds.has(b.id),
            ).length
            return {
              id,
              entry,
              info: {
                backlinks: mentions,
                incomingEdges: incoming.length,
                total: mentions + incoming.length,
                loading: false,
              } satisfies LinkInfo,
            }
          } catch {
            if (cancelled) return null
            return { id, entry: null, info: emptyInfo(false) }
          }
        }),
      )
      if (cancelled) return
      const nextEntries: Record<string, Entry | null> = {}
      const nextLinks: Record<string, LinkInfo> = {}
      for (const r of results) {
        if (!r) continue
        nextEntries[r.id] = r.entry
        nextLinks[r.id] = r.info
      }
      setEntries(nextEntries)
      setLinks(nextLinks)
    })()
    return () => {
      cancelled = true
    }
  }, [open, ids])

  // Suggested keeper = most links, tie-break by longest text then most recent.
  const suggestedId = useMemo(() => {
    let best: string | null = null
    let bestScore = -1
    for (const item of group.items) {
      const info = links[item.id]
      const total = info && !info.loading ? info.total : -1
      const entryLen = entries[item.id]?.text?.length ?? 0
      // Weight links far above length so length only breaks ties.
      const score = total * 1_000_000 + Math.min(entryLen, 999_999)
      if (score > bestScore) {
        bestScore = score
        best = item.id
      }
    }
    // Fall back to the newest member before link counts load.
    if (!best && group.items.length > 0) best = group.items[0].id
    return best
  }, [group.items, links, entries])

  // Pre-select everything except the suggested keeper for deletion once
  // link counts have loaded (user can still adjust).
  useEffect(() => {
    if (!open || !suggestedId) return
    const allLoaded = ids.every((id) => links[id] && !links[id].loading)
    if (!allLoaded) return
    setToDelete((prev) => {
      if (prev.size > 0) return prev
      return new Set(ids.filter((id) => id !== suggestedId))
    })
    setKeeperId((prev) => prev ?? suggestedId)
  }, [open, suggestedId, ids, links])

  const ordered = useMemo(() => {
    const meta = new Map(group.items.map((i) => [i.id, i]))
    return ids
      .map((id) => ({ id, meta: meta.get(id) ?? null }))
      .sort(
        (a, b) =>
          (links[b.id]?.total ?? -1) - (links[a.id]?.total ?? -1) ||
          (b.meta?.updated_at ?? 0) - (a.meta?.updated_at ?? 0),
      )
  }, [ids, group.items, links])

  const toggleDelete = (id: string) =>
    setToDelete((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  const keepOnly = (id: string) => {
    setKeeperId(id)
    setToDelete(new Set(ids.filter((other) => other !== id)))
  }

  const handleDelete = async () => {
    if (toDelete.size === 0) return
    if (!confirming) {
      setConfirming(true)
      return
    }
    setDeleting(true)
    try {
      const res = await api.items.bulkDelete([...toDelete])
      toast.success(
        res.deleted.length === 1
          ? "Deleted 1 duplicate"
          : `Deleted ${res.deleted.length} duplicates`,
      )
      if (res.not_found.length > 0) {
        toast.warning(`${res.not_found.length} already gone`)
      }
      await Promise.all([
        mutate("duplicate-titles"),
        mutate((key) => Array.isArray(key) && key[0] === "items"),
      ])
      onOpenChange(false)
      setToDelete(new Set())
      setConfirming(false)
      // If the entry being viewed was deleted, leave to the list.
      if (toDelete.has(selfId)) router.push("/entries")
      else if (keeperId && keeperId !== selfId) {
        router.push(`/entries/${encodeURIComponent(keeperId)}`)
      }
    } catch {
      toast.error("Delete failed — nothing was removed")
    } finally {
      setDeleting(false)
    }
  }

  const anyLoading = ids.some((id) => !links[id] || links[id].loading)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[90vh] max-w-5xl flex-col p-0">
        <DialogHeader className="shrink-0 px-6 pt-6">
          <DialogTitle className="flex items-center gap-2">
            <ListChecks className="size-4" />
            Clean up “{group.title}”
          </DialogTitle>
          <DialogDescription>
            {group.count} entries share this title. Link count = wikilink
            mentions + incoming manual edges — keep the one others point to,
            delete the rest. The suggested keeper is marked with{" "}
            <Crown className="inline size-3" />.
          </DialogDescription>
        </DialogHeader>

        <div className="flex shrink-0 items-center gap-2 px-6">
          <div className="inline-flex rounded-lg border p-0.5 text-xs font-medium">
            <button
              type="button"
              onClick={() => setView("list")}
              className={cn(
                "flex items-center gap-1.5 rounded-md px-2.5 py-1 transition-colors",
                view === "list"
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              <ListChecks className="size-3.5" /> List with checkboxes
            </button>
            <button
              type="button"
              onClick={() => setView("compare")}
              className={cn(
                "flex items-center gap-1.5 rounded-md px-2.5 py-1 transition-colors",
                view === "compare"
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              <ArrowLeftRight className="size-3.5" /> Side-by-side
            </button>
          </div>
          {anyLoading && (
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Loader2 className="size-3 animate-spin" /> counting links…
            </span>
          )}
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
          {view === "list" ? (
            <ul className="space-y-2">
              {ordered.map(({ id, meta }) => {
                const info = links[id] ?? emptyInfo(true)
                const entry = entries[id]
                const isSuggested = id === suggestedId
                const isSelf = id === selfId
                const checked = toDelete.has(id)
                const snippet = (entry?.text ?? "")
                  .replace(/^#+\s*/gm, "")
                  .split("\n")
                  .map((l) => l.trim())
                  .filter(Boolean)
                  .slice(0, 3)
                  .join(" · ")
                  .slice(0, 220)
                return (
                  <li
                    key={id}
                    className={cn(
                      "flex gap-3 rounded-xl border p-3 transition-colors",
                      checked
                        ? "border-destructive/40 bg-destructive/[0.04]"
                        : "border-border",
                      isSuggested && "border-amber-500/40",
                    )}
                  >
                    <Checkbox
                      checked={checked}
                      onCheckedChange={() => toggleDelete(id)}
                      aria-label={`Mark ${id} for deletion`}
                      className="mt-1"
                    />
                    <div className="min-w-0 flex-1 space-y-1">
                      <div className="flex flex-wrap items-center gap-1.5 text-[13px]">
                        <Link
                          href={`/entries/${encodeURIComponent(id)}`}
                          prefetch={false}
                          target="_blank"
                          className="inline-flex min-w-0 items-center gap-1 font-mono font-medium hover:text-primary"
                        >
                          <span className="truncate">
                            {meta ? disambiguator({ ...meta, id }) : id}
                          </span>
                          <ExternalLink className="size-3 shrink-0 opacity-50" />
                        </Link>
                        {isSelf && (
                          <span className="rounded-full bg-primary/10 px-1.5 py-px font-mono text-[10px] font-bold uppercase tracking-wider text-primary">
                            you are here
                          </span>
                        )}
                        {isSuggested && (
                          <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-1.5 py-px font-mono text-[10px] font-bold uppercase tracking-wider text-amber-600 dark:text-amber-400">
                            <Crown className="size-2.5" /> most links
                          </span>
                        )}
                      </div>
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 font-mono text-[11px] text-muted-foreground">
                        <span
                          title={`${info.backlinks} wikilink mentions + ${info.incomingEdges} incoming manual edges`}
                          className="inline-flex items-center gap-1 font-bold text-foreground"
                        >
                          {info.loading ? (
                            <Loader2 className="size-3 animate-spin" />
                          ) : (
                            <>🔗 {info.total}</>
                          )}
                          {!info.loading && (
                            <span className="font-normal text-muted-foreground">
                              ({info.backlinks} mentions · {info.incomingEdges}{" "}
                              edges)
                            </span>
                          )}
                        </span>
                        {meta && (
                          <span>
                            {meta.source_id} · {formatDate(meta.updated_at)}
                          </span>
                        )}
                        {entry && (
                          <span>{entry.text.length.toLocaleString()} chars</span>
                        )}
                      </div>
                      {snippet && (
                        <p className="line-clamp-2 text-xs leading-relaxed text-muted-foreground">
                          {snippet}
                        </p>
                      )}
                      <div className="flex gap-2 pt-0.5">
                        <button
                          type="button"
                          onClick={() => keepOnly(id)}
                          className="font-mono text-[11px] font-medium text-primary hover:underline"
                        >
                          Keep this one, delete the rest
                        </button>
                      </div>
                    </div>
                  </li>
                )
              })}
            </ul>
          ) : (
            <div className="flex snap-x gap-3 overflow-x-auto pb-2">
              {ordered.map(({ id, meta }) => {
                const info = links[id] ?? emptyInfo(true)
                const entry = entries[id]
                const isSuggested = id === suggestedId
                return (
                  <article
                    key={id}
                    className={cn(
                      "w-80 max-w-[85vw] shrink-0 snap-start space-y-2 rounded-xl border p-3",
                      isSuggested
                        ? "border-amber-500/50"
                        : "border-border",
                      toDelete.has(id) && "opacity-70",
                    )}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <p className="truncate font-mono text-xs font-bold">
                          {meta
                            ? disambiguator({ ...meta, id })
                            : id.slice(0, 12)}
                        </p>
                        <p className="font-mono text-[11px] text-muted-foreground">
                          {info.loading
                            ? "counting…"
                            : `🔗 ${info.total} (${info.backlinks} mentions · ${info.incomingEdges} edges)`}
                        </p>
                      </div>
                      <label className="flex shrink-0 cursor-pointer items-center gap-1.5 font-mono text-[11px] text-muted-foreground">
                        <Checkbox
                          checked={toDelete.has(id)}
                          onCheckedChange={() => toggleDelete(id)}
                          aria-label={`Mark ${id} for deletion`}
                        />
                        delete
                      </label>
                    </div>
                    {isSuggested && (
                      <p className="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-1.5 py-px font-mono text-[10px] font-bold uppercase tracking-wider text-amber-600 dark:text-amber-400">
                        <Crown className="size-2.5" /> most links
                      </p>
                    )}
                    <div className="max-h-96 overflow-y-auto rounded-lg border border-border/60 bg-muted/20 p-2">
                      {entry ? (
                        <MarkdownView
                          content={entry.text.slice(0, 6000)}
                        />
                      ) : (
                        <p className="flex items-center gap-2 p-4 text-xs text-muted-foreground">
                          <Loader2 className="size-3 animate-spin" /> loading…
                        </p>
                      )}
                    </div>
                    <Link
                      href={`/entries/${encodeURIComponent(id)}`}
                      prefetch={false}
                      target="_blank"
                      className="inline-flex items-center gap-1 font-mono text-[11px] text-primary hover:underline"
                    >
                      Open full entry <ExternalLink className="size-3" />
                    </Link>
                  </article>
                )
              })}
            </div>
          )}
        </div>

        <DialogFooter className="shrink-0 items-center border-t px-6 py-4 sm:justify-between">
          <p className="text-xs text-muted-foreground">
            {toDelete.size === 0
              ? "Tick entries to delete. Nothing is removed until you confirm."
              : `${toDelete.size} marked for deletion${keeperId && !toDelete.has(keeperId) ? ` · keeping ${keeperId.slice(0, 8)}…` : ""}`}
          </p>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => onOpenChange(false)}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant={confirming ? "default" : "destructive"}
              size="sm"
              disabled={toDelete.size === 0 || deleting}
              onClick={handleDelete}
            >
              {deleting ? (
                <>
                  <Loader2 className="size-3.5 animate-spin" /> Deleting…
                </>
              ) : confirming ? (
                <>Confirm delete {toDelete.size}?</>
              ) : (
                <>
                  <Trash2 className="size-3.5" /> Delete {toDelete.size || ""}
                </>
              )}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
