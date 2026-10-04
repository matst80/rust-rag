"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import {
  Code2,
  ExternalLink,
  FileCode,
  LoaderCircle,
  ScanSearch,
  Search,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { searchCode, type CodeSearchHit, type Entry } from "@/lib/api";
import { cn, entryTitle } from "@/lib/utils";
import { ScoreBadge } from "@/components/code/code-symbol-graph";

function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

/**
 * Inline code search for an entry: scoped to the entry's `repo` metadata when
 * set, otherwise across all indexed repos. Prefilled with the entry title and
 * auto-run once; hits deep-link into the code browser (?repo=&file=).
 */
export function EntryCodePanel({ entry }: { entry: Entry }) {
  const repoMeta = entry.metadata.repo;
  const repo =
    typeof repoMeta === "string" && repoMeta.trim()
      ? repoMeta.trim()
      : undefined;

  const [query, setQuery] = useState(() => entryTitle(entry));
  const [hits, setHits] = useState<CodeSearchHit[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const autoRan = useRef(false);

  const onSearch = async (q: string) => {
    const trimmed = q.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      setHits(await searchCode({ query: trimmed, repo, limit: 10 }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (autoRan.current) return;
    autoRan.current = true;
    const initial = entryTitle(entry);
    if (initial.trim()) void onSearch(initial);
    // Panel remounts per entry; only the entry's own title should auto-run.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entry.id]);

  return (
    <div className="flex h-full flex-col overflow-y-auto p-4">
      <div className="space-y-2">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Code Search
        </h3>
        <div className="flex gap-1.5">
          <div className="relative flex-1">
            <Search className="absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground/40" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && onSearch(query)}
              placeholder={repo ? `Search ${repo}…` : "Search all repos…"}
              className="h-8 rounded-lg pl-8 text-xs"
            />
          </div>
          <Button
            size="sm"
            className="h-8 rounded-lg px-3"
            disabled={busy || !query.trim()}
            onClick={() => onSearch(query)}
          >
            {busy ? (
              <LoaderCircle className="size-3.5 animate-spin" />
            ) : (
              <Search className="size-3.5" />
            )}
          </Button>
        </div>
        <div className="flex items-center justify-between gap-2">
          <span className="font-mono text-[10px] text-muted-foreground/60">
            {repo ? (
              <>
                scoped to <span className="text-foreground/80">{repo}</span>
              </>
            ) : (
              "no repo metadata — searching all repos"
            )}
          </span>
          <Link
            href={
              repo
                ? `/code?repo=${encodeURIComponent(repo)}`
                : "/code"
            }
            className="flex shrink-0 items-center gap-1 font-mono text-[10px] text-muted-foreground transition-colors hover:text-primary"
            title="Open in Code Search"
          >
            <Code2 className="size-3" />
            open code search
            <ExternalLink className="size-2.5" />
          </Link>
        </div>
      </div>

      <div className="mt-4 flex-1 space-y-2">
        {error && (
          <p className="break-all rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">
            {error}
          </p>
        )}
        {!error && hits === null && !busy && (
          <div className="flex flex-col items-center justify-center gap-2 py-10 text-center">
            <ScanSearch className="size-6 text-muted-foreground/30" />
            <p className="text-xs text-muted-foreground/60">
              Type a query to search indexed code.
            </p>
          </div>
        )}
        {!error && busy && hits === null && (
          <div className="flex items-center justify-center gap-2 py-10">
            <div className="size-4 animate-spin rounded-full border-2 border-border border-t-primary" />
            <span className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
              Searching code…
            </span>
          </div>
        )}
        {hits?.length === 0 && (
          <div className="flex flex-col items-center justify-center gap-2 py-10 text-center">
            <Search className="size-6 text-muted-foreground/30" />
            <p className="text-xs text-muted-foreground/60">
              No code matched that query.
            </p>
          </div>
        )}
        {hits?.map((hit, i) => (
          <Link
            key={`${hit.repo}/${hit.path}/${hit.start_line}/${i}`}
            href={`/code?repo=${encodeURIComponent(hit.repo)}&file=${encodeURIComponent(hit.path)}`}
            title="Open in Code Search"
            className={cn(
              "block space-y-1.5 rounded-xl border border-border bg-card p-3 transition-colors",
              "hover:border-primary/30 hover:bg-primary/[0.03]",
            )}
          >
            <div className="flex items-center justify-between gap-2">
              <span className="truncate font-mono text-xs font-semibold text-primary">
                {hit.symbol_name ?? basename(hit.path)}
              </span>
              <ScoreBadge score={hit.score} />
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge
                variant="outline"
                className="h-4 px-1.5 font-mono text-[8px] uppercase leading-none"
              >
                {hit.repo}
              </Badge>
              {hit.symbol_kind && (
                <Badge
                  variant="secondary"
                  className="h-4 px-1.5 font-mono text-[8px] uppercase leading-none"
                >
                  {hit.symbol_kind}
                </Badge>
              )}
              <span className="flex min-w-0 items-center gap-1 font-mono text-[10px] text-muted-foreground/60">
                <FileCode className="size-3 shrink-0" />
                <span className="truncate">
                  {hit.path}:{hit.start_line}
                </span>
              </span>
            </div>
            <pre className="max-h-16 overflow-hidden whitespace-pre-wrap rounded-lg bg-muted/40 px-2.5 py-1.5 font-mono text-[10px] leading-relaxed text-foreground/70">
              {hit.snippet}
            </pre>
          </Link>
        ))}
      </div>
    </div>
  );
}
