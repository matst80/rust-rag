"use client";

import { getCollections, saveCollections, type SmartCollection } from "@/lib/smart-collections";
import { useEntryNavigation } from "@/hooks/use-entry-navigation";
import { ListSkeleton } from "@/components/ui/list-skeleton";
import { InboxAssignment } from "./inbox-assignment";
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
import { useItems, useDeleteItem, useBulkDeleteItems, useSchemas, useCategories, useMetadataFacets, type SortOrder } from "@/lib/api";
import { useSWRConfig } from "swr";
import { toast } from "sonner";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";
import type { Entry } from "@/lib/api";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import { EntryCard } from "./entry-card";

function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);
  return debounced;
}

const DATE_PRESETS = [
  { id: "any", label: "Any time", ms: null as number | null },
  { id: "day", label: "Past 24 hours", ms: 24 * 3600_000 },
  { id: "week", label: "Past 7 days", ms: 7 * 24 * 3600_000 },
  { id: "month", label: "Past 30 days", ms: 30 * 24 * 3600_000 },
  { id: "year", label: "Past year", ms: 365 * 24 * 3600_000 },
] as const;

const PAGE_SIZE_OPTIONS = [10, 20, 50, 100];

/** Normalize persisted/legacy property filters to key → values.
 *  Accepts the old single-string shape (`{tags: "work"}`) so saved
 *  collections and session state keep working after the upgrade. */
function normalizePropFilters(raw: unknown): Record<string, string[]> {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
  const out: Record<string, string[]> = {};
  for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!key) continue;
    const candidates = Array.isArray(value) ? value : [value];
    const clean = [...new Set(
      candidates
        .filter((v): v is string => typeof v === "string")
        .map((v) => v.trim())
        .filter(Boolean),
    )];
    if (clean.length > 0) out[key] = clean;
  }
  return out;
}

interface Suggestion {
  value: string;
  count?: number;
}

/** Text input with a filterable suggestion dropdown (keyboard navigable). */
function SuggestInput({
  value,
  onChange,
  suggestions,
  placeholder,
  ariaLabel,
  onEnter,
  inputRef,
}: {
  value: string;
  onChange: (value: string) => void;
  suggestions: Suggestion[];
  placeholder: string;
  ariaLabel: string;
  onEnter?: () => void;
  inputRef?: React.RefObject<HTMLInputElement | null>;
}) {
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const boxRef = useRef<HTMLDivElement>(null);
  const filtered = useMemo(() => {
    const q = value.trim().toLowerCase();
    const list = q
      ? suggestions.filter(
          (s) => s.value.toLowerCase().includes(q) && s.value.toLowerCase() !== q,
        )
      : suggestions;
    return list.slice(0, 8);
  }, [value, suggestions]);

  useEffect(() => {
    setHighlight(0);
  }, [filtered.length, value]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open ]);

  const pick = (suggestion: string) => {
    onChange(suggestion);
    setOpen(false);
    inputRef?.current?.focus();
  };

  return (
    <div ref={boxRef} className="relative flex-1">
      <Input
        ref={inputRef}
        aria-label={ariaLabel}
        placeholder={placeholder}
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" && filtered.length > 0) {
            e.preventDefault();
            setOpen(true);
            setHighlight((h) => (h + 1) % filtered.length);
          } else if (e.key === "ArrowUp" && filtered.length > 0) {
            e.preventDefault();
            setOpen(true);
            setHighlight((h) => (h - 1 + filtered.length) % filtered.length);
          } else if (e.key === "Enter") {
            if (open && filtered[highlight]) {
              e.preventDefault();
              pick(filtered[highlight].value);
            } else {
              onEnter?.();
            }
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
        className="h-9 text-sm font-mono"
        autoComplete="off"
        role="combobox"
        aria-expanded={open && filtered.length > 0}
        aria-autocomplete="list"
      />
      {open && filtered.length > 0 && (
        <div className="absolute left-0 right-0 top-full z-50 mt-1 overflow-hidden rounded-xl border border-border bg-popover shadow-xl">
          {filtered.map((s, i) => (
            <button
              key={s.value}
              type="button"
              onClick={() => pick(s.value)}
              onMouseEnter={() => setHighlight(i)}
              className={cn(
                "flex w-full items-center justify-between gap-2 px-3 py-2 text-left font-mono text-xs",
                i === highlight ? "bg-primary/10 text-primary" : "text-foreground",
              )}
              role="option"
              aria-selected={i === highlight}
            >
              <span className="truncate">{s.value}</span>
              {s.count !== undefined && (
                <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {s.count}
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

interface EntriesListProps {
  inbox?: boolean;
  collection?: SmartCollection | null;
  onInboxChange?: (value: boolean) => void;
  selectedCategory: string | null;
  selectedPath?: string | null;
  /** Provided by the browser so mobile can switch collections (the sidebar is desktop-only). */
  onSelectCategory?: (category: string | null) => void;
}

export function EntriesList({ selectedCategory, selectedPath, onSelectCategory, inbox, collection, onInboxChange }: EntriesListProps) {
  const [hiddenIds, setHiddenIds] = useState<Set<string>>(new Set());
  const onAssigned = (id: string, hide: boolean) => setHiddenIds(prev => { const next = new Set(prev); if (hide) next.add(id); else next.delete(id); return next; });
  const [collectionName, setCollectionName] = useState("");
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
  const [storedProps, setPropertyFilters] = useSessionState<Record<string, string[]>>(
    "entries-list:props",
    {},
  );
  // Multi-value per key: `tags=[work, home]` matches entries with EITHER.
  const propertyFilters = useMemo(() => normalizePropFilters(storedProps), [storedProps]);
  const propValueCount = useMemo(
    () => Object.values(propertyFilters).reduce((n, vs) => n + vs.length, 0),
    [propertyFilters],
  );
  const [pageSizeRaw, setPageSize] = useSessionState<number>("entries-list:page-size", 20);
  const pageSize = PAGE_SIZE_OPTIONS.includes(pageSizeRaw) ? pageSizeRaw : 20;
  const [filterKeyDraft, setFilterKeyDraft] = useState("");
  const [filterValueDraft, setFilterValueDraft] = useState("");
  const valueInputRef = useRef<HTMLInputElement>(null);
  const [datePreset, setDatePreset] = useSessionState<string>(
    "entries-list:date",
    "any",
  );
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [selectMode, setSelectMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [bulkArmed, setBulkArmed] = useState(false);
  const [bulkBusy, setBulkBusy] = useState(false);
  const { data: schemas } = useSchemas();
  const { data: categories } = useCategories();

  // Server-side search: debounce keystrokes so each pause issues one query
  // across the whole corpus (the old client-side filter only saw this page).
  const debouncedSearch = useDebouncedValue(localSearch.trim(), 300);
  const dateMin = useMemo(() => {
    const preset = DATE_PRESETS.find((p) => p.id === datePreset);
    if (!preset?.ms) return undefined;
    return Date.now() - preset.ms;
  }, [datePreset]);

  useEffect(() => {
    if (!collection) return;
    setSelectedType(collection.type); setPropertyFilters(normalizePropFilters(collection.metadata)); setLocalSearch(collection.search); setSortOrder(collection.sort); setPage(1);
  }, [collection, setSelectedType, setPropertyFilters, setLocalSearch, setSortOrder, setPage]);
  useEffect(() => { setPage(1); setHiddenIds(new Set()); }, [inbox, setPage]);
  const lastFilters = useRef<{
    category: string | null;
    path: string | null | undefined;
    sort: SortOrder;
    type: string | null;
    props: string;
    search: string;
    dateMin: number | undefined;
    pageSize: number;
  }>({
    category: selectedCategory,
    path: selectedPath,
    sort: sortOrder,
    type: selectedType,
    props: JSON.stringify(propertyFilters),
    search: debouncedSearch,
    dateMin,
    pageSize,
  });
  useEffect(() => {
    const propsKey = JSON.stringify(propertyFilters);
    const prev = lastFilters.current;
    if (
      prev.category !== selectedCategory ||
      prev.path !== selectedPath ||
      prev.sort !== sortOrder ||
      prev.type !== selectedType ||
      prev.props !== propsKey ||
      prev.search !== debouncedSearch ||
      prev.dateMin !== dateMin ||
      prev.pageSize !== pageSize
    ) {
      lastFilters.current = {
        category: selectedCategory,
        path: selectedPath,
        sort: sortOrder,
        type: selectedType,
        props: propsKey,
        search: debouncedSearch,
        dateMin,
        pageSize,
      };
      setPage(1);
      // The visible set changed — drop stale selection.
      setSelectedIds(new Set());
      setBulkArmed(false);
    }
  }, [selectedCategory, selectedPath, sortOrder, selectedType, propertyFilters, debouncedSearch, dateMin, pageSize, setPage]);

  // The API takes one comma-joined string per key; commas mean OR.
  // (A value containing a literal comma can't be filtered exactly —
  // rare enough to accept; noted in the popover below.)
  const metadataParam = useMemo(() => {
    const pairs = Object.entries(propertyFilters)
      .map(([key, values]) => [key, values.join(",")] as const)
      .filter(([, joined]) => joined.length > 0);
    return pairs.length > 0 ? Object.fromEntries(pairs) : undefined;
  }, [propertyFilters]);

  const { data: pagedData, isLoading } = useItems({
    needs_home: inbox || undefined,
    source_id: selectedCategory ?? undefined,
    path_prefix: selectedPath ?? undefined,
    limit: pageSize,
    offset: (page - 1) * pageSize,
    sort_order: sortOrder,
    type: selectedType ?? undefined,
    metadata: metadataParam,
    search: debouncedSearch || undefined,
    min_created_at: dateMin,
  });

  // Property-filter autocomplete: distinct keys + top values over the
  // current view (same scoping filters, so suggestions stay relevant).
  const { data: facetsData } = useMetadataFacets({
    source_id: selectedCategory ?? undefined,
    path_prefix: selectedPath ?? undefined,
    type: selectedType ?? undefined,
    needs_home: inbox || undefined,
    search: debouncedSearch || undefined,
  });
  const facets = useMemo(() => facetsData?.facets ?? [], [facetsData]);
  const keySuggestions: Suggestion[] = useMemo(
    () => facets.map((f) => ({ value: f.key, count: f.count })),
    [facets],
  );
  const valueSuggestions: Suggestion[] = useMemo(() => {
    const facet = facets.find((f) => f.key === filterKeyDraft.trim());
    if (!facet) return [];
    return facet.values.map((v) => ({ value: v.value, count: v.count }));
  }, [facets, filterKeyDraft]);

  const addPropertyFilter = () => {
    const key = filterKeyDraft.trim();
    const value = filterValueDraft.trim();
    if (!key || !value) return;
    setPropertyFilters((prev) => {
      const next = normalizePropFilters(prev);
      const existing = next[key] ?? [];
      if (!existing.includes(value)) next[key] = [...existing, value];
      return next;
    });
    setFilterValueDraft("");
  };

  const removePropertyFilter = (key: string, value?: string) => {
    setPropertyFilters((prev) => {
      const next = normalizePropFilters(prev);
      if (value === undefined) {
        delete next[key];
      } else {
        const rest = (next[key] ?? []).filter((v) => v !== value);
        if (rest.length === 0) delete next[key];
        else next[key] = rest;
      }
      return next;
    });
  };

  const activeFilterCount =
    propValueCount +
    (selectedType ? 1 : 0) +
    (debouncedSearch ? 1 : 0) +
    (dateMin ? 1 : 0);

  const entries = pagedData?.items?.filter(entry => !hiddenIds.has(entry.id));
  const totalCount = pagedData?.total_count ?? 0;
  const rangeStart = totalCount === 0 ? 0 : (page - 1) * pageSize + 1;
  const rangeEnd = Math.min(page * pageSize, totalCount);
  const clearAllFilters = () => {
    setSelectedType(null);
    setPropertyFilters({});
    setLocalSearch("");
    setDatePreset("any");
  };

  const { trigger: deleteItem } = useDeleteItem();
  const { trigger: bulkDelete } = useBulkDeleteItems();
  const { mutate } = useSWRConfig();

  // Filtering is server-side now (search/type/props/dates all shape the
  // query above) — the page contents are the visible set, no client filter.
  const filteredEntries = entries ?? [];

  const navigation = useEntryNavigation(filteredEntries);

  const totalPages = Math.ceil(totalCount / pageSize);

  const handleDelete = async (id: string) => {
    onAssigned(id, true);
    try { await deleteItem(id); toast.success("Entry deleted"); }
    catch { onAssigned(id, false); toast.error("Could not delete entry"); return; }
    void mutate(key => Array.isArray(key) && key[0] === "items");
    mutate("items");
    if (selectedCategory) {
      mutate(["items", selectedCategory]);
    }
    mutate("categories");
  };

  const refreshAfterDelete = () => {
    // Predicate match: list queries use array keys (["items", options],
    // ["entries-paths", sourceId]) that a plain string key would miss.
    mutate((key) =>
      Array.isArray(key) && (key[0] === "items" || key[0] === "entries-paths"),
    );
    mutate("categories");
  };

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    setBulkArmed(false);
  };

  const handleBulkDelete = async () => {
    if (!bulkArmed) {
      setBulkArmed(true);
      return;
    }
    const ids = [...selectedIds];
    if (ids.length === 0) return;
    setBulkBusy(true);
    try {
      let deleted = 0;
      let notFound = 0;
      // Server caps batches at 200 ids per request.
      for (let i = 0; i < ids.length; i += 200) {
        const res = await bulkDelete(ids.slice(i, i + 200));
        deleted += res.deleted.length;
        notFound += res.not_found.length;
      }
      refreshAfterDelete();
      setSelectedIds(new Set());
      setBulkArmed(false);
      setSelectMode(false);
      if (notFound > 0) {
        toast.success(`Deleted ${deleted} entries (${notFound} already gone)`);
      } else {
        toast.success(`Deleted ${deleted} ${deleted === 1 ? "entry" : "entries"}`);
      }
    } catch {
      toast.error("Bulk delete failed");
    } finally {
      setBulkBusy(false);
    }
  };

  const hasActiveFilters = activeFilterCount > 0;

  const filterBar = (
    <div className="space-y-3">
      {/* Mobile collection picker — the sidebar is desktop-only. */}
      {onSelectCategory && (
        <div className="md:hidden">
          <Select
            value={selectedCategory ?? "__all__"}
            onValueChange={(value) =>
              onSelectCategory(value === "__all__" ? null : value)
            }
          >
            <SelectTrigger className="h-12 w-full rounded-2xl border-muted px-5 font-semibold text-xs uppercase tracking-wider">
              <SelectValue placeholder="All collections" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">All Intelligence</SelectItem>
              {categories?.map((c) => (
                <SelectItem key={c.id} value={c.id}>
                  {c.name} ({c.count})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Button variant={inbox ? "secondary" : "outline"} onClick={() => onInboxChange?.(!inbox)}>{inbox ? "Needs a home · On" : "Needs a home"}</Button>
        <Input aria-label="Collection name" placeholder="Name this filter set" value={collectionName} onChange={e => setCollectionName(e.target.value)} className="w-48" />
        <Button variant="outline" disabled={!collectionName.trim()} onClick={() => {
          try { saveCollections([...getCollections(), { id: crypto.randomUUID(), inbox, name: collectionName.trim(), category: selectedCategory, path: selectedPath ?? null, type: selectedType, metadata: propertyFilters, search: localSearch, sort: sortOrder }]); setCollectionName(""); toast.success("Collection pinned to sidebar"); } catch { toast.error("Could not save collection"); }
        }}>Pin search</Button>
      </div>
      {/* Filters and Search Bar */}
      <div className="flex flex-col sm:flex-row sm:items-center gap-3">
        <div className="relative flex-1 group">
          <Search className="absolute left-4 top-1/2 mt-0.5 size-4 -translate-y-1/2 text-muted-foreground/60 transition-colors group-focus-within:text-primary" />
          <input
            type="text"
            placeholder="Search all entries..."
            aria-label="Search all entries"
            className="w-full h-12 bg-muted/20 border-muted/40 rounded-2xl pl-12 pr-20 sm:pr-10 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all hover:bg-muted/30"
            value={localSearch}
            onChange={(e) => setLocalSearch(e.target.value)}
          />
          {localSearch && (
            <button
              type="button"
              onClick={() => setLocalSearch("")}
              aria-label="Clear search"
              className="absolute right-10 top-1/2 -translate-y-1/2 rounded-full p-1.5 text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground sm:right-3"
            >
              <X className="size-4" />
            </button>
          )}

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

            <Select value={datePreset} onValueChange={setDatePreset}>
              <SelectTrigger
                aria-label="Filter by date"
                className={cn(
                  // NOTE: `h-12!` (important) — SelectTrigger's own
                  // `data-[size=default]:h-9` outranks a plain `h-12`.
                  "h-12! rounded-2xl border-muted px-5 w-full sm:w-auto",
                  dateMin && "border-primary/40 text-primary bg-primary/5",
                )}
              >
                <SelectValue placeholder="Any time" />
              </SelectTrigger>
              <SelectContent>
                {DATE_PRESETS.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select
              value={selectedType ?? "__all__"}
              onValueChange={(value) =>
                setSelectedType(value === "__all__" ? null : value)
              }
            >
              <SelectTrigger className="h-12! rounded-2xl border-muted px-5 w-full sm:w-[180px]">
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

            <Button
              variant="outline"
              onClick={() => {
                setSelectMode((v) => !v);
                setSelectedIds(new Set());
                setBulkArmed(false);
              }}
              className={cn(
                "h-12 rounded-2xl border-muted px-5 shrink-0",
                selectMode && "border-primary/40 text-primary bg-primary/5",
              )}
              title="Select multiple entries for bulk delete"
            >
              <Checkbox
                checked={selectMode && selectedIds.size > 0}
                tabIndex={-1}
                className="mr-2 pointer-events-none"
              />
              <span className="font-semibold text-xs uppercase tracking-wider">
                Select
                {selectMode && selectedIds.size > 0 && ` (${selectedIds.size})`}
              </span>
            </Button>

            <Popover open={filtersOpen} onOpenChange={setFiltersOpen}>
              <PopoverTrigger asChild>
                <Button
                  variant="outline"
                  className={cn(
                    "h-12 rounded-2xl border-muted px-5 shrink-0",
                    propValueCount > 0 &&
                      "border-primary/40 text-primary bg-primary/5",
                  )}
                >
                  <Filter className="mr-2 size-4 opacity-60" />
                  <span className="font-semibold text-xs uppercase tracking-wider">
                    Properties
                    {propValueCount > 0 && ` (${propValueCount})`}
                  </span>
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" className="w-80 rounded-2xl p-4 space-y-4">
                <div className="space-y-1.5">
                  <p className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    Filter by property
                  </p>
                  <p className="text-[11px] text-muted-foreground/70 leading-relaxed">
                    Exact match on a metadata field — add several values per key
                    to match ANY of them. Start typing for suggestions
                    {facetsData ? (
                      <>
                        {" "}— {facets.length} keys across {facetsData.scanned} entries
                        {facetsData.truncated ? " (sampled)" : ""}.
                      </>
                    ) : (
                      " (loading suggestions...)."
                    )}
                  </p>
                </div>
                <div className="flex gap-2">
                  <SuggestInput
                    value={filterKeyDraft}
                    onChange={setFilterKeyDraft}
                    suggestions={keySuggestions}
                    placeholder="key"
                    ariaLabel="Property key"
                    onEnter={() => valueInputRef.current?.focus()}
                  />
                  <SuggestInput
                    value={filterValueDraft}
                    onChange={setFilterValueDraft}
                    suggestions={valueSuggestions}
                    placeholder="value"
                    ariaLabel="Property value"
                    onEnter={addPropertyFilter}
                    inputRef={valueInputRef}
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
                {propValueCount > 0 && (
                  <div className="flex flex-wrap gap-1.5 pt-1">
                    {Object.entries(propertyFilters).flatMap(([key, values]) =>
                      values.map((value) => (
                        <Badge
                          key={`${key}=${value}`}
                          variant="secondary"
                          className="gap-1.5 rounded-full pl-2.5 pr-1 py-1 text-[11px] font-mono"
                        >
                          {key}={value}
                          <button
                            type="button"
                            onClick={() => removePropertyFilter(key, value)}
                            className="rounded-full p-0.5 hover:bg-muted-foreground/20"
                            aria-label={`Remove filter ${key}=${value}`}
                          >
                            <X className="size-3" />
                          </button>
                        </Badge>
                      )),
                    )}
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
            {debouncedSearch && (
              <Badge
                variant="outline"
                className="gap-1.5 rounded-full pl-2.5 pr-1 py-1 text-[11px] border-primary/30 text-primary bg-primary/5"
              >
                search: “{debouncedSearch.length > 24 ? `${debouncedSearch.slice(0, 24)}…` : debouncedSearch}”
                <button
                  type="button"
                  onClick={() => setLocalSearch("")}
                  className="rounded-full p-0.5 hover:bg-primary/20"
                  aria-label="Clear search"
                >
                  <X className="size-3" />
                </button>
              </Badge>
            )}
            {dateMin ? (
              <Badge
                variant="outline"
                className="gap-1.5 rounded-full pl-2.5 pr-1 py-1 text-[11px] font-mono border-primary/30 text-primary bg-primary/5"
              >
                {DATE_PRESETS.find((p) => p.id === datePreset)?.label ?? datePreset}
                <button
                  type="button"
                  onClick={() => setDatePreset("any")}
                  className="rounded-full p-0.5 hover:bg-primary/20"
                  aria-label="Clear date filter"
                >
                  <X className="size-3" />
                </button>
              </Badge>
            ) : null}
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
            {Object.entries(propertyFilters).flatMap(([key, values]) =>
              values.map((value) => (
                <Badge
                  key={`${key}=${value}`}
                  variant="outline"
                  className="gap-1.5 rounded-full pl-2.5 pr-1 py-1 text-[11px] font-mono border-primary/30 text-primary bg-primary/5"
                >
                  {key}={value}
                  <button
                    type="button"
                    onClick={() => removePropertyFilter(key, value)}
                    className="rounded-full p-0.5 hover:bg-primary/20"
                    aria-label={`Remove filter ${key}=${value}`}
                  >
                    <X className="size-3" />
                  </button>
                </Badge>
              )),
            )}
            <button
              type="button"
              onClick={clearAllFilters}
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
        <ListSkeleton count={5} />
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
          <div className="flex flex-wrap items-center justify-center gap-3">
            {hasActiveFilters && (
              <Button
                size="lg"
                variant="outline"
                className="h-12 rounded-2xl px-8 transition-all hover:scale-105 active:scale-95"
                onClick={clearAllFilters}
              >
                <X className="mr-2 size-5" />
                Clear search & filters
              </Button>
            )}
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

        {/* Select-all row (multiselect mode) */}
        {selectMode && filteredEntries.length > 0 && (() => {
          const pageIds = filteredEntries.map((e) => e.id);
          const allOnPage = pageIds.every((id) => selectedIds.has(id));
          return (
            <div className="flex items-center gap-3 rounded-2xl border border-border/60 bg-muted/20 px-4 py-2.5">
              <span
                role="checkbox"
                aria-checked={allOnPage}
                aria-label={allOnPage ? "Deselect all on this page" : "Select all on this page"}
                onClick={() => {
                  setSelectedIds((prev) => {
                    const next = new Set(prev);
                    if (allOnPage) pageIds.forEach((id) => next.delete(id));
                    else pageIds.forEach((id) => next.add(id));
                    return next;
                  });
                  setBulkArmed(false);
                }}
                className="flex cursor-pointer items-center gap-2.5"
              >
                <Checkbox checked={allOnPage} tabIndex={-1} className="pointer-events-none" />
                <span className="text-xs font-semibold text-muted-foreground">
                  {allOnPage ? "Deselect page" : "Select page"}
                </span>
              </span>
              <span className="ml-auto font-mono text-[10px] uppercase tracking-widest text-muted-foreground/60">
                {selectedIds.size > 0
                  ? `${selectedIds.size} selected`
                  : `${filteredEntries.length} on this page`}
              </span>
            </div>
          );
        })()}

        {/* List of Cards */}
        <div {...navigation} className="flex flex-col">
          {filteredEntries.map((entry, index) => (
            <div key={entry.id} data-entry-row tabIndex={-1} className="rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/40">
            <EntryCard
              entry={entry}
              index={index}
              onDelete={selectMode ? undefined : handleDelete}
              selectable={selectMode}
              selected={selectedIds.has(entry.id)}
              onToggleSelect={toggleSelect}
            />
            {inbox && <InboxAssignment entry={entry} onAssigned={onAssigned} />}
            </div>
          ))}
        </div>

        {totalPages > 1 && (
          <div className="flex flex-col sm:flex-row items-center justify-between gap-4 pt-6 border-t border-muted/20">
            <div className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
              Page {page} of {totalPages} — showing {rangeStart}–{rangeEnd} of {totalCount}
            </div>
            <div className="flex items-center gap-2">
              <Select
                value={String(pageSize)}
                onValueChange={(v) => setPageSize(Number(v))}
              >
                <SelectTrigger
                  aria-label="Entries per page"
                  className="h-10! rounded-xl border-muted px-3 font-mono text-[11px] text-muted-foreground w-auto gap-1"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {PAGE_SIZE_OPTIONS.map((n) => (
                    <SelectItem key={n} value={String(n)}>
                      {n} / page
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
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
            Neural manifold view — showing {rangeStart}–{rangeEnd} of {totalCount}
            {hasActiveFilters ? " · filtered" : ""}
          </p>
        </div>
      </div>

      {/* Bulk action bar */}
      {selectMode && selectedIds.size > 0 && (
        <div className="fixed bottom-6 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3 rounded-2xl border border-border bg-card/95 px-4 py-3 shadow-2xl backdrop-blur-md animate-in slide-in-from-bottom-4 duration-300">
          <span className="font-mono text-[11px] font-bold uppercase tracking-widest text-muted-foreground whitespace-nowrap">
            {selectedIds.size} selected
          </span>
          <Button
            variant="ghost"
            size="sm"
            className="h-9 rounded-xl font-mono text-[11px] uppercase tracking-wider"
            onClick={() => {
              setSelectedIds(new Set());
              setBulkArmed(false);
            }}
          >
            Clear
          </Button>
          <Button
            variant={bulkArmed ? "destructive" : "outline"}
            size="sm"
            className="h-9 rounded-xl font-mono text-[11px] uppercase tracking-wider gap-2"
            onClick={handleBulkDelete}
            disabled={bulkBusy}
          >
            <Trash2 className="size-3.5" />
            {bulkBusy
              ? "Deleting…"
              : bulkArmed
                ? `Confirm delete ${selectedIds.size}?`
                : `Delete ${selectedIds.size}`}
          </Button>
          {bulkArmed && !bulkBusy && (
            <button
              type="button"
              onClick={() => setBulkArmed(false)}
              className="text-muted-foreground hover:text-foreground transition-colors"
              aria-label="Cancel bulk delete"
            >
              <X className="size-4" />
            </button>
          )}
        </div>
      )}

      {/* Mobile FAB (hidden in multiselect mode — the bulk bar owns the bottom) */}
      {!selectMode && (
      <Button
        size="icon"
        className="fixed bottom-6 right-6 size-14 rounded-full shadow-2xl shadow-primary/40 z-50 animate-in zoom-in duration-300"
        asChild
      >
        <Link href="/entries/new">
          <Plus className="size-6" />
        </Link>
      </Button>
      )}
    </>
  );
}
