"use client"

import Link from "next/link"
import { AlertTriangle, ListChecks } from "lucide-react"
import { useMemo, useState } from "react"
import { DuplicateCleanupDialog } from "./duplicate-cleanup-dialog"
import { useDuplicateTitles, type DuplicateTitleGroup } from "@/lib/api"
import { normalizeTitle } from "@/lib/utils"

export function useDuplicateGroup(title: string | null | undefined): DuplicateTitleGroup | null {
  const { data } = useDuplicateTitles()
  return useMemo(() => {
    if (!title) return null
    const key = normalizeTitle(title)
    return data?.groups.find((g) => normalizeTitle(g.title) === key) ?? null
  }, [data, title])
}

/** Short disambiguator: wiki path, else `source · short-id`. */
export function disambiguator(opts: {
  path?: string | null
  source_id?: string
  id: string
}): string {
  if (opts.path) return opts.path
  const short = opts.id.length > 12 ? `${opts.id.slice(0, 8)}…` : opts.id
  return opts.source_id ? `${opts.source_id} · ${short}` : short
}

/** Inline `×N` badge for duplicated titles in lists and cells. */
export function DuplicateTitleBadge({ group }: { group: DuplicateTitleGroup }) {
  return (
    <span
      title={`${group.count} entries share this title${group.all_extracted ? " (all derived from body headings — set an explicit title to fix)" : ""}`}
      className="inline-flex shrink-0 items-center gap-1 rounded-full border border-amber-500/30 bg-amber-500/10 px-1.5 py-px font-mono text-[9px] font-black uppercase tracking-wider text-amber-600 dark:text-amber-400"
    >
      <AlertTriangle className="size-2.5" />
      ×{group.count}
    </span>
  )
}

/** Banner for the entry detail page when its title is shared. */
export function DuplicateTitleBanner({
  title,
  selfId,
}: {
  title: string
  selfId: string
}) {
  const group = useDuplicateGroup(title)
  const [cleanupOpen, setCleanupOpen] = useState(false)
  if (!group || group.count < 2) return null
  const siblings = group.items.filter((i) => i.id !== selfId).slice(0, 6)
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-amber-500/30 bg-amber-500/[0.07] px-4 py-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="flex items-center gap-2 text-[13px] font-medium text-amber-700 dark:text-amber-300">
          <AlertTriangle className="size-3.5 shrink-0" />
          {group.count} entries share the title “{group.title}”
          {group.all_extracted && " — all derived from body headings"}
        </p>
        <button
          type="button"
          onClick={() => setCleanupOpen(true)}
          className="inline-flex items-center gap-1.5 rounded-lg border border-amber-500/30 bg-background px-2 py-1 text-xs font-medium text-amber-700 transition-colors hover:border-amber-500/60 hover:bg-amber-500/10 dark:text-amber-300"
        >
          <ListChecks className="size-3.5" />
          Compare & clean up
        </button>
      </div>
      <DuplicateCleanupDialog
        group={group}
        selfId={selfId}
        open={cleanupOpen}
        onOpenChange={setCleanupOpen}
      />
      {siblings.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {siblings.map((s) => (
            <Link
              key={s.id}
              href={`/entries/${encodeURIComponent(s.id)}`}
              prefetch={false}
              title={s.id}
              className="max-w-64 truncate rounded-lg border border-amber-500/25 bg-background px-2 py-1 font-mono text-[11px] transition-colors hover:border-amber-500/60 hover:text-amber-700 dark:hover:text-amber-300"
            >
              {disambiguator(s)}
            </Link>
          ))}
        </div>
      )}
      <p className="text-xs text-muted-foreground">
        Tip: set an explicit <code className="rounded bg-muted px-1 font-mono">title</code> in
        Details → Metadata to give this entry its own name.
      </p>
    </div>
  )
}
