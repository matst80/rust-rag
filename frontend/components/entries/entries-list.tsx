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

import { EntryCard } from "./entry-card"

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

      <div className="flex items-center justify-center pt-10">
        <p className="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
          End of intelligence pool — {filteredEntries.length} records retrieved
        </p>
      </div>
    </div>
  )
}

