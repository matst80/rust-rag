"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTheme } from "next-themes"
import {
  Search,
  FileText,
  ArrowRight,
  Loader2,
  Plus,
  CalendarDays,
  Inbox,
  FolderTree,
  GitBranch,
  Flame,
  Check,
  LayoutGrid,
  MessageSquare,
  Database,
  Braces,
  Code,
  Radar,
  Star,
  History,
  SunMoon,
  Table2,
} from "lucide-react"
import {
  Command,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandSeparator,
} from "@/components/ui/command"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { api } from "@/lib/api"
import { useDuplicateTitles, useSchemas } from "@/lib/api/hooks"
import type { SearchResult } from "@/lib/api/types"
import { entryTitle, entryTitleUnclipped, normalizeTitle } from "@/lib/utils"
import { DuplicateTitleBadge } from "@/components/entries/duplicate-title-badge"
import { getRecents, getStarred, type PinnedEntry } from "@/lib/favorites"
import { toast } from "sonner"

function todayId(): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, "0")
  return `daily-${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

interface PaletteView {
  name: string
  href: string
  icon: typeof Search
  keywords?: string
}

const VIEWS: PaletteView[] = [
  { name: "Search", href: "/", icon: Search, keywords: "find query" },
  { name: "Today", href: "/today", icon: CalendarDays, keywords: "daily note journal" },
  { name: "Journal", href: "/journal", icon: History, keywords: "daily notes calendar streak tasks" },
  { name: "Databases", href: "/databases", icon: Database, keywords: "table board calendar rows schemas" },
  { name: "Entries", href: "/entries", icon: FolderTree, keywords: "list browse all" },
  { name: "Wiki", href: "/wiki", icon: FolderTree, keywords: "tree paths folders" },
  { name: "Graph", href: "/visualize", icon: GitBranch, keywords: "network links visualize" },
  { name: "Kanban", href: "/kanban", icon: LayoutGrid, keywords: "board tasks todo cards" },
  { name: "Chat", href: "/chat", icon: MessageSquare, keywords: "talk assistant" },
  { name: "Deep Search", href: "/assisted", icon: Radar, keywords: "ai assisted multi query" },
  { name: "Code", href: "/code", icon: Code, keywords: "repo symbols search" },
  { name: "Grill", href: "/grill", icon: Flame, keywords: "harness audit review" },
  { name: "Schemas", href: "/schemas", icon: Braces, keywords: "types structured data" },
]

export function HeaderSearch() {
  const router = useRouter()
  const { resolvedTheme, setTheme } = useTheme()
  const [open, setOpen] = React.useState(false)
  const [query, setQuery] = React.useState("")
  const [results, setResults] = React.useState<SearchResult[]>([])
  const [isLoading, setIsLoading] = React.useState(false)
  const [capturing, setCapturing] = React.useState(false)
  const [captured, setCaptured] = React.useState(false)
  const [recents, setRecents] = React.useState<PinnedEntry[]>([])
  const [starred, setStarred] = React.useState<PinnedEntry[]>([])
  const { data: schemas } = useSchemas()
  const { data: dupData } = useDuplicateTitles()
  const dupGroupFor = (title: string) => {
    const key = normalizeTitle(title)
    return dupData?.groups.find((g) => normalizeTitle(g.title) === key) ?? null
  }

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

  // Refresh recents/starred every time the palette opens.
  React.useEffect(() => {
    if (open) {
      setStarred(getStarred())
      const starredIds = new Set(getStarred().map((e) => e.id))
      setRecents(getRecents().filter((e) => !starredIds.has(e.id)).slice(0, 5))
    }
  }, [open ])

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

  const close = () => {
    setOpen(false)
    setQuery("")
  }

  const handleSelect = (id: string) => {
    close()
    router.push(`/entries/${encodeURIComponent(id)}`)
  }

  const go = (href: string) => {
    close()
    router.push(href)
  }

  const toggleTheme = () => {
    setTheme(resolvedTheme === "dark" ? "light" : "dark")
    close()
  }

  /** Dump the query as an unorganized inbox entry without leaving context. */
  const quickCapture = async () => {
    const text = query.trim()
    if (!text || capturing) return
    setCapturing(true)
    try {
      const entry = await api.items.create({
        text,
        metadata: { captured_via: "palette" },
        source_id: "knowledge",
      })
      setCaptured(true)
      toast.success("Captured to inbox", {
        action: {
          label: "Open",
          onClick: () => router.push(`/entries/${encodeURIComponent(entry.id)}`),
        },
      })
      setTimeout(() => {
        close()
        setCaptured(false)
      }, 600)
    } catch {
      toast.error("Capture failed")
    } finally {
      setCapturing(false)
    }
  }

  const handleFullSearch = () => {
    const q = query
    close()
    const params = new URLSearchParams({
      q,
      mode: "basic",
      hybrid: "true",
      rerank: "false",
    })
    router.push(`/?${params.toString()}`)
  }

  const q = query.trim().toLowerCase()
  const matchesQuery = (name: string, extra?: string) =>
    !q || name.toLowerCase().includes(q) || (extra ?? "").toLowerCase().includes(q)

  const matchedViews = VIEWS.filter((v) => matchesQuery(v.name, v.keywords))
  const topSchemas = React.useMemo(() => {
    const all = schemas ?? []
    const sorted = [...all].sort((a, b) => (b.item_count ?? 0) - (a.item_count ?? 0))
    return sorted
      .filter((s) => matchesQuery(s.type_name, `${s.title ?? ""} database table`))
      .slice(0, q ? 4 : 6)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schemas, query])
  const showRecents = !q && (starred.length > 0 || recents.length > 0)

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="flex h-9 w-full max-w-2xl items-center gap-2.5 rounded-md border border-border bg-muted/40 px-3.5 text-sm text-muted-foreground shadow-2xs transition-colors hover:border-primary/40 hover:bg-muted/70 hover:text-foreground"
        title="Command palette (⌘K or /)"
      >
        <Search className="size-4 shrink-0 opacity-60" />
        <span className="flex-1 truncate text-left">Type a command or search…</span>
        <kbd className="hidden shrink-0 items-center gap-0.5 rounded border border-border/80 bg-background px-1.5 py-0.5 font-sans text-[10px] font-semibold text-muted-foreground sm:inline-flex">
          ⌘K
        </kbd>
      </button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogHeader className="sr-only">
          <DialogTitle>Command palette</DialogTitle>
          <DialogDescription>Type a command or search entries…</DialogDescription>
        </DialogHeader>
        <DialogContent className="overflow-hidden p-0" showCloseButton={false}>
          <Command shouldFilter={false} className="[&_[cmdk-group-heading]]:text-muted-foreground **:data-[slot=command-input-wrapper]:h-12 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group]]:px-2 [&_[cmdk-group]:not([hidden])_~[cmdk-group]]:pt-0 [&_[cmdk-input-wrapper]_svg]:h-5 [&_[cmdk-input-wrapper]_svg]:w-5 [&_[cmdk-input]]:h-12 [&_[cmdk-item]]:px-2 [&_[cmdk-item]]:py-3 [&_[cmdk-item]_svg]:h-5 [&_[cmdk-item]_svg]:w-5">
            <CommandInput
              placeholder="Type a command or search… (Enter for full search)"
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

              {!isLoading && (
                <CommandEmpty>
                  <div className="py-2">
                    <p className="text-sm text-muted-foreground">No matches.</p>
                    {q && (
                      <div className="mt-2 flex flex-col gap-1">
                        <button
                          type="button"
                          onClick={quickCapture}
                          disabled={capturing}
                          className="inline-flex items-center gap-1.5 text-xs text-primary hover:underline font-mono disabled:opacity-50"
                        >
                          {captured ? (
                            <Check className="size-3 text-emerald-500" />
                          ) : capturing ? (
                            <Loader2 className="size-3 animate-spin" />
                          ) : (
                            <Inbox className="size-3" />
                          )}
                          <span>
                            {captured
                              ? "Captured to inbox!"
                              : capturing
                                ? "Capturing…"
                                : `Capture "${query.trim().slice(0, 60)}" to inbox`}
                          </span>
                        </button>
                        <button
                          type="button"
                          onClick={handleFullSearch}
                          className="inline-flex items-center gap-1.5 text-xs text-muted-foreground hover:underline font-mono"
                        >
                          <span>Open full search for &quot;{query}&quot;</span>
                          <ArrowRight className="size-3" />
                        </button>
                      </div>
                    )}
                  </div>
                </CommandEmpty>
              )}

              {showRecents && (
                <CommandGroup heading="Jump back in">
                  {starred.map((e) => (
                    <CommandItem key={`star-${e.id}`} onSelect={() => handleSelect(e.id)} className="cursor-pointer gap-2">
                      <Star className="size-4 shrink-0 fill-amber-400 text-amber-400" />
                      <span className="truncate text-sm">{e.title}</span>
                    </CommandItem>
                  ))}
                  {recents.map((e) => (
                    <CommandItem key={`recent-${e.id}`} onSelect={() => handleSelect(e.id)} className="cursor-pointer gap-2">
                      <History className="size-4 shrink-0 text-muted-foreground" />
                      <span className="truncate text-sm">{e.title}</span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {!isLoading && results.length > 0 && (
                <CommandGroup heading="Entries">
                  {results.map((r) => {
                    const dup = dupGroupFor(entryTitleUnclipped(r))
                    return (
                    <CommandItem
                      key={r.id}
                      onSelect={() => handleSelect(r.id)}
                      className="flex items-center gap-2 cursor-pointer py-2.5"
                    >
                      <FileText className="size-4 text-primary shrink-0" />
                      <div className="flex flex-col min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-semibold truncate">
                            {entryTitle(r)}
                          </span>
                          {dup && <DuplicateTitleBadge group={dup} />}
                          <span className="font-mono text-[9px] uppercase px-1.5 py-0.5 rounded border border-border text-muted-foreground shrink-0">
                            {r.source_id}
                          </span>
                          {dup && (
                            <span className="font-mono text-[9px] text-muted-foreground/70 shrink-0">
                              {r.id.slice(0, 8)}…
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-muted-foreground/80 line-clamp-1 truncate mt-0.5">
                          {r.text}
                        </p>
                      </div>
                    </CommandItem>
                    )
                  })}
                  <CommandItem
                    onSelect={handleFullSearch}
                    className="justify-center border-t border-border/40 text-xs font-mono text-primary font-medium cursor-pointer py-2"
                  >
                    <span>View all results on Search page →</span>
                  </CommandItem>
                </CommandGroup>
              )}

              {matchedViews.length > 0 && (
                <CommandGroup heading="Go to">
                  {matchedViews.map((v) => (
                    <CommandItem key={v.href} onSelect={() => go(v.href)} className="cursor-pointer gap-2">
                      <v.icon className="size-4 shrink-0 text-muted-foreground" />
                      <span className="text-sm">{v.name}</span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              {topSchemas.length > 0 && (
                <CommandGroup heading="Databases">
                  {topSchemas.map((s) => (
                    <CommandItem
                      key={s.type_name}
                      onSelect={() => go(`/databases/${encodeURIComponent(s.type_name)}`)}
                      className="cursor-pointer gap-2"
                    >
                      <Table2 className="size-4 shrink-0 text-muted-foreground" />
                      <span className="truncate text-sm">
                        {s.title ?? s.type_name}
                        <span className="ml-1.5 font-mono text-[10px] text-muted-foreground">{s.type_name}</span>
                      </span>
                      <span className="ml-auto font-mono text-[10px] tabular-nums text-muted-foreground">
                        {s.item_count ?? 0}
                      </span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}

              <CommandSeparator />
              <CommandGroup heading="Actions">
                <CommandItem onSelect={() => go("/entries/new")} className="cursor-pointer gap-2">
                  <Plus className="size-4 shrink-0 text-primary" />
                  <span className="text-sm">New entry</span>
                </CommandItem>
                <CommandItem onSelect={() => go("/today")} className="cursor-pointer gap-2">
                  <CalendarDays className="size-4 shrink-0 text-primary" />
                  <span className="text-sm">Open today&apos;s note</span>
                  <span className="ml-auto font-mono text-[10px] text-muted-foreground">{todayId()}</span>
                </CommandItem>
                {q && (
                  <CommandItem onSelect={quickCapture} disabled={capturing} className="cursor-pointer gap-2">
                    {captured ? (
                      <Check className="size-4 text-emerald-500 shrink-0" />
                    ) : (
                      <Inbox className="size-4 text-primary shrink-0" />
                    )}
                    <span className="text-sm">
                      {captured ? "Captured to inbox!" : `Capture to inbox: "${query.trim().slice(0, 50)}"`}
                    </span>
                  </CommandItem>
                )}
                <CommandItem onSelect={toggleTheme} className="cursor-pointer gap-2">
                  <SunMoon className="size-4 shrink-0 text-muted-foreground" />
                  <span className="text-sm">Toggle theme</span>
                  <span className="ml-auto font-mono text-[10px] text-muted-foreground">
                    {resolvedTheme === "dark" ? "dark → light" : "light → dark"}
                  </span>
                </CommandItem>
              </CommandGroup>
            </CommandList>
          </Command>
        </DialogContent>
      </Dialog>
    </>
  )
}
