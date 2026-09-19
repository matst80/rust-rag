"use client";

import { useState, useMemo, useEffect, useRef } from "react";
import { useSessionState } from "@/hooks/use-session-state";
import Link from "next/link";
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
  ArrowUpDown,
  Filter,
  X,
  Tag as TagIcon,
} from "lucide-react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardAction,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useItems, useDeleteItem, useSchemas, type SortOrder } from "@/lib/api";
import { useSWRConfig } from "swr";
import { cn } from "@/lib/utils";
import type { Entry } from "@/lib/api";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import { EntryCard } from "./entry-card";

interface EntriesListProps {
  selectedCategory: string | null;
  selectedPath?: string | null;
}

export function EntriesList({ selectedCategory, selectedPath }: EntriesListProps) {
  const [localSearch, setLocalSearch] = useSessionState<string>(
    "entries-list:search",
    "",
  );
  const [page, setPage] = useSessionState<number>("entries-list:page", 1);
  const [sortOrder, setSortOrder] = useSessionState<SortOrder>(
    "entries-list:sort",
    "desc",
  );
  const [selectedType, setSelectedType] = useSessionState<string | null>(
    "entries-list:type",
    null,
  );
  const [propertyFilters, setPropertyFilters] = useSessionState<
    Record<string, string>
  >("entries-list:props", {});
  const [filterKeyDraft, setFilterKeyDraft] = useState("");
  const [filterValueDraft, setFilterValueDraft] = useState("");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const { data: schemas } = useSchemas();
  const PAGE_SIZE = 20;

  const lastFilters = useRef<{
    category: string | null;
    path: string | null | undefined;
    sort: SortOrder;
    type: string | null;
    props: string;
  }>({
    category: selectedCategory,
    path: selectedPath,
    sort: sortOrder,
    type: selectedType,
    props: JSON.stringify(propertyFilters),
  });
  useEffect(() => {
    const propsKey = JSON.stringify(propertyFilters);
    const prev = lastFilters.current;
    if (
      prev.category !== selectedCategory ||
      prev.path !== selectedPath ||
      prev.sort !== sortOrder ||
      prev.type !== selectedType ||
      prev.props !== propsKey
    ) {
      lastFilters.current = {
        category: selectedCategory,
        path: selectedPath,
        sort: sortOrder,
        type: selectedType,
        props: propsKey,
      };
      setPage(1);
    }
  }, [selectedCategory, selectedPath, sortOrder, selectedType, propertyFilters, setPage]);

  const { data: pagedData, isLoading } = useItems({
    source_id: selectedCategory ?? undefined,
    path_prefix: selectedPath ?? undefined,
    limit: PAGE_SIZE,
    offset: (page - 1) * PAGE_SIZE,
    sort_order: sortOrder,
    type: selectedType ?? undefined,
    metadata:
      Object.keys(propertyFilters).length > 0 ? propertyFilters : undefined,
  });

  const addPropertyFilter = () => {
    const key = filterKeyDraft.trim();
    const value = filterValueDraft.trim();
    if (!key || !value) return;
    setPropertyFilters((prev) => ({ ...prev, [key]: value }));
    setFilterKeyDraft("");
    setFilterValueDraft("");
  };

  const removePropertyFilter = (key: string) => {
    setPropertyFilters((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
  };

  const activeFilterCount =
    Object.keys(propertyFilters).length + (selectedType ? 1 : 0);

  const entries = pagedData?.items;
  const totalCount = pagedData?.total_count ?? 0;

  const { trigger: deleteItem } = useDeleteItem();
  const { mutate } = useSWRConfig();

  const filteredEntries = useMemo(() => {
    if (!entries) return [];
    if (!localSearch) return entries;
    const search = localSearch.toLowerCase();
    return entries.filter(
      (entry) =>
        entry.id.toLowerCase().includes(search) ||
        entry.text.toLowerCase().includes(search) ||
        entry.source_id.toLowerCase().includes(search),
    );
  }, [entries, localSearch]);

  const totalPages = Math.ceil(totalCount / PAGE_SIZE);

  const handleDelete = async (id: string) => {
    await deleteItem(id);
    mutate("items");
    if (selectedCategory) {
      mutate(["items", selectedCategory]);
    }
    mutate("categories");
  };

  const hasActiveFilters = activeFilterCount > 0;

  const filterBar = (
    <div className="space-y-3">
      {/* Filters and Search Bar */}
      <div className="flex flex-col sm:flex-row gap-3">
        <div className="relative flex-1 group">
          <Search className="absolute left-4 top-1/2 mt-0.5 size-4 -translate-y-1/2 text-muted-foreground/60 transition-colors group-focus-within:text-primary" />
          <input
            type="text"
            placeholder="Search page content..."
            className="w-full h-12 bg-muted/20 border-muted/40 rounded-2xl pl-12 pr-12 sm:pr-4 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all hover:bg-muted/30"
            value={localSearch}
            onChange={(e) => setLocalSearch(e.target.value)}
          />

          <div className="absolute right-2 top-1/2 -translate-y-1/2 sm:hidden">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-8 rounded-xl"
                >
                  <ArrowUpDown className="size-4 opacity-60" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                align="end"
                className="w-48 rounded-xl p-1"
              >
                    <DropdownMenuItem
                      onClick={() => setSortOrder("desc")}
                      className={cn(
                        "rounded-lg",
                        sortOrder === "desc" && "bg-primary/10 text-primary",
                      )}
                    >
                      Newest First
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      onClick={() => setSortOrder("asc")}
                      className={cn(
                        "rounded-lg",
                        sortOrder === "asc" && "bg-primary/10 text-primary",
                      )}
                    >
                      Oldest First
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </div>

            <div className="hidden sm:block">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="outline"
                    className="h-12 rounded-2xl border-muted px-5"
                  >
                    <ArrowUpDown className="mr-2 size-4 opacity-60" />
                    <span className="font-semibold text-xs uppercase tracking-wider">
                      {sortOrder === "desc" ? "Newest First" : "Oldest First"}
                    </span>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent
                  align="end"
                  className="w-48 rounded-xl p-1"
                >
                  <DropdownMenuItem
                    onClick={() => setSortOrder("desc")}
                    className={cn(
                      "rounded-lg",
                      sortOrder === "desc" && "bg-primary/10 text-primary",
                    )}
                  >
                    Newest First
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onClick={() => setSortOrder("asc")}
                    className={cn(
                      "rounded-lg",
                      sortOrder === "asc" && "bg-primary/10 text-primary",
                    )}
                  >
                    Oldest First
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            <Select
              value={selectedType ?? "__all__"}
              onValueChange={(value) =>
                setSelectedType(value === "__all__" ? null : value)
              }
            >
              <SelectTrigger className="h-12 rounded-2xl border-muted px-5 w-full sm:w-[180px]">
                <SelectValue placeholder="All types" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__all__">All types</SelectItem>
                {schemas?.map((s) => (
                  <SelectItem key={s.type_name} value={s.type_name}>
                    {s.type_name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Popover open={filtersOpen} onOpenChange={setFiltersOpen}>
              <PopoverTrigger asChild>
                <Button
                  variant="outline"
                  className={cn(
                    "h-12 rounded-2xl border-muted px-5 shrink-0",
                    Object.keys(propertyFilters).length > 0 &&
                      "border-primary/40 text-primary bg-primary/5",
                  )}
                >
                  <Filter className="mr-2 size-4 opacity-60" />
                  <span className="font-semibold text-xs uppercase tracking-wider">
                    Properties
                    {Object.keys(propertyFilters).length > 0 &&
                      ` (${Object.keys(propertyFilters).length})`}
                  </span>
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" className="w-80 rounded-2xl p-4 space-y-4">
                <div className="space-y-1.5">
                  <p className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    Filter by property
                  </p>
                  <p className="text-[11px] text-muted-foreground/70 leading-relaxed">
                    Exact match on a metadata field (e.g. author, repo, doc_type, tags).
                  </p>
                </div>
                <div className="flex gap-2">
                  <Input
                    placeholder="key"
                    value={filterKeyDraft}
                    onChange={(e) => setFilterKeyDraft(e.target.value)}
                    className="h-9 text-sm"
                  />
                  <Input
                    placeholder="value"
                    value={filterValueDraft}
                    onChange={(e) => setFilterValueDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") addPropertyFilter();
                    }}
                    className="h-9 text-sm"
                  />
                  <Button
                    size="sm"
                    className="h-9 px-3 shrink-0"
                    onClick={addPropertyFilter}
                    disabled={!filterKeyDraft.trim() || !filterValueDraft.trim()}
                  >
                    Add
                  </Button>
                </div>
                {Object.keys(propertyFilters).length > 0 && (
                  <div className="flex flex-wrap gap-1.5 pt-1">
                    {Object.entries(propertyFilters).map(([key, value]) => (
                      <Badge
                        key={key}
                        variant="secondary"
                        className="gap-1.5 rounded-full pl-2.5 pr-1 py-1 text-[11px] font-mono"
                      >
                        {key}={value}
                        <button
                          type="button"
                          onClick={() => removePropertyFilter(key)}
                          className="rounded-full p-0.5 hover:bg-muted-foreground/20"
                        >
                          <X className="size-3" />
                        </button>
                      </Badge>
                    ))}
                  </div>
                )}
              </PopoverContent>
            </Popover>
          </div>
        {hasActiveFilters && (
          <div className="flex flex-wrap items-center gap-2">
            <span className="flex items-center gap-1 text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60">
              <TagIcon className="size-3" />
              Active filters:
            </span>
            {selectedType && (
              <Badge
                variant="outline"
                className="gap-1.5 rounded-full pl-2.5 pr-1 py-1 text-[11px] font-mono border-primary/30 text-primary bg-primary/5"
              >
                type={selectedType}
                <button
                  type="button"
                  onClick={() => setSelectedType(null)}
                  className="rounded-full p-0.5 hover:bg-primary/20"
                >
                  <X className="size-3" />
                </button>
              </Badge>
            )}
            {Object.entries(propertyFilters).map(([key, value]) => (
              <Badge
                key={key}
                variant="outline"
                className="gap-1.5 rounded-full pl-2.5 pr-1 py-1 text-[11px] font-mono border-primary/30 text-primary bg-primary/5"
              >
                {key}={value}
                <button
                  type="button"
                  onClick={() => removePropertyFilter(key)}
                  className="rounded-full p-0.5 hover:bg-primary/20"
                >
                  <X className="size-3" />
                </button>
              </Badge>
            ))}
            <button
              type="button"
              onClick={() => {
                setSelectedType(null);
                setPropertyFilters({});
              }}
              className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 hover:text-primary transition-colors ml-1"
            >
              Clear all
            </button>
          </div>
        )}
      </div>
    );

  if (isLoading) {
    return (
      <div className="flex flex-col gap-6 md:gap-8 p-4 md:p-10 animate-in fade-in slide-in-from-bottom-4 duration-700">
        {filterBar}
        <div className="flex flex-col gap-4">
          {[1, 2, 3, 4, 5].map((i) => (
            <div
              key={i}
              className="h-24 animate-pulse rounded-[1.5rem] bg-muted/10 border border-muted/5"
            />
          ))}
        </div>
      </div>
    );
  }

  if (!entries || entries.length === 0) {
    return (
      <div className="flex flex-col gap-6 md:gap-8 p-4 md:p-10 animate-in fade-in slide-in-from-bottom-4 duration-700">
        {filterBar}
        <div className="flex flex-1 flex-col items-center justify-center py-24 text-center animate-in fade-in slide-in-from-bottom-8 duration-1000">
          <div className="mb-8 flex size-24 items-center justify-center rounded-3xl bg-muted/10 border border-muted/20 shadow-inner">
            <Database className="size-10 text-muted-foreground/40" />
          </div>
          <h3 className="mb-4 text-2xl font-bold tracking-tight">
            Intelligence Pool Empty
          </h3>
          <p className="mb-10 text-muted-foreground text-lg max-w-sm mx-auto leading-relaxed">
            {hasActiveFilters
              ? "No records match the current filters. Try clearing one."
              : selectedPath
                ? `No records found under "${selectedPath}".`
                : selectedCategory
                  ? `No records found in the "${selectedCategory}" collection.`
                  : "Your neural network of memories is currently offline. Start by creating a record."}
          </p>
          <Button
            size="lg"
            className="h-12 rounded-2xl px-8 shadow-lg shadow-primary/20 transition-all hover:scale-105 active:scale-95"
            asChild
          >
            <Link href="/entries/new">
              <Plus className="mr-2 size-5" />
              Create Entry
            </Link>
          </Button>
        </div>
      </div>
    );
  }

  return (
    <>
      <div className="flex flex-col gap-6 md:gap-8 p-4 md:p-10 animate-in fade-in slide-in-from-bottom-4 duration-700">
        {/* Header Area */}
        <div className="space-y-6">
          {filterBar}
        </div>

        {/* List of Cards */}
        <div className="flex flex-col">
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
              <p className="text-muted-foreground font-medium">
                No records found matching "{localSearch}"
              </p>
            </div>
          )}
        </div>

        {totalPages > 1 && (
          <div className="flex flex-col sm:flex-row items-center justify-between gap-4 pt-6 border-t border-muted/20">
            <div className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
              Page {page} of {totalPages} — {totalCount} total records
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="icon"
                className="size-10 rounded-xl"
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={page === 1}
              >
                <ChevronLeft className="size-4" />
              </Button>
              <Button
                variant="outline"
                size="icon"
                className="size-10 rounded-xl"
                onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
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

      {/* Mobile FAB */}
      <Button
        size="icon"
        className="fixed bottom-6 right-6 size-14 rounded-full shadow-2xl shadow-primary/40 z-50 animate-in zoom-in duration-300"
        asChild
      >
        <Link href="/entries/new">
          <Plus className="size-6" />
        </Link>
      </Button>
    </>
  );
}
