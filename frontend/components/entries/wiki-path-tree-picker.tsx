"use client"

import { useState } from "react"
import { Check, Database, FileText, FolderTree, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { useEntriesTree } from "@/lib/api"
import { cn, entryTitle } from "@/lib/utils"
import { WikiTree, ancestorChain } from "./wiki-tree"

function normalizePath(input: string): string | null {
  const trimmed = input.trim().replace(/^\/+|\/+$/g, "")
  if (!trimmed) return null
  const segments = trimmed.split("/").map((s) => s.trim()).filter(Boolean)
  return segments.length === 0 ? null : segments.join("/")
}

interface WikiPathTreePickerProps {
  sourceId: string
  value: string
  onChange: (path: string) => void
}

export function WikiPathTreePicker({ sourceId, value, onChange }: WikiPathTreePickerProps) {
  const [open, setOpen] = useState(false)
  const [expanded, setExpanded] = useState<Set<string>>(() => ancestorChain(value))
  const { data: rootData } = useEntriesTree(open ? sourceId : null, undefined)
  // Preview: real entries that live at the currently selected/typed path,
  // so picking a folder isn't a guess.
  const { data: previewData, isLoading: previewLoading } = useEntriesTree(
    open ? sourceId : null,
    value || undefined
  )

  const toggle = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }

  const handleSelect = (path: string) => {
    onChange(path)
    setExpanded((prev) => new Set(prev).add(path))
  }

  const previewEntries = previewData?.entries ?? []

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          className={cn(
            "w-full justify-start gap-2 font-mono text-sm h-9",
            value ? "text-primary border-primary/40" : "text-muted-foreground"
          )}
        >
          <FolderTree className="size-4 shrink-0" />
          <span className="truncate">{value || "No wiki path"}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[28rem] p-0 overflow-hidden rounded-xl border-border shadow-xl">
        <div className="flex flex-col">
          <div className="border-b border-border px-3 py-2">
            <Input
              value={value}
              onChange={(e) => onChange(e.target.value)}
              placeholder="team/handbook"
              className="h-9 font-mono text-sm"
            />
            <p className="mt-1 text-xs text-muted-foreground">
              Slash-separated. Pick a folder below, or type a new/nested path.
            </p>
          </div>

          <div className="max-h-64 overflow-y-auto p-1.5 border-b border-border">
            <button
              type="button"
              onClick={() => handleSelect("")}
              className={cn(
                "flex w-full items-center gap-2 rounded-md px-2 py-1.5 font-mono text-sm transition-colors",
                !value ? "bg-primary/10 text-primary font-semibold" : "hover:bg-muted/50"
              )}
            >
              <Database className="size-4 text-muted-foreground shrink-0" />
              <span className="truncate">{sourceId} (root)</span>
              {!value && <Check className="size-4 text-primary ml-auto shrink-0" />}
            </button>

            {rootData && rootData.children.length === 0 && (
              <p className="px-2 py-4 text-center text-sm text-muted-foreground">
                No existing wiki folders in <span className="font-mono">{sourceId}</span> yet.
              </p>
            )}

            {rootData && (
              <WikiTree
                sourceId={sourceId}
                prefix=""
                depth={0}
                value={value || null}
                expanded={expanded}
                onToggle={toggle}
                onSelect={handleSelect}
              >
                {rootData.children}
              </WikiTree>
            )}
          </div>

          <div className="px-3 py-2 border-b border-border">
            <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground mb-1.5">
              {value ? `Entries in "${value}"` : `Entries at root`}
              {previewData ? ` (${previewEntries.length})` : ""}
            </p>
            <div className="max-h-32 overflow-y-auto flex flex-col gap-1">
              {previewLoading && (
                <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Loader2 className="size-3 animate-spin" /> loading…
                </div>
              )}
              {!previewLoading && previewEntries.length === 0 && (
                <p className="text-xs text-muted-foreground">Nothing here yet.</p>
              )}
              {previewEntries.slice(0, 8).map((entry) => (
                <div key={entry.id} className="flex items-center gap-1.5 text-sm min-w-0">
                  <FileText className="size-3.5 text-muted-foreground shrink-0" />
                  <span className="truncate">{entryTitle(entry)}</span>
                </div>
              ))}
              {previewEntries.length > 8 && (
                <p className="text-xs text-muted-foreground">
                  +{previewEntries.length - 8} more
                </p>
              )}
            </div>
          </div>

          <div className="flex justify-end px-3 py-2">
            <Button type="button" size="sm" className="h-8 text-sm" onClick={() => setOpen(false)}>
              Done
            </Button>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}

export { normalizePath as normalizeWikiPath }
