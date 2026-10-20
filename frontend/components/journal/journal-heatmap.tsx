"use client"

import Link from "next/link"
import { cn } from "@/lib/utils"
import { heatmapWeeks, type JournalDay } from "@/lib/journal"

interface JournalHeatmapProps {
  days: JournalDay[]
  weeks?: number
}

function intensityClass(day: JournalDay): string {
  const activity = day.count + day.doneTasks + Math.min(day.openTasks, 3) * 0.5
  if (activity <= 0) return "bg-muted/40"
  if (activity <= 1) return "bg-emerald-500/25"
  if (activity <= 2.5) return "bg-emerald-500/50"
  if (activity <= 4) return "bg-emerald-500/75"
  return "bg-emerald-500"
}

export function JournalHeatmap({ days, weeks = 16 }: JournalHeatmapProps) {
  const cols = heatmapWeeks(days, weeks)
  const totalNotes = days.filter((d) => d.entryId).length
  const totalDone = days.reduce((n, d) => n + d.doneTasks, 0)
  const streak = currentStreak(days)

  return (
    <div className="rounded-2xl border border-border bg-card p-4">
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="font-mono text-xs font-black uppercase tracking-[2px]">
          Activity
        </h2>
        <p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
          {totalNotes} notes · {totalDone} tasks done · {streak}-day streak
        </p>
      </div>
      <div className="flex gap-1 overflow-x-auto pb-1">
        {cols.map((col, w) => (
          <div key={w} className="flex flex-col gap-1">
            {col.map((day) => {
              const href = day.entryId
                ? `/entries/${encodeURIComponent(day.entryId)}`
                : `/today?date=${day.key}`
              return (
                <Link
                  key={day.key}
                  href={href}
                  prefetch={false}
                  title={`${day.key}${day.entryId ? "" : " — no note yet"}${day.openTasks > 0 ? ` · ${day.openTasks} open` : ""}${day.doneTasks > 0 ? ` · ${day.doneTasks} done` : ""}`}
                  className={cn(
                    "size-3.5 rounded-[4px] transition-transform hover:scale-125 hover:ring-1 hover:ring-primary",
                    intensityClass(day)
                  )}
                />
              )
            })}
          </div>
        ))}
      </div>
      <p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-muted-foreground/60">
        Last {weeks} weeks · click a square to open that day
      </p>
    </div>
  )
}

function currentStreak(days: JournalDay[]): number {
  const byKey = new Map(days.map((d) => [d.key, d]))
  let streak = 0
  const cursor = new Date()
  cursor.setHours(0, 0, 0, 0)
  // If today has no note yet, the streak counts back from yesterday.
  const pad = (n: number) => String(n).padStart(2, "0")
  const keyOf = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
  if (!byKey.get(keyOf(cursor))?.entryId) {
    cursor.setDate(cursor.getDate() - 1)
  }
  while (byKey.get(keyOf(cursor))?.entryId) {
    streak += 1
    cursor.setDate(cursor.getDate() - 1)
  }
  return streak
}
