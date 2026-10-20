"use client"

import { useEffect, useMemo, useState } from "react"
import Link from "next/link"
import { CalendarDays, Columns3, Plus, Table2 } from "lucide-react"
import { useSWRConfig } from "swr"
import { useItems, useSchema } from "@/lib/api/hooks"
import { useSessionState } from "@/hooks/use-session-state"
import { EntryForm } from "@/components/entries/entry-form"
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { cn } from "@/lib/utils"
import {
  datePropStorageKey,
  defaultDateProp,
  defaultGroupBy,
  groupByStorageKey,
  schemaProperties,
  viewStorageKey,
  type DatabaseViewKind,
  type DbProperty,
} from "@/lib/database"
import { DatabaseTable } from "./database-table"
import { DatabaseBoard } from "./database-board"
import { DatabaseCalendar } from "./database-calendar"

interface DatabaseViewProps {
  typeName: string
}

const VIEW_TABS: { id: DatabaseViewKind; label: string; icon: typeof Table2 }[] = [
  { id: "table", label: "Table", icon: Table2 },
  { id: "board", label: "Board", icon: Columns3 },
  { id: "calendar", label: "Calendar", icon: CalendarDays },
]

export function DatabaseView({ typeName }: DatabaseViewProps) {
  const { data: schema, isLoading: schemaLoading, error: schemaError } = useSchema(typeName)
  const { data, isLoading: itemsLoading, mutate } = useItems({ type: typeName, limit: 1000, sort_order: "desc" })
  const { mutate: mutateGlobal } = useSWRConfig()

  const [view, setView] = useSessionState<DatabaseViewKind>(viewStorageKey(typeName), "table")
  const [groupByKey, setGroupByKey] = useSessionState<string | null>(groupByStorageKey(typeName), null)
  const [datePropKey, setDatePropKey] = useSessionState<string | null>(datePropStorageKey(typeName), null)
  const [showNew, setShowNew] = useState(false)
  const [visibleCount, setVisibleCount] = useState(8)

  const properties = useMemo(() => (schema ? schemaProperties(schema) : []), [schema])

  const groupBy: DbProperty | null = useMemo(() => {
    if (groupByKey) {
      const hit = properties.find((p) => p.key === groupByKey && p.kind === "enum")
      if (hit) return hit
    }
    return defaultGroupBy(properties)
  }, [properties, groupByKey])

  const dateProp: DbProperty | null = useMemo(() => {
    if (datePropKey) {
      const hit = properties.find((p) => p.key === datePropKey && p.kind === "date")
      if (hit) return hit
    }
    return defaultDateProp(properties)
  }, [properties, datePropKey])

  useEffect(() => {
    setVisibleCount(8)
  }, [typeName])

  const entries = data?.items ?? []
  const visibleProps = useMemo(() => properties.slice(0, visibleCount), [properties, visibleCount])

  // Board needs an enum to group by; fall back to table with a hint.
  useEffect(() => {
    if (view === "board" && properties.length > 0 && !groupBy) {
      setView("table")
    }
  }, [view, properties, groupBy, setView])

  if (schemaLoading || itemsLoading) {
    return (
      <div className="flex flex-col gap-3">
        <div className="h-10 animate-pulse rounded-xl bg-muted/20" />
        <div className="h-64 animate-pulse rounded-2xl bg-muted/20" />
      </div>
    )
  }

  if (schemaError || !schema) {
    return (
      <div className="rounded-2xl border border-dashed border-border p-10 text-center">
        <p className="text-sm text-muted-foreground">
          Schema <code className="rounded bg-muted px-1 font-mono">{typeName}</code> not found.
        </p>
        <p className="mt-2 text-xs text-muted-foreground/70">
          <Link href="/databases" className="text-primary hover:underline">All databases</Link>
          {" · "}
          <Link href="/schemas" className="text-primary hover:underline">Manage schemas</Link>
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-1 rounded-xl border border-border bg-card p-1">
          {VIEW_TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              onClick={() => setView(tab.id)}
              className={cn(
                "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold transition-colors",
                view === tab.id ? "bg-primary/10 text-primary" : "text-muted-foreground hover:text-foreground"
              )}
            >
              <tab.icon className="size-3.5" />
              {tab.label}
            </button>
          ))}
        </div>
        <div className="flex items-center gap-2">
          <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
            {entries.length} rows
          </span>
          <button
            type="button"
            onClick={() => setShowNew(true)}
            className="flex items-center gap-1.5 rounded-xl bg-primary px-3.5 py-2 font-mono text-[11px] font-bold uppercase tracking-widest text-primary-foreground transition-transform hover:scale-105 active:scale-95"
          >
            <Plus className="size-3.5" />
            New
          </button>
        </div>
      </div>

      {view === "table" && (
        <>
          <DatabaseTable typeName={typeName} entries={entries} properties={properties} visibleProps={visibleProps} />
          {properties.length > visibleCount && (
            <button
              type="button"
              onClick={() => setVisibleCount((c) => c + 8)}
              className="self-center font-mono text-[11px] font-bold uppercase tracking-widest text-muted-foreground transition-colors hover:text-primary"
            >
              Show more columns ({properties.length - visibleCount} hidden)
            </button>
          )}
        </>
      )}

      {view === "board" && groupBy && (
        <DatabaseBoard
          typeName={typeName}
          entries={entries}
          properties={properties}
          groupBy={groupBy}
          onGroupByChange={(p) => setGroupByKey(p.key)}
        />
      )}

      {view === "calendar" && (
        <DatabaseCalendar
          entries={entries}
          properties={properties}
          dateProp={dateProp}
          onDatePropChange={(p) => setDatePropKey(p?.key ?? null)}
        />
      )}

      <Dialog open={showNew} onOpenChange={setShowNew}>
        <DialogContent className="max-h-[90vh] flex max-w-2xl flex-col p-0">
          <DialogHeader className="p-6 pb-2">
            <DialogTitle className="font-mono text-sm uppercase tracking-widest">
              New {typeName} row
            </DialogTitle>
          </DialogHeader>
          <div className="no-scrollbar flex-1 overflow-y-auto px-6 pb-6">
            <EntryForm
              minimal
              initialData={{ type_name: typeName, source_id: "knowledge" } as never}
              onSuccess={() => {
                mutate()
                mutateGlobal((key) => Array.isArray(key) && key[0] === "items")
                setShowNew(false)
              }}
              onCancel={() => setShowNew(false)}
            />
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
