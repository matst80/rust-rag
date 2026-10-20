"use client"

import { useMemo, useState } from "react"
import Link from "next/link"
import { ChevronLeft, ChevronRight } from "lucide-react"
import { cn } from "@/lib/utils"
import { dailyKey, dailyNoteId, monthGrid, type JournalDay } from "@/lib/journal"

interface JournalCalendarProps {
  days: Map<string, JournalDay>
}

export function JournalCalendar({ days }: JournalCalendarProps) {
  const today = new Date()
  const [viewYear, setViewYear] = useState(today.getFullYear())
  const [viewMonth, setViewMonth] = useState(today.getMonth())

  const cells = useMemo(() => monthGrid(viewYear, viewMonth), [viewYear, viewMonth])
  const monthLabel = new Date(viewYear, viewMonth, 1).toLocaleDateString([], {
    month: "long",
    year: "numeric",
  })

  const shiftMonth = (delta: number) => {
    const d = new Date(viewYear, viewMonth + delta, 1)
    setViewYear(d.getFullYear())
    setViewMonth(d.getMonth())
  }

  const goToday = () => {
    setViewYear(today.getFullYear())
    setViewMonth(today.getMonth())
  }

  return (
    <div className="rounded-2xl border border-border bg-card p-4">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="font-mono text-xs font-black uppercase tracking-[2px]">
          {monthLabel}
        </h2>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={goToday}
            className="rounded-md px-2 py-1 font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            Today
          </button>
          <button
            type="button"
            onClick={() => shiftMonth(-1)}
            aria-label="Previous month"
            className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <ChevronLeft className="size-4" />
          </button>
          <button
            type="button"
            onClick={() => shiftMonth(1)}
            aria-label="Next month"
            className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <ChevronRight className="size-4" />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-7 gap-1">
        {["M", "T", "W", "T", "F", "S", "S"].map((d, i) => (
          <div
            key={i}
            className="pb-1 text-center font-mono text-[10px] font-bold uppercase text-muted-foreground/60"
          >
            {d}
          </div>
        ))}
        {cells.map((date) => {
          const key = dailyKey(date)
          const day = days.get(key)
          const isToday = dailyKey(today) === key
          const inMonth = date.getMonth() === viewMonth
          const hasNote = !!day?.entryId
          const open = day?.openTasks ?? 0
          const href = hasNote
            ? `/entries/${encodeURIComponent(day!.entryId!)}`
            : `/today?date=${key}`
          return (
            <Link
              key={key + (inMonth ? "-in" : "-out")}
              href={href}
              prefetch={false}
              title={
                hasNote
                  ? `${key} — open note${open > 0 ? ` (${open} open tasks)` : ""}`
                  : `${key} — start a note`
              }
              className={cn(
                "group flex min-h-11 flex-col items-center justify-center gap-0.5 rounded-lg border px-1 py-1.5 text-xs transition-colors",
                isToday
                  ? "border-primary/60 bg-primary/10 font-bold text-primary"
                  : hasNote
                    ? "border-border bg-muted/30 hover:border-primary/40 hover:bg-primary/5"
                    : "border-transparent hover:border-border hover:bg-muted/40",
                !inMonth && "opacity-40"
              )}
            >
              <span className="tabular-nums leading-none">{date.getDate()}</span>
              {hasNote ? (
                <span className="flex items-center gap-1">
                  <span
                    className={cn(
                      "size-1.5 rounded-full",
                      open > 0 ? "bg-amber-500" : "bg-emerald-500"
                    )}
                  />
                  {open > 0 && (
                    <span className="font-mono text-[9px] tabular-nums text-muted-foreground">
                      {open}
                    </span>
                  )}
                </span>
              ) : (
                <span className="text-[10px] leading-none text-muted-foreground/0 transition-colors group-hover:text-muted-foreground/60">
                  +
                </span>
              )}
              <span className="hidden">{dailyNoteId(date)}</span>
            </Link>
          )
        })}
      </div>

      <div className="mt-3 flex items-center gap-4 font-mono text-[10px] uppercase tracking-wider text-muted-foreground/70">
        <span className="flex items-center gap-1.5">
          <span className="size-1.5 rounded-full bg-emerald-500" /> noted
        </span>
        <span className="flex items-center gap-1.5">
          <span className="size-1.5 rounded-full bg-amber-500" /> open tasks
        </span>
      </div>
    </div>
  )
}
