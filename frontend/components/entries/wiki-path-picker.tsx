"use client"

import { useMemo, useState } from "react"
import { useSWRConfig } from "swr"
import { Check, FolderPlus, LoaderCircle, Pencil } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { useEntriesPaths, useUpdateItem } from "@/lib/api"
import type { Entry } from "@/lib/api"
import { toast } from "sonner"
import { cn } from "@/lib/utils"

interface WikiPathPickerProps {
  entry: Entry
}

function normalizePath(input: string): string | null {
  const trimmed = input.trim().replace(/^\/+|\/+$/g, "")
  if (!trimmed) return null
  // Collapse repeated slashes, drop empty segments.
  const segments = trimmed.split("/").map((s) => s.trim()).filter(Boolean)
  return segments.length === 0 ? null : segments.join("/")
}

export function WikiPathPicker({ entry }: WikiPathPickerProps) {
  const { mutate } = useSWRConfig()
  const { data: pathsData } = useEntriesPaths(entry.source_id)
  const { trigger: updateItem, isMutating } = useUpdateItem(entry.id)

  const [open, setOpen] = useState(false)
  const [input, setInput] = useState("")

  const suggestions = useMemo(() => {
    const rows = pathsData?.paths ?? []
    return rows
      .filter((r) => r.source_id === entry.source_id)
      .map((r) => r.path)
      .filter(Boolean)
      .sort()
  }, [pathsData, entry.source_id])

  const filtered = useMemo(() => {
    const q = input.trim().toLowerCase()
    if (!q) return suggestions.slice(0, 12)
    return suggestions.filter((p) => p.toLowerCase().includes(q)).slice(0, 12)
  }, [input, suggestions])

  const apply = async (rawPath: string | null) => {
    const normalized = rawPath === null ? null : normalizePath(rawPath)
    if (normalized === (entry.path ?? null)) {
      setOpen(false)
      return
    }
    try {
      await updateItem({
        text: entry.text,
        source_id: entry.source_id,
        metadata: entry.metadata,
        path: normalized ?? null,
      })
      await mutate(["items", entry.id])
      await mutate(["entries-paths", entry.source_id])
      await mutate(["entries-paths", ""])
      toast.success(normalized ? `Filed under ${normalized}` : "Removed from wiki")
      setOpen(false)
      setInput("")
    } catch {
      toast.error("Failed to update wiki path")
    }
  }

  const currentPath = entry.path ?? null
  const candidate = normalizePath(input)

  return (
    <Popover open={open} onOpenChange={(o) => { setOpen(o); if (!o) setInput("") }}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className={cn(
            "font-mono text-[10px] uppercase tracking-[1.5px] h-6 px-2 gap-1.5",
            currentPath ? "text-primary border-primary/40 hover:bg-primary/10" : "text-muted-foreground"
          )}
        >
          {currentPath ? (
            <>
              <Pencil className="size-3" />
              {currentPath}
            </>
          ) : (
            <>
              <FolderPlus className="size-3" />
              Add to wiki
            </>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-80 p-0 overflow-hidden rounded-xl border-border shadow-xl"
      >
        <Command shouldFilter={false} loop>
          <CommandInput
            placeholder="Type path (e.g. team/handbook)…"
            value={input}
            onValueChange={setInput}
            className="h-11 border-none text-sm"
          />
          <CommandList>
            {isMutating && (
              <div className="py-4 flex items-center justify-center gap-2 text-muted-foreground">
                <LoaderCircle className="size-3.5 animate-spin" />
                <span className="font-mono text-[10px] uppercase tracking-wider">Saving…</span>
              </div>
            )}

            {filtered.length === 0 && !input && (
              <CommandEmpty className="py-6 text-center text-xs text-muted-foreground">
                No existing paths in <span className="font-mono">{entry.source_id}</span>.
                <br />Type a new one above.
              </CommandEmpty>
            )}

            {filtered.length > 0 && (
              <CommandGroup heading="Existing paths">
                {filtered.map((path) => (
                  <CommandItem
                    key={path}
                    value={path}
                    onSelect={() => apply(path)}
                    className="cursor-pointer font-mono text-xs"
                  >
                    <span className="flex-1 truncate">{path}</span>
                    {path === currentPath && <Check className="size-3.5 text-primary" />}
                  </CommandItem>
                ))}
              </CommandGroup>
            )}

            {candidate && !suggestions.includes(candidate) && (
              <CommandGroup heading="Create">
                <CommandItem
                  value={`__create__${candidate}`}
                  onSelect={() => apply(candidate)}
                  className="cursor-pointer font-mono text-xs text-primary"
                >
                  <FolderPlus className="size-3.5 mr-2" />
                  <span className="truncate">{candidate}</span>
                </CommandItem>
              </CommandGroup>
            )}

            {currentPath && (
              <CommandGroup heading="Remove">
                <CommandItem
                  value="__clear__"
                  onSelect={() => apply(null)}
                  className="cursor-pointer text-xs text-destructive"
                >
                  Remove from wiki
                </CommandItem>
              </CommandGroup>
            )}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
