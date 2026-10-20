"use client"

import { useMemo, useState } from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { toast } from "sonner"
import { useSWRConfig } from "swr"
import { api, type Entry } from "@/lib/api"
import { cn } from "@/lib/utils"
import {
  dateKeyOf,
  entryTitle,
  formatPropValue,
  groupByCandidates,
  propValue,
  type DbProperty,
} from "@/lib/database"

interface DatabaseBoardProps {
  typeName: string
  entries: Entry[]
  properties: DbProperty[]
  groupBy: DbProperty
  onGroupByChange: (prop: DbProperty) => void
}

const GROUP_COLORS = [
  "bg-slate-500/10 text-slate-500 border-slate-500/20",
  "bg-blue-500/10 text-blue-500 border-blue-500/20",
  "bg-amber-500/10 text-amber-500 border-amber-500/20",
  "bg-emerald-500/10 text-emerald-500 border-emerald-500/20",
  "bg-violet-500/10 text-violet-500 border-violet-500/20",
  "bg-rose-500/10 text-rose-500 border-rose-500/20",
]

export function DatabaseBoard({ typeName, entries, properties, groupBy, onGroupByChange }: DatabaseBoardProps) {
  const router = useRouter()
  const { mutate } = useSWRConfig()
  const [draggedId, setDraggedId] = useState<string | null>(null)

  const candidates = groupByCandidates(properties)
  const columns = useMemo(() => {
    const values = groupBy.enumValues ?? []
    return [...values, ""]
  }, [groupBy])

  const byColumn = useMemo(() => {
    const map = new Map<string, Entry[]>()
    for (const c of columns) map.set(c, [])
    for (const e of entries) {
      const v = propValue(e, groupBy.key)
      const key = v === null || v === undefined ? "" : String(v)
      if (!map.has(key)) map.set(key, [])
      map.get(key)!.push(e)
    }
    return map
  }, [entries, columns, groupBy])

  const dateProp = properties.find((p) => p.kind === "date") ?? null

  const moveCard = async (entry: Entry, nextGroup: string) => {
    const current = propValue(entry, groupBy.key)
    const currentStr = current === null || current === undefined ? "" : String(current)
    if (currentStr === nextGroup) return
    // Optimistic refresh comes from the mutate below; keep it simple and direct.
    try {
      const nextData = { ...((entry.data ?? {}) as Record<string, unknown>) }
      if (nextGroup === "" && !groupBy.required) {
        delete nextData[groupBy.key]
      } else {
        nextData[groupBy.key] = nextGroup
      }
      await api.items.update(entry.id, {
        text: entry.text,
        source_id: entry.source_id,
        metadata: entry.metadata,
        path: entry.path ?? undefined,
        type: typeName,
        data: nextData,
      })
      mutate((key) =>
        Array.isArray(key) &&
        (key[0] === "items" || (key[0] === "item" && key[1] === entry.id))
      )
      toast.success(`Moved to ${nextGroup === "" ? "Unset" : nextGroup.replace(/_/g, " ")}`)
    } catch (err) {
      toast.error(`Move failed: ${(err as Error).message}`)
      mutate((key) => Array.isArray(key) && key[0] === "items")
    }
  }

  return (
    <div className="flex flex-col gap-3">
      {candidates.length > 1 && (
        <div className="flex items-center gap-2">
          <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
            Group by
          </span>
          <select
            value={groupBy.key}
            onChange={(e) => {
              const next = candidates.find((c) => c.key === e.target.value)
              if (next) onGroupByChange(next)
            }}
            className="h-8 rounded-lg border border-border bg-background px-2 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
          >
            {candidates.map((c) => (
              <option key={c.key} value={c.key}>
                {c.title}
              </option>
            ))}
          </select>
        </div>
      )}

      <div className="flex gap-4 overflow-x-auto pb-4">
        {columns.map((col, i) => {
          const cards = byColumn.get(col) ?? []
          const label = col === "" ? "Unset" : col.replace(/_/g, " ")
          return (
            <div
              key={col || "__unset"}
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => {
                const entry = entries.find((e) => e.id === draggedId)
                if (entry) moveCard(entry, col)
                setDraggedId(null)
              }}
              className={cn(
                "flex w-72 shrink-0 flex-col rounded-xl border border-border/60 bg-muted/20",
                draggedId && "border-dashed"
              )}
            >
              <div className="flex items-center gap-2 border-b border-border/50 p-3">
                <h3 className="font-mono text-[10px] font-black uppercase tracking-[2px]">{label}</h3>
                <span
                  className={cn(
                    "rounded border px-1.5 py-0 font-mono text-[10px] tabular-nums",
                    GROUP_COLORS[i % GROUP_COLORS.length]
                  )}
                >
                  {cards.length}
                </span>
              </div>
              <div className="flex max-h-[60vh] flex-1 flex-col gap-2 overflow-y-auto p-2.5">
                {cards.map((card) => {
                  const dateVal = dateProp ? propValue(card, dateProp.key) : null
                  const dateKey = dateKeyOf(dateVal)
                  return (
                    <div
                      key={card.id}
                      draggable
                      onDragStart={() => setDraggedId(card.id)}
                      onDragEnd={() => setDraggedId(null)}
                      className={cn(
                        "group cursor-grab rounded-xl border border-border/70 bg-card p-3 transition-all hover:border-primary/40 hover:shadow-md active:cursor-grabbing",
                        draggedId === card.id && "opacity-50"
                      )}
                    >
                      <button
                        type="button"
                        onClick={() => router.push(`/entries/${encodeURIComponent(card.id)}`)}
                        className="block w-full truncate text-left text-[13px] font-semibold leading-snug hover:text-primary"
                        title={card.id}
                      >
                        {entryTitle(card)}
                      </button>
                      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[10px] text-muted-foreground">
                        {dateKey && <span className="tabular-nums">📅 {dateKey}</span>}
                        {properties
                          .filter((p) => p.key !== groupBy.key && p.key !== dateProp?.key && p.key !== "title" && p.key !== "name")
                          .slice(0, 2)
                          .map((p) => (
                            <span key={p.key} className="truncate">
                              {p.title}: {formatPropValue(propValue(card, p.key))}
                            </span>
                          ))}
                      </div>
                      <Link
                        href={`/entries/${encodeURIComponent(card.id)}`}
                        prefetch={false}
                        className="mt-1 hidden font-mono text-[10px] uppercase tracking-wider text-primary group-hover:block"
                      >
                        Open →
                      </Link>
                    </div>
                  )
                })}
                {cards.length === 0 && (
                  <div className="flex items-center justify-center rounded-lg border border-dashed border-border/40 p-6 opacity-50">
                    <span className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                      Drop here
                    </span>
                  </div>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
