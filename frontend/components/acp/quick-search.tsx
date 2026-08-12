"use client";

import React, { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { Search, File, Folder, X, Command } from "lucide-react";
import { cn } from "@/lib/utils";

interface QuickSearchProps {
  onSearch: (query: string) => void;
  suggestions: { query: string; directories?: string[]; files?: string[] } | null;
  onSelect: (path: string, type: "file" | "dir") => void;
  onClose: () => void;
  placeholder?: string;
}

export function QuickSearch({
  onSearch,
  suggestions,
  onSelect,
  onClose,
  placeholder = "Search files and directories...",
}: QuickSearchProps) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const items = React.useMemo(() => {
    if (!suggestions || suggestions.query !== query) return [];
    const dirs = (suggestions.directories || []).map((d) => ({ path: d, type: "dir" as const }));
    const files = (suggestions.files || []).map((f) => ({ path: f, type: "file" as const }));
    return [...dirs, ...files];
  }, [suggestions, query]);

  useEffect(() => {
    const timeout = setTimeout(() => {
      onSearch(query);
    }, 150);
    return () => clearTimeout(timeout);
  }, [query, onSearch]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % Math.max(1, items.length));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + items.length) % Math.max(1, items.length));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = items[selectedIndex];
      if (item) {
        onSelect(item.path, item.type);
      }
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh] bg-background/40 backdrop-blur-sm p-4">
      <div 
        className="w-full max-w-2xl bg-card border border-border shadow-2xl rounded-xl overflow-hidden flex flex-col animate-in fade-in zoom-in-95 duration-200"
        onKeyDown={handleKeyDown}
      >
        <div className="flex items-center px-4 py-3 border-b border-border">
          <Search className="size-5 text-muted-foreground mr-3" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
            placeholder={placeholder}
            className="flex-1 bg-transparent border-none outline-none text-sm placeholder:text-muted-foreground"
            autoComplete="off"
          />
          <div className="flex items-center gap-1.5 ml-2">
            <kbd className="hidden sm:inline-flex h-5 select-none items-center gap-1 rounded border border-border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground opacity-100">
              <span className="text-xs">ESC</span>
            </kbd>
            <button onClick={onClose} className="p-1 hover:bg-muted rounded-md text-muted-foreground">
              <X className="size-4" />
            </button>
          </div>
        </div>

        <div className="max-h-[60vh] overflow-y-auto no-scrollbar">
          {items.length > 0 ? (
            <ul className="py-2">
              {items.map((item, i) => (
                <li key={item.path}>
                  <button
                    onClick={() => onSelect(item.path, item.type)}
                    onMouseEnter={() => setSelectedIndex(i)}
                    className={cn(
                      "w-full flex items-center px-4 py-2.5 text-left transition-colors",
                      i === selectedIndex ? "bg-primary text-primary-foreground" : "hover:bg-muted/50"
                    )}
                  >
                    {item.type === "dir" ? (
                      <Folder className={cn("size-4 mr-3", i === selectedIndex ? "text-primary-foreground" : "text-blue-500")} />
                    ) : (
                      <File className={cn("size-4 mr-3", i === selectedIndex ? "text-primary-foreground" : "text-muted-foreground")} />
                    )}
                    <div className="flex flex-col min-w-0">
                      <span className="text-sm font-medium truncate">{item.path.split("/").pop()}</span>
                      <span className={cn(
                        "text-[10px] truncate font-mono",
                        i === selectedIndex ? "text-primary-foreground/70" : "text-muted-foreground"
                      )}>
                        {item.path}
                      </span>
                    </div>
                    {i === selectedIndex && (
                      <span className="ml-auto text-[10px] font-mono opacity-60">ENTER</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          ) : query.length > 0 ? (
            <div className="py-12 text-center text-muted-foreground">
              <p className="text-sm">No results found for "{query}"</p>
            </div>
          ) : (
            <div className="py-12 text-center text-muted-foreground">
              <div className="flex justify-center mb-3">
                <Command className="size-8 opacity-20" />
              </div>
              <p className="text-sm">Type to search files and directories...</p>
              <p className="text-[10px] mt-1 opacity-60 italic">Searching in the daemon's current environment</p>
            </div>
          )}
        </div>

        <div className="px-4 py-2 bg-muted/30 border-t border-border flex items-center justify-between text-[10px] text-muted-foreground font-medium uppercase tracking-wider">
          <div className="flex items-center gap-4">
            <span className="flex items-center gap-1"><kbd className="bg-muted px-1 rounded border border-border">↑↓</kbd> Navigate</span>
            <span className="flex items-center gap-1"><kbd className="bg-muted px-1 rounded border border-border">ENTER</kbd> Select</span>
          </div>
          {items.length > 0 && <span>{items.length} results</span>}
        </div>
      </div>
    </div>
  );
}
