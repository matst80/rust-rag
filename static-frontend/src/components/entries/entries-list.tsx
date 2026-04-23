import { useState, useMemo, useEffect } from "react"
import {
  FileText,
  Plus,
  Trash2,
  Search,
  MoreVertical,
  Calendar,
  Share2,
  ExternalLink,
  ChevronRight,
  ChevronLeft,
  Database,
  Filter,
  ArrowUpDown,
} from "lucide-react"
import { Card, CardContent, CardHeader, CardTitle, CardAction } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useItems, useDeleteItem, type SortOrder } from "@/lib/api"
import { useSWRConfig } from "swr"
import { cn } from "@/lib/utils"
import type { Entry } from "@/lib/api"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

import { EntryCard } from "./entry-card"

interface EntriesListProps {
  selectedCategory: string | null
}

export function EntriesList({ selectedCategory }: EntriesListProps) {
  const [localSearch, setLocalSearch] = useState("")
  const [page, setPage] = useState(1)
  const [sortOrder, setSortOrder] = useState<SortOrder>("desc")
  const PAGE_SIZE = 20

  useEffect(() => {
    setPage(1)
  }, [selectedCategory, sortOrder])

  const { data: pagedData, isLoading } = useItems({
    source_id: selectedCategory ?? undefined,
    limit: PAGE_SIZE,
    offset: (page - 1) * PAGE_SIZE,
    sort_order: sortOrder,
  })

  const entries = pagedData?.items
  const totalCount = pagedData?.total_count ?? 0

  const { trigger: deleteItem } = useDeleteItem()
  const { mutate } = useSWRConfig()

  const filteredEntries = useMemo(() => {
    if (!entries) return []
    if (!localSearch) return entries
    const search = localSearch.toLowerCase()
    return entries.filter(
      (entry) =>
        entry.id.toLowerCase().includes(search) ||
        entry.text.toLowerCase().includes(search) ||
        entry.source_id.toLowerCase().includes(search)
    )
  }, [entries, localSearch])

  const totalPages = Math.ceil(totalCount / PAGE_SIZE)

  const handleDelete = async (id: string) => {
    await deleteItem(id)
    mutate("items")
    if (selectedCategory) {
      mutate(["items", selectedCategory])
    }
    mutate("categories")
  }

  if (isLoading) {
    return (
      <div className="flex flex-col gap-4 animate-in fade-in slide-in-from-bottom-8 duration-700">
        <div className="space-y-4">
          <div className="h-8 w-64 animate-pulse rounded-lg bg-muted/40" />
          <div className="h-10 w-full animate-pulse rounded-2xl bg-muted/20" />
        </div>
        <div className="flex flex-col gap-4">
          {[1, 2, 3, 4, 5].map((i) => (
            <div key={i} className="h-24 animate-pulse rounded-[1.5rem] bg-muted/10 border border-muted/5" />
          ))}
        </div>
      </div>
    )
  }

  if (!entries || entries.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center py-32 text-center animate-in fade-in slide-in-from-bottom-8 duration-1000">
        <div className="mb-8 flex size-24 items-center justify-center rounded-3xl bg-muted/10 border border-muted/20 shadow-inner">
          <Database className="size-10 text-muted-foreground/40" />
        </div>
        <h3 className="mb-4 text-2xl font-bold tracking-tight">Intelligence Pool Empty</h3>
        <p className="mb-10 text-muted-foreground text-lg max-w-sm mx-auto leading-relaxed">
          {selectedCategory
            ? `No records found in the "${selectedCategory}" collection.`
            : "Your neural network of memories is currently offline. Start by creating a record."}
        </p>
        <Button size="lg" className="h-12 rounded-2xl px-8 shadow-lg shadow-primary/20 transition-all hover:scale-105 active:scale-95" asChild>
          <a href="/entries/new/">
            <Plus className="mr-2 size-5" />
            Create Entry
          </a>
        </Button>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-8 p-8 md:p-10 animate-in fade-in slide-in-from-bottom-4 duration-700">
      {/* Header Area */}
      <div className="space-y-6">
        <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
          <div className="space-y-1.5">
            <div className="flex items-center gap-2 text-[10px] font-bold uppercase tracking-[0.2em] text-primary/60">
              <span className="opacity-50">Intelligence Browser</span>
              <ChevronRight className="size-3" />
              <span>{selectedCategory || "Global Memory"}</span>
            </div>
            <h1 className="text-3xl font-extrabold tracking-tight">
              {selectedCategory ? `${selectedCategory} Records` : "Unified Knowledge"}
            </h1>
          </div>

          <Button size="lg" className="rounded-2xl shadow-xl shadow-primary/10 transition-all hover:shadow-primary/20" asChild>
            <a href="/entries/new/">
              <Plus className="mr-2 size-5" />
              New Record
            </a>
          </Button>
        </div>

        {/* Filters and Search Bar */}
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="relative flex-1 group">
            <Search className="absolute left-4 top-1/2 mt-0.5 size-4 -translate-y-1/2 text-muted-foreground/60 transition-colors group-focus-within:text-primary" />
            <input
              type="text"
              placeholder="Search page content..."
              className="w-full h-12 bg-muted/20 border-muted/40 rounded-2xl pl-12 pr-4 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all hover:bg-muted/30"
              value={localSearch}
              onChange={(e) => setLocalSearch(e.target.value)}
            />
          </div>
          
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" className="h-12 rounded-2xl border-muted px-5">
                <ArrowUpDown className="mr-2 size-4 opacity-60" />
                <span className="font-semibold text-xs uppercase tracking-wider">
                  {sortOrder === "desc" ? "Newest First" : "Oldest First"}
                </span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48 rounded-xl p-1">
              <DropdownMenuItem 
                onClick={() => setSortOrder("desc")}
                className={cn("rounded-lg", sortOrder === "desc" && "bg-primary/10 text-primary")}
              >
                Newest First
              </DropdownMenuItem>
              <DropdownMenuItem 
                onClick={() => setSortOrder("asc")}
                className={cn("rounded-lg", sortOrder === "asc" && "bg-primary/10 text-primary")}
              >
                Oldest First
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          <Button variant="outline" className="h-12 rounded-2xl border-muted px-5">
            <Filter className="mr-2 size-4 opacity-60" />
            <span className="font-semibold text-xs uppercase tracking-wider">Filters</span>
          </Button>
        </div>
      </div>

      {/* List of Cards */}
      <div className="flex flex-col gap-4">
        {filteredEntries.map((entry, index) => (
          <EntryCard
            key={entry.id}
            entry={entry}
            index={index}
            onDelete={handleDelete}
          />
        ))}
        {localSearch && filteredEntries.length === 0 && (
          <div className="py-20 text-center">
            <p className="text-muted-foreground font-medium">No records found matching "{localSearch}"</p>
          </div>
        )}
      </div>

      {totalPages > 1 && (
        <div className="flex items-center justify-between pt-6 border-t border-muted/20">
          <div className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
            Page {page} of {totalPages} — {totalCount} total records
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="icon"
              className="size-10 rounded-xl"
              onClick={() => setPage(p => Math.max(1, p - 1))}
              disabled={page === 1}
            >
              <ChevronLeft className="size-4" />
            </Button>
            <Button
              variant="outline"
              size="icon"
              className="size-10 rounded-xl"
              onClick={() => setPage(p => Math.min(totalPages, p + 1))}
              disabled={page === totalPages}
            >
              <ChevronRight className="size-4" />
            </Button>
          </div>
        </div>
      )}

      <div className="flex items-center justify-center pt-10">
        <p className="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
          Neural manifold view — {filteredEntries.length} records shown
        </p>
      </div>
    </div>
  )
}

