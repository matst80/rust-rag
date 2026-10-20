"use client"

import { useMemo, useState } from "react"
import Link from "next/link"
import { ChevronLeft, ChevronRight } from "lucide-react"
import { cn } from "@/lib/utils"
import type { Entry } from "@/lib/api"
import {
  dateCandidates,
  dateKeyOf,
  entryTitle,
  propValue,
  type DbProperty,
} from "@/lib/database"

interface DatabaseCalendarProps {
  entries: Entry[]
  properties: DbProperty[]
  dateProp: DbProperty | null
  onDatePropChange: (prop: DbProperty | null) => void
}

function pad(n: number): string {
  return String(n).padStart(2, "0")
}

function dayKey(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

export function DatabaseCalendar({ entries, properties, dateProp, onDatePropChange }: DatabaseCalendarProps) {
  const today = new Date()
  const [year, setYear] = useState(today.getFullYear())
  const [month, setMonth] = useState(today.getMonth())

  const candidates = dateCandidates(properties)

  const { cells, byDay, unscheduled } = useMemo(() => {
    const first = new Date(year, month, 1)
    const startOffset = (first.getDay() + 6) % 7
    const start = new Date(year, month, 1 - startOffset)
    const cells = Array.from({ length: 42 }, (_, i) => {
      const d = new Date(start)
      d.setDate(start.getDate() + i)
      return d
    })
    const byDay = new Map<string, Entry[]>()
    const unscheduled: Entry[] = []
    for (const e of entries) {
      const key = dateProp ? dateKeyOf(propValue(e, dateProp.key)) : null
      if (!key) {
        unscheduled.push(e)
        continue
      }
      if (!byDay.has(key)) byDay.set(key, [])
      byDay.get(key)!.push(e)
    }
    return { cells, byDay, unscheduled }
  }, [entries, dateProp, year, month])

  const monthLabel = new Date(year, month, 1).toLocaleDateString([], { month: "long", year: "numeric" })

  const shift = (delta: number) => {
    const d = new Date(year, month + delta, 1)
    setYear(d.getFullYear())
    setMonth(d.getMonth())
  }

  if (!dateProp) {
    return (
      <div className="rounded-2xl border border-dashed border-border p-8 text-center">
        <p className="text-sm text-muted-foreground">This type has no date property.</p>
        <p className="mt-1 text-xs text-muted-foreground/70">
          Add a <code className="rounded bg-muted px-1 font-mono">format: date</code> field
          (e.g. <code className="rounded bg-muted px-1 font-mono">due</code>) to the schema to
          unlock the calendar view — table and board work without it.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          {candidates.length > 1 && (
            <>
              <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                Date field
              </span>
              <select
                value={dateProp.key}
                onChange={(e) => {
                  const next = candidates.find((c) => c.key === e.target.value) ?? null
                  onDatePropChange(next)
                }}
                className="h-8 rounded-lg border border-border bg-background px-2 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
              >
                {candidates.map((c) => (
                  <option key={c.key} value={c.key}>
                    {c.title}
                  </option>
                ))}
              </select>
            </>
          )}
          <h2 className="font-mono text-xs font-black uppercase tracking-[2px]">{monthLabel}</h2>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => {
              setYear(today.getFullYear())
              setMonth(today.getMonth())
            }}
            className="rounded-md px-2 py-1 font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            Today
          </button>
          <button type="button" onClick={() => shift(-1)} aria-label="Previous month" className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground">
            <ChevronLeft className="size-4" />
          </button>
          <button type="button" onClick={() => shift(1)} aria-label="Next month" className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground">
            <ChevronRight className="size-4" />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-7 gap-1">
        {["M", "T", "W", "T", "F", "S", "S"].map((d, i) => (
          <div key={i} className="pb-1 text-center font-mono text-[10px] font-bold uppercase text-muted-foreground/60">
            {d}
          </div>
        ))}
        {cells.map((date) => {
          const key = dayKey(date)
          const dayEntries = byDay.get(key) ?? []
          const isToday = dayKey(today) === key
          const inMonth = date.getMonth() === month
          return (
            <div
              key={key}
              className={cn(
                "flex min-h-20 flex-col gap-1 rounded-lg border p-1.5",
                isToday ? "border-primary/60 bg-primary/[0.06]" : "border-border/60 bg-card",
                !inMonth && "opacity-50"
              )}
            >
              <span className={cn("font-mono text-[11px] tabular-nums", isToday ? "font-black text-primary" : "text-muted-foreground")}>
                {date.getDate()}
              </span>
              <div className="flex flex-col gap-1 overflow-hidden">
                {dayEntries.slice(0, 3).map((e) => (
                  <Link
                    key={e.id}
                    href={`/entries/${encodeURIComponent(e.id)}`}
                    prefetch={false}
                    title={e.id}
                    className="truncate rounded-md bg-primary/10 px-1.5 py-0.5 text-[11px] font-medium leading-snug hover:bg-primary/20 hover:text-primary"
                  >
                    {entryTitle(e)}
                  </Link>
                ))}
                {dayEntries.length > 3 && (
                  <span className="px-1 font-mono text-[10px] text-muted-foreground">
                    +{dayEntries.length - 3} more
                  </span>
                )}
              </div>
            </div>
          )
        })}
      </div>

      {unscheduled.length > 0 && (
        <div className="rounded-2xl border border-border bg-card p-3">
          <p className="mb-2 font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
            Unscheduled · {unscheduled.length}
          </p>
          <div className="flex flex-wrap gap-1.5">
            {unscheduled.slice(0, 24).map((e) => (
              <Link
                key={e.id}
                href={`/entries/${encodeURIComponent(e.id)}`}
                prefetch={false}
                title={`Set ${dateProp.title} to schedule`}
                className="max-w-56 truncate rounded-lg border border-border px-2.5 py-1.5 text-xs transition-colors hover:border-primary/40 hover:text-primary"
              >
                {entryTitle(e)}
              </Link>
            ))}
            {unscheduled.length > 24 && (
              <span className="px-2 py-1.5 font-mono text-[11px] text-muted-foreground">
                +{unscheduled.length - 24} more in table view
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
