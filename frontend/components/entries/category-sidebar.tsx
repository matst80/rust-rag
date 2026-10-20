"use client";

import { getCollections, saveCollections, type SmartCollection } from "@/lib/smart-collections";
import { ChevronDown, Eye, EyeOff, FolderOpen, FolderTree, Inbox, Layers } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { useCategories, useEntriesTree } from "@/lib/api";
import { useEffect, useMemo, useState } from "react";
import { WikiTree, ancestorChain } from "./wiki-tree";
import {
  getHiddenSources,
  getShowHiddenSources,
  setHiddenSources,
  setShowHiddenSources,
  splitSources,
} from "@/lib/sources";

interface CategorySidebarProps {
  inbox?: boolean;
  onInbox?: () => void;
  onCollection?: (value: SmartCollection) => void;
  selectedCategory: string | null;
  onSelectCategory: (category: string | null) => void;
  selectedPath: string | null;
  onSelectPath: (path: string | null) => void;
}

export function CategorySidebar({
  inbox, onInbox, onCollection,
  selectedCategory,
  onSelectCategory,
  selectedPath,
  onSelectPath,
}: CategorySidebarProps) {
  const [collections, setCollections] = useState<SmartCollection[]>([]);
  useEffect(() => {
    const refresh = () => setCollections(getCollections()); refresh();
    window.addEventListener("collections-changed", refresh);
    window.addEventListener("storage", refresh);
    return () => { window.removeEventListener("collections-changed", refresh); window.removeEventListener("storage", refresh); };
  }, []);
  const { data: categories, isLoading } = useCategories();
  const [expanded, setExpanded] = useState<Set<string>>(() =>
    ancestorChain(selectedPath ?? "")
  );
  const { data: pathRoot } = useEntriesTree(selectedCategory, undefined);
  const [userHidden, setUserHidden] = useState<string[]>([]);
  const [showHidden, setShowHidden] = useState(false);
  const [hiddenOpen, setHiddenOpen] = useState(false);

  useEffect(() => {
    setUserHidden(getHiddenSources());
    setShowHidden(getShowHiddenSources());
  }, []);

  const togglePath = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };
  const [filterText, setFilterText] = useState("");
  const filteredCategories = useMemo(
    () =>
      categories?.filter((category) =>
        filterText
          ? category.name.toLowerCase().includes(filterText.toLowerCase())
          : true,
      ),
    [categories, filterText],
  );

  const { visible, hidden } = useMemo(
    () => splitSources(filteredCategories, userHidden),
    [filteredCategories, userHidden],
  );

  const hideCategory = (id: string) => {
    const next = [...new Set([...userHidden, id])];
    setUserHidden(next);
    setHiddenSources(next);
    if (selectedCategory === id) {
      onSelectCategory(null);
      onSelectPath(null);
    }
  };

  const unhideCategory = (id: string) => {
    const next = userHidden.filter((h) => h !== id);
    setUserHidden(next);
    setHiddenSources(next);
  };

  const toggleShowHidden = () => {
    const next = !showHidden;
    setShowHidden(next);
    setShowHiddenSources(next);
    if (next) setHiddenOpen(true);
  };

  const renderRow = (category: { id: string; name: string; count: number }, isHiddenRow = false) => (
    <div key={category.id} className="group/row flex items-center gap-1">
      <Button
        variant="ghost"
        size="sm"
        className={cn(
          "h-12 flex-1 justify-start gap-4 rounded-2xl px-4 transition-all duration-300 min-w-0",
          selectedCategory === category.id
            ? "bg-primary/5 text-primary shadow-sm ring-1 ring-primary/10"
            : "text-muted-foreground hover:bg-muted font-medium",
        )}
        onClick={() => {
          onSelectCategory(category.id);
          onSelectPath(null);
        }}
        title={isHiddenRow ? "Hidden collection — click to view anyway" : category.name}
      >
        <div
          className={cn(
            "flex size-8 items-center justify-center rounded-xl transition-all duration-300 shrink-0",
            selectedCategory === category.id
              ? "bg-primary/10 text-primary"
              : "bg-muted group-hover:bg-primary/10 group-hover:text-primary",
          )}
        >
          <FolderOpen className="size-4" />
        </div>
        <span
          className={cn(
            "flex-1 text-left text-sm truncate",
            selectedCategory === category.id
              ? "font-bold tracking-tight"
              : "opacity-70",
          )}
        >
          {category.name}
        </span>
        <Badge
          variant="outline"
          className={cn(
            "ml-auto px-2 py-0 text-[10px] font-bold border-none transition-colors shrink-0",
            selectedCategory === category.id
              ? "bg-primary/10 text-primary"
              : "bg-muted/40 text-muted-foreground",
          )}
        >
          {category.count}
        </Badge>
      </Button>
      <button
        type="button"
        onClick={() => (isHiddenRow ? unhideCategory(category.id) : hideCategory(category.id))}
        title={isHiddenRow ? `Unhide ${category.name}` : `Hide ${category.name}`}
        className="size-7 shrink-0 items-center justify-center rounded-lg text-muted-foreground/40 opacity-0 transition-all hover:bg-muted hover:text-primary focus-visible:opacity-100 group-hover/row:opacity-100 flex"
      >
        {isHiddenRow ? <Eye className="size-3.5" /> : <EyeOff className="size-3.5" />}
      </button>
    </div>
  );

  return (
    <aside className="hidden md:flex w-full flex-col bg-muted/5 p-4 md:p-6 md:w-80 md:min-h-[calc(100vh-3.5rem)] border-r border-muted/20 backdrop-blur-sm">
      <div className="mb-4 md:mb-8 space-y-4">
        <h2 className="flex items-center gap-2.5 px-3 text-[11px] font-bold uppercase tracking-[0.2em] text-primary/60">
          <Layers className="size-4" />
          Knowledge Collections
        </h2>

        <div className="relative px-3">
          <div className="absolute inset-y-0 left-6 flex items-center pointer-events-none">
            <span className="text-muted-foreground/40 font-bold text-xs uppercase">
              Find
            </span>
          </div>
          <input
            type="text"
            value={filterText}
            onChange={(e) => setFilterText(e.target.value)}
            placeholder="collection..."
            className="w-full bg-muted/20 border border-muted-foreground/10 rounded-xl py-2 pl-14 pr-4 text-xs font-semibold focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all placeholder:text-muted-foreground/30 placeholder:uppercase placeholder:font-bold"
          />
        </div>
      </div>

      <nav className="flex flex-row gap-2 overflow-x-auto pb-4 md:flex-col md:overflow-x-visible md:pb-0">
        <Button
          variant="ghost"
          size="sm"
          className={cn(
            "h-12 justify-start gap-3.5 rounded-2xl px-4 py-6 transition-all duration-300",
            selectedCategory === null
              ? "bg-primary/5 text-primary shadow-sm ring-1 ring-primary/20"
              : "text-muted-foreground hover:bg-muted font-medium",
          )}
          onClick={() => {
            onSelectCategory(null);
            onSelectPath(null);
          }}
        >
          <div
            className={cn(
              "flex size-8 items-center justify-center rounded-xl transition-colors",
              selectedCategory === null
                ? "bg-primary text-primary-foreground shadow-lg shadow-primary/20"
                : "bg-muted text-muted-foreground",
            )}
          >
            <FolderOpen className="size-4" />
          </div>
          <span
            className={cn(
              "text-sm",
              selectedCategory === null
                ? "font-bold tracking-tight"
                : "opacity-80",
            )}
          >
            All Intelligence
          </span>
        </Button>

        <Button
          variant="ghost"
          size="sm"
          className={cn(
            "h-12 w-full justify-start gap-4 rounded-2xl px-4 transition-all duration-300",
            inbox
              ? "bg-primary/5 text-primary shadow-sm ring-1 ring-primary/10"
              : "text-muted-foreground hover:bg-muted font-medium",
          )}
          onClick={onInbox}
          title="Entries without a path, tags, or links"
        >
          <div
            className={cn(
              "flex size-8 items-center justify-center rounded-xl transition-all duration-300 shrink-0",
              inbox
                ? "bg-primary/10 text-primary"
                : "bg-muted group-hover:bg-primary/10 group-hover:text-primary",
            )}
          >
            <Inbox className="size-4" />
          </div>
          <span
            className={cn(
              "flex-1 text-left text-sm truncate",
              inbox ? "font-bold tracking-tight" : "opacity-70",
            )}
          >
            Needs a home
          </span>
        </Button>
        {collections.map(collection => <div key={collection.id} className="flex items-center gap-1"><Button variant="ghost" className="min-w-0 flex-1 justify-start truncate" onClick={() => onCollection?.(collection)}>{collection.name}</Button><button aria-label={`Remove ${collection.name}`} className="p-2 text-muted-foreground" onClick={() => saveCollections(collections.filter(c => c.id !== collection.id))}>×</button></div>)}
        <div className="my-4 h-px bg-muted/30 mx-3 hidden md:block" />

        {isLoading ? (
          <div className="flex flex-col gap-2 mt-2">
            {[1, 2, 3].map((i) => (
              <div
                key={i}
                className="h-12 w-full animate-pulse rounded-2xl bg-muted/40"
              />
            ))}
          </div>
        ) : (
          <>
            {visible.map((category) => renderRow(category))}

            {hidden.length > 0 && (
              <div className="mt-2 rounded-2xl border border-dashed border-muted-foreground/20">
                <button
                  type="button"
                  onClick={() => (showHidden ? setHiddenOpen((v) => !v) : toggleShowHidden())}
                  className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-[11px] font-bold uppercase tracking-[0.15em] text-muted-foreground/70 hover:text-primary transition-colors"
                  title={showHidden ? "Collapse hidden collections" : `Show ${hidden.length} hidden collections (memories, todos)`}
                >
                  <EyeOff className="size-3.5" />
                  <span className="flex-1">
                    Hidden · {hidden.length}
                  </span>
                  <ChevronDown
                    className={cn("size-3.5 transition-transform", hiddenOpen && showHidden && "rotate-180")}
                  />
                </button>
                {showHidden && hiddenOpen && (
                  <div className="flex flex-col gap-1 px-1 pb-2">
                    <p className="px-3 pb-1 text-[11px] leading-snug text-muted-foreground/70">
                      Agent memories &amp; todos live here. They stay out of the way until you need them.
                    </p>
                    {hidden.map((category) => renderRow(category, true))}
                    <button
                      type="button"
                      onClick={toggleShowHidden}
                      className="mx-3 mt-1 flex items-center justify-center gap-1.5 rounded-lg py-1.5 text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 hover:text-primary transition-colors"
                    >
                      <Eye className="size-3" /> Hide again
                    </button>
                  </div>
                )}
                {!showHidden && (
                  <button
                    type="button"
                    onClick={toggleShowHidden}
                    className="mx-3 mb-2 flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 hover:text-primary transition-colors"
                  >
                    <Eye className="size-3" /> Show memories &amp; hidden
                  </button>
                )}
              </div>
            )}
          </>
        )}
      </nav>

      {selectedCategory && (
        <div className="hidden md:flex flex-col mt-6 pt-4 border-t border-muted/20">
          <h3 className="flex items-center gap-2 px-3 mb-2 text-[11px] font-bold uppercase tracking-[0.2em] text-primary/60">
            <FolderTree className="size-3.5" />
            Wiki Path
          </h3>
          <div className="flex flex-col overflow-y-auto max-h-96">
            <button
              type="button"
              onClick={() => onSelectPath(null)}
              className={cn(
                "flex items-center gap-2 rounded-md mx-1 px-2 py-1.5 font-mono text-sm transition-colors",
                !selectedPath
                  ? "bg-primary/10 text-primary font-semibold"
                  : "text-muted-foreground hover:bg-muted/50"
              )}
            >
              <span className="truncate">{selectedCategory} (root)</span>
            </button>
            {pathRoot && pathRoot.children.length === 0 && (
              <p className="px-4 py-3 text-xs text-muted-foreground">
                No wiki folders in this collection yet.
              </p>
            )}
            {pathRoot && (
              <WikiTree
                sourceId={selectedCategory}
                prefix=""
                depth={0}
                value={selectedPath}
                expanded={expanded}
                onToggle={togglePath}
                onSelect={onSelectPath}
              >
                {pathRoot.children}
              </WikiTree>
            )}
          </div>
        </div>
      )}
    </aside>
  );
}
