"use client"

import { useMemo } from "react"
import Link from "next/link"
import { CalendarDays, Loader2, Plus } from "lucide-react"
import { useItems } from "@/lib/api"
import {
  dailyKey,
  dailyNoteId,
  extractTasks,
  parseDailyId,
  type JournalDay,
  type JournalTask,
} from "@/lib/journal"
import { JournalCalendar } from "@/components/journal/journal-calendar"
import { JournalHeatmap } from "@/components/journal/journal-heatmap"
import { JournalTasks } from "@/components/journal/journal-tasks"

export default function JournalPage() {
  const { data, isLoading } = useItems({
    path_prefix: "journal",
    limit: 500,
    sort_order: "desc",
  })

  const { days, openTasks, doneTasks, recent } = useMemo(() => {
    const dayMap = new Map<string, JournalDay>()
    const open: JournalTask[] = []
    const done: JournalTask[] = []
    const items = data?.items ?? []

    for (const item of items) {
      const date = parseDailyId(item.id)
      if (!date) continue
      const key = dailyKey(date)
      const tasks = extractTasks(item.text, item.id, key)
      const openCount = tasks.filter((t) => !t.done).length
      const doneCount = tasks.filter((t) => t.done).length
      dayMap.set(key, {
        date,
        key,
        entryId: item.id,
        count: 1,
        openTasks: openCount,
        doneTasks: doneCount,
      })
      for (const t of tasks) {
        if (t.done) done.push({ ...t, entryTitle: key })
        else open.push({ ...t, entryTitle: key })
      }
    }

    // Newest-first for the recent list.
    const recent = [...dayMap.values()].sort(
      (a, b) => b.date.getTime() - a.date.getTime()
    ).slice(0, 7)

    return { days: dayMap, openTasks: open, doneTasks: done, recent }
  }, [data])

  const allDays = useMemo(() => [...days.values()], [days])
  const todayId = dailyNoteId()

  return (
    <div className="mx-auto flex min-h-[calc(100vh-3rem)] w-full max-w-5xl flex-col gap-5 p-4 md:p-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="flex items-center gap-2 font-mono text-sm font-black uppercase tracking-[3px]">
            <CalendarDays className="size-4 text-primary" />
            Journal
          </h1>
          <p className="mt-1 text-xs text-muted-foreground">
            Daily notes · streaks · tasks across{" "}
            <code className="rounded bg-muted px-1 font-mono">journal/</code>
          </p>
        </div>
        <Link
          href={`/entries/${encodeURIComponent(todayId)}`}
          prefetch={false}
          className="flex items-center gap-1.5 rounded-xl bg-primary px-4 py-2 font-mono text-[11px] font-bold uppercase tracking-widest text-primary-foreground transition-transform hover:scale-105 active:scale-95"
        >
          <Plus className="size-3.5" />
          Today&apos;s note
        </Link>
      </header>

      {isLoading && allDays.length === 0 ? (
        <div className="flex items-center justify-center gap-2 py-20 text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          <span className="font-mono text-xs uppercase tracking-widest">
            Loading journal…
          </span>
        </div>
      ) : (
        <>
          <div className="grid gap-5 lg:grid-cols-2">
            <JournalCalendar days={days} />
            <div className="flex flex-col gap-5">
              <JournalHeatmap days={allDays} weeks={16} />
              {recent.length > 0 && (
                <div className="rounded-2xl border border-border bg-card p-4">
                  <h2 className="mb-2 font-mono text-xs font-black uppercase tracking-[2px]">
                    Recent days
                  </h2>
                  <div className="flex flex-wrap gap-1.5">
                    {recent.map((d) => (
                      <Link
                        key={d.key}
                        href={`/entries/${encodeURIComponent(d.entryId!)}`}
                        prefetch={false}
                        className="rounded-lg border border-border px-2.5 py-1.5 font-mono text-[11px] tabular-nums transition-colors hover:border-primary/40 hover:text-primary"
                      >
                        {d.key}
                        {d.openTasks > 0 && (
                          <span className="ml-1.5 text-amber-500">·{d.openTasks}</span>
                        )}
                      </Link>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
          <JournalTasks openTasks={openTasks} doneTasks={doneTasks} />
        </>
      )}
    </div>
  )
}
