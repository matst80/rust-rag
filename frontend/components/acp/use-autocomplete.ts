"use client";

import { useState, useMemo, useEffect, useCallback, useRef } from "react";

interface AutocompleteProps {
  draft: string;
  availableCommands: Array<{ name: string; description?: string }>;
  onSelect: (cmdName: string) => void;
  listDirectories: (query: string) => void;
  findFiles: (query: string) => void;
  suggestions: { query: string; directories?: string[]; files?: string[] } | null;
}

export function useAutocomplete({
  draft,
  availableCommands,
  onSelect,
  listDirectories,
  findFiles,
  suggestions
}: AutocompleteProps) {
  const [acOpen, setAcOpen] = useState(false);
  const [acIndex, setAcIndex] = useState(0);
  const acListRef = useRef<HTMLUListElement>(null);

  const { query: acQuery, type: acType } = useMemo(() => {
    if (draft.startsWith("/")) {
      const firstSpace = draft.indexOf(" ");
      if (firstSpace === -1) {
        return { query: draft.slice(1).toLowerCase(), type: "command" as const };
      }
    }

    // Check if we are typing a path (contains / or starts with . or ~)
    const lastWord = draft.split(/\s+/).pop() || "";
    if (lastWord.includes("/") || lastWord.startsWith(".") || lastWord.startsWith("~")) {
      return { query: lastWord, type: "path" as const };
    }

    return { query: null, type: null };
  }, [draft]);

  const filteredCommands = useMemo(() => {
    if (acType !== "command" || !acQuery) return [];
    return availableCommands.filter((c) =>
      c.name.toLowerCase().startsWith(acQuery)
    );
  }, [availableCommands, acQuery, acType]);

  const filteredSuggestions = useMemo(() => {
    if (acType !== "path" || !acQuery || !suggestions || suggestions.query !== acQuery) return [];
    const dirs = suggestions.directories?.map(d => ({ name: d, type: "dir" as const })) || [];
    const files = suggestions.files?.map(f => ({ name: f, type: "file" as const })) || [];
    return [...dirs, ...files];
  }, [acQuery, acType, suggestions]);

  const items = useMemo(() => {
    if (acType === "command") return filteredCommands.map(c => ({ name: c.name, description: c.description, type: "command" as const }));
    if (acType === "path") return filteredSuggestions;
    return [];
  }, [acType, filteredCommands, filteredSuggestions]);

  useEffect(() => {
    if (acType === "path" && acQuery) {
      if (acQuery.endsWith("/")) {
        listDirectories(acQuery);
      } else {
        findFiles(acQuery);
      }
    }
  }, [acQuery, acType, listDirectories, findFiles]);

  useEffect(() => {
    if (items.length > 0) {
      setAcOpen(true);
      setAcIndex(0);
    } else {
      setAcOpen(false);
    }
  }, [items.length]);

  // Ensure active autocomplete item is visible
  useEffect(() => {
    if (acOpen && acListRef.current) {
      const activeItem = acListRef.current.children[acIndex] as HTMLElement;
      if (activeItem) {
        activeItem.scrollIntoView({
          block: "nearest",
        });
      }
    }
  }, [acIndex, acOpen]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!acOpen || items.length === 0) return false;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setAcIndex((prev) => (prev + 1) % items.length);
      return true;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setAcIndex((prev) => (prev - 1 + items.length) % items.length);
      return true;
    } else if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      const item = items[acIndex];
      if (item.type === "command") {
        onSelect(item.name);
      } else {
        // For paths, replace the last word
        const words = draft.split(/\s+/);
        words[words.length - 1] = item.name;
        onSelect(words.join(" "));
      }
      return true;
    } else if (e.key === "Escape") {
      setAcOpen(false);
      return true;
    }
    return false;
  }, [acOpen, items, acIndex, onSelect, draft]);

  return {
    acOpen,
    setAcOpen,
    acIndex,
    setAcIndex,
    items,
    acListRef,
    handleKeyDown,
  };
}
