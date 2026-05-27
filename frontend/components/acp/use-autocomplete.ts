"use client";

import { useState, useMemo, useEffect, useCallback, useRef } from "react";

interface AutocompleteProps {
  draft: string;
  availableCommands: Array<{ name: string; description?: string }>;
  onSelect: (cmdName: string) => void;
}

export function useAutocomplete({ draft, availableCommands, onSelect }: AutocompleteProps) {
  const [acOpen, setAcOpen] = useState(false);
  const [acIndex, setAcIndex] = useState(0);
  const acListRef = useRef<HTMLUListElement>(null);

  const acQuery = useMemo(() => {
    if (!draft.startsWith("/")) return null;
    const firstSpace = draft.indexOf(" ");
    if (firstSpace !== -1) return null;
    return draft.slice(1).toLowerCase();
  }, [draft]);

  const filteredCommands = useMemo(() => {
    if (acQuery === null) return [];
    return availableCommands.filter((c) =>
      c.name.toLowerCase().startsWith(acQuery)
    );
  }, [availableCommands, acQuery]);

  useEffect(() => {
    if (filteredCommands.length > 0) {
      setAcOpen(true);
      setAcIndex(0);
    } else {
      setAcOpen(false);
    }
  }, [filteredCommands.length]);

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
    if (!acOpen || filteredCommands.length === 0) return false;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setAcIndex((prev) => (prev + 1) % filteredCommands.length);
      return true;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setAcIndex((prev) => (prev - 1 + filteredCommands.length) % filteredCommands.length);
      return true;
    } else if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      onSelect(filteredCommands[acIndex].name);
      return true;
    } else if (e.key === "Escape") {
      setAcOpen(false);
      return true;
    }
    return false;
  }, [acOpen, filteredCommands, acIndex, onSelect]);

  return {
    acOpen,
    setAcOpen,
    acIndex,
    setAcIndex,
    filteredCommands,
    acListRef,
    handleKeyDown,
  };
}
