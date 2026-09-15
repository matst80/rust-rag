"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { Search, FileText, ArrowRight, Loader2 } from "lucide-react"
import {
  CommandDialog,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
} from "@/components/ui/command"
import { api } from "@/lib/api"
import type { SearchResult } from "@/lib/api/types"

export function HeaderSearch() {
  const router = useRouter()
  const [open, setOpen] = React.useState(false)
  const [query, setQuery] = React.useState("")
  const [results, setResults] = React.useState<SearchResult[]>([])
  const [isLoading, setIsLoading] = React.useState(false)

  // Keyboard shortcut: Cmd+K / Ctrl+K or / to open
  React.useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault()
        setOpen((prev) => !prev)
      } else if (
        e.key === "/" &&
        !["INPUT", "TEXTAREA"].includes((e.target as HTMLElement)?.tagName)
      ) {
        e.preventDefault()
        setOpen(true)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  // Debounced search (default: NO rerank, sparse/hybrid keyword-friendly, not dense-only)
  React.useEffect(() => {
    if (!query.trim()) {
      setResults([])
      setIsLoading(false)
      return
    }

    const timer = setTimeout(async () => {
      setIsLoading(true)
      try {
        const bundle = await api.search({
          query: query.trim(),
          top_k: 6,
          hybrid: true,
          rerank: false,
        })
        setResults(bundle.results)
      } catch {
        setResults([])
      } finally {
        setIsLoading(false)
      }
    }, 200)

    return () => clearTimeout(timer)
  }, [query])

  const handleSelect = (id: string) => {
    setOpen(false)
    router.push(`/entries/${encodeURIComponent(id)}`)
  }

  const handleFullSearch = () => {
    setOpen(false)
    const params = new URLSearchParams({
      q: query,
      mode: "basic",
      hybrid: "true",
      rerank: "false",
    })
    router.push(`/?${params.toString()}`)
  }

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="flex items-center gap-2 h-7 px-2.5 rounded border border-border bg-muted/30 hover:bg-muted/60 text-muted-foreground hover:text-foreground text-xs font-mono transition-colors shadow-2xs"
        title="Search entries (⌘K or /)"
      >
        <Search className="size-3.5 opacity-60" />
        <span className="hidden md:inline">Quick search...</span>
        <span className="md:hidden">Search</span>
        <kbd className="hidden md:inline-flex items-center gap-0.5 rounded bg-background border border-border/80 px-1.5 py-0.5 text-[9px] font-sans font-semibold text-muted-foreground">
          ⌘K
        </kbd>
      </button>

      <CommandDialog open={open} onOpenChange={setOpen} title="Quick Search">
        <CommandInput
          placeholder="Search entries... (press Enter to run full search)"
          value={query}
          onValueChange={setQuery}
          onKeyDown={(e) => {
            if (e.key === "Enter" && query.trim() && results.length === 0) {
              e.preventDefault()
              handleFullSearch()
            }
          }}
        />
        <CommandList>
          {isLoading && (
            <div className="flex items-center justify-center py-6 gap-2 text-xs font-mono text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" />
              <span>Searching...</span>
            </div>
          )}

          {!isLoading && query.trim() && results.length === 0 && (
            <CommandEmpty>
              <div className="py-2">
                <p className="text-sm text-muted-foreground">No quick results found.</p>
                <button
                  type="button"
                  onClick={handleFullSearch}
                  className="mt-2 inline-flex items-center gap-1.5 text-xs text-primary hover:underline font-mono"
                >
                  <span>Open full search for "{query}"</span>
                  <ArrowRight className="size-3" />
                </button>
              </div>
            </CommandEmpty>
          )}

          {results.length > 0 && (
            <CommandGroup heading="Entries">
              {results.map((r) => (
                <CommandItem
                  key={r.id}
                  value={`${r.id} ${r.text.slice(0, 50)}`}
                  onSelect={() => handleSelect(r.id)}
                  className="flex items-center gap-2 cursor-pointer py-2.5"
                >
                  <FileText className="size-4 text-primary shrink-0" />
                  <div className="flex flex-col min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-xs font-bold truncate">
                        {r.id}
                      </span>
                      <span className="font-mono text-[9px] uppercase px-1.5 py-0.5 rounded border border-border text-muted-foreground shrink-0">
                        {r.source_id}
                      </span>
                    </div>
                    <p className="text-xs text-muted-foreground/80 line-clamp-1 truncate mt-0.5">
                      {r.text}
                    </p>
                  </div>
                </CommandItem>
              ))}

              <CommandItem
                onSelect={handleFullSearch}
                className="justify-center border-t border-border/40 text-xs font-mono text-primary font-medium cursor-pointer py-2"
              >
                <span>View all results on Search page →</span>
              </CommandItem>
            </CommandGroup>
          )}
        </CommandList>
      </CommandDialog>
    </>
  )
}
