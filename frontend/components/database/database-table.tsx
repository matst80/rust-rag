"use client"

import { useEntryNavigation } from "@/hooks/use-entry-navigation"
import { useMemo, useState } from "react"
import Link from "next/link"
import { ArrowUpDown, Search } from "lucide-react"
import { toast } from "sonner"
import { useSWRConfig } from "swr"
import { api, type Entry } from "@/lib/api"
import { cn } from "@/lib/utils"
import { entryTitle,
  formatPropValue,
  propValue,
  type DbProperty,
} from "@/lib/database"
import { entryTitleUnclipped, normalizeTitle } from "@/lib/utils"
import { DuplicateTitleBadge } from "@/components/entries/duplicate-title-badge"
import { useDuplicateTitles } from "@/lib/api/hooks"

interface DatabaseTableProps {
  typeName: string
  entries: Entry[]
  properties: DbProperty[]
  visibleProps: DbProperty[]
}

type SortKey = { key: string; dir: 1 | -1 } | null

export function DatabaseTable({ typeName, entries, properties, visibleProps }: DatabaseTableProps) {
  const { mutate } = useSWRConfig()
  const [query, setQuery] = useState("")
  const { data: dupData } = useDuplicateTitles()
  const dupGroupFor = (title: string) => {
    const key = normalizeTitle(title)
    return dupData?.groups.find((g) => normalizeTitle(g.title) === key) ?? null
  }
  const [sort, setSort] = useState<SortKey>(null)
  const [savingId, setSavingId] = useState<string | null>(null)

  const titleKey = properties.find((p) => p.key === "title")?.key ?? properties.find((p) => p.key === "name")?.key

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    let rows = entries
    if (q) {
      rows = rows.filter((e) => {
        if (e.id.toLowerCase().includes(q)) return true
        if (entryTitle(e).toLowerCase().includes(q)) return true
        if (e.text.toLowerCase().includes(q)) return true
        return visibleProps.some((p) => String(propValue(e, p.key) ?? "").toLowerCase().includes(q))
      })
    }
    if (sort) {
      rows = [...rows].sort((a, b) => {
        let av: string
        let bv: string
        if (sort.key === "__title") {
          av = entryTitle(a)
          bv = entryTitle(b)
        } else if (sort.key === "__updated") {
          return (a.updated_at - b.updated_at) * sort.dir
        } else {
          av = formatPropValue(propValue(a, sort.key))
          bv = formatPropValue(propValue(b, sort.key))
        }
        return av.localeCompare(bv) * sort.dir
      })
    }
    return rows
  }, [entries, query, sort, visibleProps])

  const navigation = useEntryNavigation(filtered)

  const toggleSort = (key: string) => {
    setSort((prev) => {
      if (prev?.key !== key) return { key, dir: 1 }
      if (prev.dir === 1) return { key, dir: -1 }
      return null
    })
  }

  const updateProp = async (entry: Entry, prop: DbProperty, rawValue: unknown) => {
    const current = propValue(entry, prop.key)
    if (rawValue === current) return
    if (typeof rawValue === "string" && typeof current !== "boolean") {
      const currentStr = current === null || current === undefined ? "" : String(current)
      if (rawValue === currentStr) return
    }
    setSavingId(entry.id)
    try {
      const nextData = { ...((entry.data ?? {}) as Record<string, unknown>) }
      if ((rawValue === "" || rawValue === null) && !prop.required) {
        delete nextData[prop.key]
      } else {
        nextData[prop.key] = rawValue
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
      toast.success("Updated")
    } catch (err) {
      toast.error(`Update failed: ${(err as Error).message}`)
    } finally {
      setSavingId(null)
    }
  }

  const headerBtn = (label: string, key: string) => (
    <button
      type="button"
      onClick={() => toggleSort(key)}
      className="flex items-center gap-1 font-mono text-[10px] font-black uppercase tracking-widest text-muted-foreground transition-colors hover:text-foreground"
    >
      {label}
      <ArrowUpDown className={cn("size-3", sort?.key === key ? "text-primary" : "opacity-40")} />
    </button>
  )

  return (
    <div {...navigation} className="overflow-hidden rounded-2xl border border-border bg-card">
      <div className="flex items-center gap-2 border-b border-border p-3">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Filter ${entries.length} rows…`}
            className="h-9 w-full rounded-xl border border-border bg-background pl-9 pr-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
        <span className="shrink-0 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
          {filtered.length} / {entries.length}
        </span>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full min-w-180 text-sm">
          <thead>
            <tr className="border-b border-border bg-muted/30 text-left">
              <th className="min-w-52 p-3">{headerBtn("Title", "__title")}</th>
              {visibleProps
                .filter((p) => p.key !== titleKey)
                .map((p) => (
                  <th key={p.key} className="min-w-32 p-3">
                    {headerBtn(p.title, p.key)}
                  </th>
                ))}
              <th className="min-w-28 p-3">{headerBtn("Updated", "__updated")}</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((entry) => (
              <tr key={entry.id} data-entry-row tabIndex={-1} className="group border-b border-border/50 transition-colors last:border-0 hover:bg-primary/[0.03] focus:bg-primary/10 focus:outline-none">
                <td className="max-w-72 p-3">
                  {(() => {
                    const dup = dupGroupFor(entryTitleUnclipped(entry))
                    return (
                      <>
                        <span className="flex items-center gap-1.5">
                          <Link
                            href={`/entries/${encodeURIComponent(entry.id)}`}
                            prefetch={false}
                            className="block truncate font-medium hover:text-primary hover:underline"
                            title={entry.id}
                          >
                            {entryTitle(entry)}
                          </Link>
                          {dup && <DuplicateTitleBadge group={dup} />}
                        </span>
                        <span className="block truncate font-mono text-[10px] text-muted-foreground/60">
                          {dup && entry.path ? `${entry.path} · ${entry.id}` : entry.id}
                        </span>
                      </>
                    )
                  })()}
                </td>
                {visibleProps
                  .filter((p) => p.key !== titleKey)
                  .map((p) => (
                    <td key={p.key} className="p-3">
                      <CellEditor
                        entry={entry}
                        prop={p}
                        disabled={savingId === entry.id}
                        onCommit={(v) => updateProp(entry, p, v)}
                      />
                    </td>
                  ))}
                <td className="whitespace-nowrap p-3 font-mono text-[11px] tabular-nums text-muted-foreground">
                  {new Date(entry.updated_at).toLocaleDateString()}
                </td>
              </tr>
            ))}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={visibleProps.length + 2} className="p-10 text-center text-sm text-muted-foreground">
                  {query ? `No rows match "${query}".` : "No rows yet — add one with New."}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function CellEditor({
  entry,
  prop,
  disabled,
  onCommit,
}: {
  entry: Entry
  prop: DbProperty
  disabled: boolean
  onCommit: (value: unknown) => void
}) {
  const value = propValue(entry, prop.key)

  if (prop.kind === "enum" && prop.enumValues) {
    return (
      <select
        value={value === null || value === undefined ? "" : String(value)}
        disabled={disabled}
        onChange={(e) => onCommit(e.target.value)}
        className="h-8 max-w-44 rounded-lg border border-transparent bg-transparent px-1.5 text-[13px] transition-colors hover:border-border focus:border-primary focus:outline-none disabled:opacity-50"
        title={prop.title}
      >
        {!prop.required && <option value="">—</option>}
        {prop.enumValues.map((v) => (
          <option key={v} value={v}>
            {v.replace(/_/g, " ")}
          </option>
        ))}
      </select>
    )
  }

  if (prop.kind === "date") {
    const str = typeof value === "string" ? value.slice(0, 10) : ""
    return (
      <input
        type="date"
        defaultValue={str}
        disabled={disabled}
        onBlur={(e) => {
          if (e.target.value !== str) onCommit(e.target.value)
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur()
        }}
        className="h-8 rounded-lg border border-transparent bg-transparent px-1.5 font-mono text-xs transition-colors hover:border-border focus:border-primary focus:outline-none disabled:opacity-50"
        title={prop.title}
      />
    )
  }

  if (prop.kind === "boolean") {
    return (
      <button
        type="button"
        disabled={disabled}
        onClick={() => onCommit(!(value === true))}
        className={cn(
          "rounded-md px-2 py-0.5 font-mono text-xs transition-colors disabled:opacity-50",
          value === true ? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400" : "bg-muted text-muted-foreground hover:text-foreground"
        )}
      >
        {value === true ? "Yes" : "No"}
      </button>
    )
  }

  return (
    <span className="block max-w-56 truncate text-[13px] text-muted-foreground" title={String(value ?? "")}>
      {formatPropValue(value)}
    </span>
  )
}
