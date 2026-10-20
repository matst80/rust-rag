"use client"

import Link from "next/link"
import { Database, Table2 } from "lucide-react"
import { useSchemas } from "@/lib/api/hooks"

export function DatabaseBrowser() {
  const { data: schemas, isLoading, error } = useSchemas()

  if (isLoading) {
    return (
      <div className="flex flex-col gap-4">
        {[1, 2, 3].map((i) => (
          <div key={i} className="h-20 animate-pulse rounded-2xl bg-muted/20" />
        ))}
      </div>
    )
  }

  if (error) {
    return <p className="text-sm text-destructive">Failed to load databases: {(error as Error).message}</p>
  }

  if (!schemas || schemas.length === 0) {
    return (
      <div className="rounded-2xl border border-dashed border-border p-10 text-center">
        <Database className="mx-auto mb-3 size-6 text-muted-foreground/40" />
        <p className="text-sm text-muted-foreground">
          No schemas registered yet. Schemas seed on first boot from{" "}
          <code className="rounded bg-muted px-1 font-mono text-xs">assets/schemas/*.json</code>{" "}
          — or create one at <Link href="/schemas" className="text-primary hover:underline">/schemas</Link>.
        </p>
      </div>
    )
  }

  const sorted = [...schemas].sort((a, b) => (b.item_count ?? 0) - (a.item_count ?? 0))

  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {sorted.map((s) => (
        <Link
          key={s.type_name}
          href={`/databases/${encodeURIComponent(s.type_name)}`}
          prefetch={false}
          className="group rounded-2xl border border-border bg-card p-4 transition-all hover:border-primary/40 hover:bg-primary/[0.03] hover:shadow-lg"
        >
          <div className="flex items-start justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="flex size-8 items-center justify-center rounded-xl bg-primary/10 text-primary">
                <Table2 className="size-4" />
              </span>
              <div className="min-w-0">
                <p className="truncate font-mono text-sm font-bold">{s.type_name}</p>
                {s.title && <p className="truncate text-xs text-muted-foreground">{s.title}</p>}
              </div>
            </div>
            <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 font-mono text-[11px] tabular-nums text-muted-foreground">
              {s.item_count ?? 0}
            </span>
          </div>
          {s.description && (
            <p className="mt-2 line-clamp-2 text-xs leading-relaxed text-muted-foreground">
              {s.description}
            </p>
          )}
          <p className="mt-3 font-mono text-[10px] font-bold uppercase tracking-widest text-primary opacity-0 transition-opacity group-hover:opacity-100">
            Open database →
          </p>
        </Link>
      ))}
    </div>
  )
}
