"use client"

import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import { Folder, FileText, ChevronRight, FolderTree } from "lucide-react"
import { useCategories, useEntriesTree } from "@/lib/api"

function buildHref(sourceId: string, path?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  return `/wiki?${params.toString()}`
}

interface EntryTreeProps {
  sourceId: string
  prefix?: string
}

export function EntryTree({ sourceId, prefix }: EntryTreeProps) {
  const router = useRouter()
  const { data: categories } = useCategories()
  const { data: tree, isLoading } = useEntriesTree(sourceId, prefix)

  const segments = prefix ? prefix.split("/") : []

  return (
    <div className="flex h-[calc(100vh-3rem)] flex-col">
      <div className="flex h-12 shrink-0 items-center gap-3 border-b border-border px-4">
        <FolderTree className="size-4 text-primary" />
        <h1 className="font-mono text-xs font-bold uppercase tracking-[2px]">Wiki</h1>
        <select
          value={sourceId}
          onChange={(e) =>
            router.push(buildHref(e.target.value))
          }
          className="ml-auto font-mono text-xs bg-background border border-border px-2 py-1"
        >
          {categories?.map((c) => (
            <option key={c.id} value={c.id}>
              {c.id} ({c.count})
            </option>
          ))}
          {!categories?.find((c) => c.id === sourceId) && (
            <option value={sourceId}>{sourceId}</option>
          )}
        </select>
      </div>

      <nav className="flex items-center gap-1 px-4 py-3 border-b border-border font-mono text-xs">
        <Link
          href={buildHref(sourceId)}
          className="text-muted-foreground hover:text-primary"
        >
          {sourceId}
        </Link>
        {segments.map((seg, i) => {
          const sub = segments.slice(0, i + 1).join("/")
          const isLast = i === segments.length - 1
          return (
            <span key={sub} className="flex items-center gap-1">
              <ChevronRight className="size-3 text-muted-foreground" />
              {isLast ? (
                <span className="text-foreground">{seg}</span>
              ) : (
                <Link
                  href={buildHref(sourceId, sub)}
                  className="text-muted-foreground hover:text-primary"
                >
                  {seg}
                </Link>
              )}
            </span>
          )
        })}
      </nav>

      <div className="flex-1 overflow-y-auto p-4">
        {isLoading && (
          <p className="font-mono text-xs text-muted-foreground">Loading…</p>
        )}

        {tree && tree.children.length === 0 && tree.entries.length === 0 && (
          <p className="font-mono text-xs text-muted-foreground">
            No entries under this path.
          </p>
        )}

        {tree && tree.children.length > 0 && (
          <div className="mb-6">
            <h2 className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground mb-3">
              Folders
            </h2>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
              {tree.children.map((c) => (
                <Link
                  key={c.path}
                  href={buildHref(sourceId, c.path)}
                  className="flex items-center gap-3 border border-border bg-card p-3 hover:border-primary/40 transition-colors"
                >
                  <Folder className="size-4 text-primary shrink-0" />
                  <div className="flex flex-col min-w-0 flex-1">
                    <span className="font-mono text-xs font-bold truncate">
                      {c.segment}
                    </span>
                    <span className="font-mono text-[10px] text-muted-foreground">
                      {c.count} entr{c.count === 1 ? "y" : "ies"}
                      {c.has_children ? " · has subfolders" : ""}
                    </span>
                  </div>
                </Link>
              ))}
            </div>
          </div>
        )}

        {tree && tree.entries.length > 0 && (
          <div>
            <h2 className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground mb-3">
              Entries here
            </h2>
            <div className="flex flex-col gap-2">
              {tree.entries.map((e) => (
                <Link
                  key={e.id}
                  href={`/entries/${encodeURIComponent(e.id)}`}
                  className="flex items-center gap-3 border border-border bg-card p-3 hover:border-primary/40 transition-colors"
                >
                  <FileText className="size-4 text-muted-foreground shrink-0" />
                  <div className="flex flex-col min-w-0 flex-1">
                    <span className="font-mono text-xs font-bold truncate">
                      {e.id}
                    </span>
                    <span className="font-mono text-[10px] text-muted-foreground line-clamp-1">
                      {e.text.slice(0, 200)}
                    </span>
                  </div>
                </Link>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
