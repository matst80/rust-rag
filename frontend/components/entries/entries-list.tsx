"use client"

import { useState, useMemo } from "react"
import Link from "next/link"
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
  Database,
  Filter
} from "lucide-react"
import { Card, CardContent, CardHeader, CardTitle, CardAction } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useItems, useDeleteItem } from "@/lib/api"
import { useSWRConfig } from "swr"
import { cn } from "@/lib/utils"
import type { Entry } from "@/lib/api"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

interface EntriesListProps {
  selectedCategory: string | null
}

export function EntriesList({ selectedCategory }: EntriesListProps) {
  const [localSearch, setLocalSearch] = useState("")
  const { data: entries, isLoading } = useItems(selectedCategory ?? undefined)
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

  const handleDelete = async (id: string) => {
    if (!confirm("Are you sure you want to delete this entry?")) return
    await deleteItem(id)
    mutate("items")
    if (selectedCategory) {
      mutate(["items", selectedCategory])
    }
    mutate("categories")
  }

  if (isLoading) {
    return (
      <div className="flex flex-col gap-8 p-10 animate-in fade-in duration-500">
        <div className="space-y-4">
          <div className="h-8 w-64 animate-pulse rounded-lg bg-muted/40" />
          <div className="h-10 w-full animate-pulse rounded-2xl bg-muted/20" />
        </div>
        <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3, 4, 5, 6].map((i) => (
            <div key={i} className="h-64 animate-pulse rounded-3xl bg-muted/10 border border-muted/20" />
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
          <Link href="/entries/new">
            <Plus className="mr-2 size-5" />
            Create Entry
          </Link>
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
            <Link href="/entries/new">
              <Plus className="mr-2 size-5" />
              New Record
            </Link>
          </Button>
        </div>

        {/* Filters and Search Bar */}
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="relative flex-1 group">
            <Search className="absolute left-4 top-1/2 mt-0.5 size-4 -translate-y-1/2 text-muted-foreground/60 transition-colors group-focus-within:text-primary" />
            <input
              type="text"
              placeholder="Search through intelligence records..."
              className="w-full h-12 bg-muted/20 border-muted/40 rounded-2xl pl-12 pr-4 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all hover:bg-muted/30"
              value={localSearch}
              onChange={(e) => setLocalSearch(e.target.value)}
            />
          </div>
          <Button variant="outline" className="h-12 rounded-2xl border-muted px-5">
            <Filter className="mr-2 size-4 opacity-60" />
            <span className="font-semibold text-xs uppercase tracking-wider">Filters</span>
          </Button>
        </div>
      </div>

      {/* Grid of Cards */}
      <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
        {filteredEntries.map((entry, index) => (
          <EntryCard
            key={entry.id}
            entry={entry}
            index={index}
            onDelete={handleDelete}
          />
        ))}
        {localSearch && filteredEntries.length === 0 && (
          <div className="col-span-full py-20 text-center">
            <p className="text-muted-foreground font-medium">No records found matching "{localSearch}"</p>
          </div>
        )}
      </div>

      <div className="flex items-center justify-center pt-10">
        <p className="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
          End of intelligence pool — {filteredEntries.length} records retrieved
        </p>
      </div>
    </div>
  )
}

function EntryCard({
  entry,
  index,
  onDelete,
}: {
  entry: Entry
  index: number
  onDelete: (id: string) => void
}) {
  const getSourceVariant = (source: string) => {
    const s = source.toLowerCase()
    if (s.includes('manual')) return 'bg-amber-500/10 text-amber-600 dark:text-amber-400'
    if (s.includes('auto')) return 'bg-sky-500/10 text-sky-600 dark:text-sky-400'
    if (s.includes('imported')) return 'bg-purple-500/10 text-purple-600 dark:text-purple-400'
    return 'bg-muted text-muted-foreground'
  }

  return (
    <Card
      className={cn(
        "group relative flex flex-col overflow-hidden transition-all duration-300",
        "bg-muted/5 hover:bg-background hover:shadow-xl hover:shadow-primary/5",
        "border-muted/30 hover:border-primary/20 rounded-lg"
      )}
      style={{ animationDelay: `${index * 30}ms` }}
    >
      {/* Clickable Area for Details */}
      <Link
        href={`/entries/${encodeURIComponent(entry.id)}`}
        className="absolute inset-0 z-10"
        aria-label={`View details for ${entry.id}`}
      />

      <CardHeader className="pb-1 pt-4 px-5 space-y-0 z-20">
        <div className="flex items-center gap-2">
          <FileText className="size-3.5 text-primary/60 shrink-0" />
          <CardTitle className="text-sm font-bold tracking-tight opacity-70 group-hover:opacity-100 transition-opacity truncate max-w-[120px]">
            {entry.id}
          </CardTitle>
          <span className={cn(
            "px-1.5 py-0.5 rounded-md text-[7px] font-extrabold uppercase tracking-widest shrink-0",
            getSourceVariant(entry.source_id)
          )}>
            {entry.source_id}
          </span>
        </div>

        <CardAction className="relative z-30">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button className="size-7 flex items-center justify-center rounded-md opacity-0 group-hover:opacity-100 transition-all hover:bg-accent text-muted-foreground hover:text-accent-foreground">
                <MoreVertical className="size-3.5" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="rounded-xl border-muted/40 shadow-xl z-50">
              <DropdownMenuItem className="text-xs font-semibold rounded-lg px-3 py-2 cursor-pointer">
                <Share2 className="mr-2 size-3.5" /> Share
              </DropdownMenuItem>
              <DropdownMenuItem
                className="text-xs font-semibold rounded-lg px-3 py-2 text-destructive cursor-pointer hover:!bg-destructive/10"
                onClick={(e) => {
                  e.stopPropagation()
                  onDelete(entry.id)
                }}
              >
                <Trash2 className="mr-2 size-3.5" /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </CardAction>
      </CardHeader>

      <CardContent className="flex-1 flex flex-col px-5 pb-3 pt-2 z-20">
        <p className="line-clamp-4 text-[15px] lg:text-[16px] text-foreground/80 leading-snug font-medium transition-colors group-hover:text-foreground">
          {entry.text}
        </p>

        {/* Hover-only Meta Section - tighter */}
        <div className="mt-3 overflow-hidden min-h-[0px] group-hover:min-h-[40px] transition-all duration-300">
          <div className="opacity-0 group-hover:opacity-100 transition-all duration-300">
            {Object.keys(entry.metadata).length > 0 && (
              <div className="flex flex-wrap gap-1 mb-3">
                {Object.entries(entry.metadata)
                  .slice(0, 3)
                  .map(([key, value]) => (
                    <div key={key} className="flex items-center rounded-md bg-muted/40 px-1.5 py-0.5 text-[8px] font-bold text-muted-foreground/60 leading-none">
                      {key}: <span className="text-foreground/70 ml-1 truncate max-w-[80px]">{String(value)}</span>
                    </div>
                  ))}
              </div>
            )}

            <div className="flex items-center justify-between pt-2 border-t border-muted/20">
              <div className="flex items-center text-[8px] font-bold text-muted-foreground/30 gap-1.5 uppercase tracking-widest">
                <Calendar className="size-3" />
                <span>Synced</span>
              </div>
              <div className="flex items-center gap-1.5 text-[9px] font-bold text-primary transition-all">
                Explore Intelligence <ChevronRight className="size-3" />
              </div>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
