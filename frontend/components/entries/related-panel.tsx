"use client";

import Link from "next/link";
import { useState } from "react";
import { GitBranch, Link2, Loader2, Plus, Search, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { RELATION_STYLES } from "../graph/relation-item";
import { cn, edgeEndpointTitle, entryTitle } from "@/lib/utils";
import type { Edge } from "@/lib/api";
import { useCreateEdge, useItem, useSearch } from "@/lib/api";
import { useSWRConfig } from "swr";
import { toast } from "sonner";

interface RelatedPanelProps {
  id: string;
  edges: Edge[] | undefined;
}

/** Content-focused list of an entry's manual links and semantic neighbors, titles first. */
export function RelatedPanel({ id, edges }: RelatedPanelProps) {
  if (!edges || edges.length === 0) {
    return (
      <div className="flex h-full flex-col overflow-y-auto">
        <div className="flex flex-1 flex-col items-center justify-center p-8 text-center gap-2">
          <GitBranch className="size-5 text-muted-foreground/30" />
          <p className="text-sm text-muted-foreground">No related entries yet</p>
          <p className="text-xs text-muted-foreground/70">
            Link this entry to another one to build the knowledge graph.
          </p>
        </div>
        <div className="shrink-0 border-t border-border p-4">
          <QuickLink id={id} />
        </div>
      </div>
    );
  }

  const sorted = [...edges].sort((a, b) => {
    if (a.edge_type === "manual" && b.edge_type !== "manual") return -1;
    if (a.edge_type !== "manual" && b.edge_type === "manual") return 1;
    return a.sort_order.localeCompare(b.sort_order);
  });

  const manual = sorted.filter((e) => e.edge_type === "manual");
  const similar = sorted.filter((e) => e.edge_type !== "manual");
  // Backlinks: manual edges pointing *at* this entry from elsewhere.
  const outgoing = manual.filter((e) => e.source_id === id);
  const incoming = manual.filter((e) => e.source_id !== id);

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="flex-1 space-y-6 p-4">
        {outgoing.length > 0 && (
          <RelatedSection title="Linked" id={id} edges={outgoing} />
        )}
        {incoming.length > 0 && (
          <RelatedSection title="Linked here" id={id} edges={incoming} />
        )}
        {similar.length > 0 && (
          <RelatedSection title="Similar" id={id} edges={similar} muted />
        )}
        <UnlinkedMentions id={id} edges={edges} />
      </div>
      <div className="shrink-0 border-t border-border p-4">
        <QuickLink id={id} />
      </div>
    </div>
  );
}

/** Entries that semantically match this entry's title but share no edge
 *  with it yet — Roam-style unlinked mentions, one click to link. */
function UnlinkedMentions({ id, edges }: { id: string; edges: Edge[] }) {
  const { mutate } = useSWRConfig();
  const { data: entry } = useItem(id);
  const [linking, setLinking] = useState<string | null>(null);
  const { trigger: createEdge } = useCreateEdge();

  const title = entry ? entryTitle(entry) : "";
  // Skip stub titles (bare ids) — they'd only match themselves.
  const query = title && title !== id ? title : "";
  const { data: results } = useSearch(query, undefined, undefined, true, 8);

  const connected = new Set<string>();
  for (const e of edges) {
    connected.add(e.source_id === id ? e.target_id : e.source_id);
  }
  const mentions = (results?.results ?? []).filter(
    (r) => r.id !== id && !connected.has(r.id),
  );
  if (!query || mentions.length === 0) return null;

  const linkMention = async (targetId: string) => {
    setLinking(targetId);
    try {
      await createEdge({
        source_id: id,
        target_id: targetId,
        relationship: "related_to",
        directed: true,
        weight: 1.0,
      });
      mutate(["edges", id]);
      toast.success("Entries linked");
    } catch {
      toast.error("Failed to link entries");
    } finally {
      setLinking(null);
    }
  };

  return (
    <div className="space-y-2">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        Mentioned in · unlinked
      </h3>
      <div className="flex flex-col gap-1.5">
        {mentions.slice(0, 5).map((m) => (
          <div
            key={m.id}
            className="group flex items-center gap-2 rounded-lg border border-dashed border-border p-3"
          >
            <Link
              href={`/entries/${encodeURIComponent(m.id)}`}
              className="min-w-0 flex-1 truncate text-sm font-medium text-foreground transition-colors hover:text-primary"
            >
              {entryTitle(m)}
            </Link>
            <button
              type="button"
              disabled={linking !== null}
              onClick={() => linkMention(m.id)}
              className="flex shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground transition-colors hover:border-primary/40 hover:text-primary disabled:opacity-50"
              title="Create a link to this entry"
            >
              {linking === m.id ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Link2 className="size-3" />
              )}
              Link
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

const QUICK_RELATIONS = [
  "related_to",
  "part_of",
  "contains",
  "depends_on",
  "is_a",
  "contradicts",
  "caused_by",
] as const;

/** Inline "link this entry to another" without switching to the Graph tab. */
function QuickLink({ id }: { id: string }) {
  const { mutate } = useSWRConfig();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [relation, setRelation] = useState<string>("related_to");
  const [saving, setSaving] = useState(false);
  const { data: results } = useSearch(query, undefined, undefined, true, 5);
  const { trigger: createEdge } = useCreateEdge();

  const candidates = (results?.results ?? []).filter((r) => r.id !== id);

  const link = async (targetId: string) => {
    setSaving(true);
    try {
      await createEdge({
        source_id: id,
        target_id: targetId,
        relationship: relation,
        directed: true,
        weight: 1.0,
      });
      mutate(["edges", id]);
      setQuery("");
      setOpen(false);
      toast.success("Entries linked");
    } catch {
      toast.error("Failed to link entries");
    } finally {
      setSaving(false);
    }
  };

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="flex w-full items-center justify-center gap-1.5 rounded-xl border border-dashed border-border py-2.5 font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground transition-colors hover:border-primary/40 hover:text-primary"
      >
        <Plus className="size-3.5" /> Link another entry…
      </button>
    );
  }

  return (
    <div className="space-y-2 rounded-xl border border-border bg-muted/20 p-3">
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1.5 font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
          <Link2 className="size-3" /> Link to…
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          onClick={() => {
            setOpen(false);
            setQuery("");
          }}
          aria-label="Cancel linking"
        >
          <X className="size-3.5" />
        </Button>
      </div>
      <div className="flex gap-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search entries…"
            className="h-9 w-full rounded-lg border border-border bg-background pl-8 pr-2 text-sm focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
        <select
          value={relation}
          onChange={(e) => setRelation(e.target.value)}
          className="h-9 shrink-0 rounded-lg border border-border bg-background px-2 font-mono text-[11px] text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          title="Relationship type"
        >
          {QUICK_RELATIONS.map((r) => (
            <option key={r} value={r}>
              {r.replace(/_/g, " ")}
            </option>
          ))}
        </select>
      </div>
      {query.trim() && candidates.length > 0 && (
        <div className="flex max-h-48 flex-col gap-1 overflow-y-auto">
          {candidates.map((c) => (
            <button
              key={c.id}
              type="button"
              disabled={saving}
              onClick={() => link(c.id)}
              className="flex items-center gap-2 rounded-lg border border-border bg-card p-2 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 disabled:opacity-50"
            >
              <span className="min-w-0 flex-1 truncate text-xs font-medium">
                {entryTitle(c)}
              </span>
              <span className="shrink-0 font-mono text-[9px] uppercase text-muted-foreground">
                {c.source_id}
              </span>
              {saving && <Loader2 className="size-3 shrink-0 animate-spin" />}
            </button>
          ))}
        </div>
      )}
      {query.trim() && results && candidates.length === 0 && (
        <p className="text-xs text-muted-foreground">No other entries match.</p>
      )}
    </div>
  );
}

function RelatedSection({
  title,
  id,
  edges,
  muted,
}: {
  title: string;
  id: string;
  edges: Edge[];
  muted?: boolean;
}) {
  return (
    <div className="space-y-2">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        {title}
      </h3>
      <div className="flex flex-col gap-1.5">
        {edges.map((edge) => {
          const targetId = edge.source_id === id ? edge.target_id : edge.source_id;
          const targetTitle = edge.source_id === id ? edge.target_title : edge.source_title;

          return (
            <Link
              key={edge.id}
              href={`/entries/${encodeURIComponent(targetId)}`}
              className={cn(
                "group flex flex-col gap-1 rounded-lg border p-3 transition-colors",
                muted
                  ? "border-border/60 hover:border-primary/30 hover:bg-primary/[0.03]"
                  : "border-border hover:border-primary/40 hover:bg-primary/5",
              )}
            >
              <span className="text-sm font-medium text-foreground group-hover:text-primary transition-colors truncate">
                {edgeEndpointTitle(targetId, targetTitle)}
              </span>
              <div className="flex items-center gap-2">
                <Badge
                  variant="outline"
                  className={cn(
                    "h-5 px-1.5 text-[10px] font-medium uppercase tracking-wide leading-none",
                    RELATION_STYLES[edge.relationship.toLowerCase()] || "",
                  )}
                >
                  {edge.relationship.replace(/_/g, " ")}
                </Badge>
                {edge.edge_type !== "manual" && (
                  <span className="text-[10px] text-muted-foreground/60">
                    {Math.round((1 - (edge.distance ?? 0)) * 100)}% match
                  </span>
                )}
              </div>
            </Link>
          );
        })}
      </div>
    </div>
  );
}
