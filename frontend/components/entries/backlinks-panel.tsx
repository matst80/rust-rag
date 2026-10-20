"use client"

import Link from "next/link"
import { ArrowLeft, Loader2, Quote } from "lucide-react"
import { useBacklinks, useEdgesForItem, type Edge } from "@/lib/api"
import { edgeEndpointTitle, normalizeTitle } from "@/lib/utils"
import { DuplicateTitleBadge } from "./duplicate-title-badge"
import { useDuplicateTitles } from "@/lib/api"

interface BacklinksPanelProps {
  id: string
  edges: Edge[] | undefined
}

/**
 * Notion/Roam-style backlinks: every entry whose text references this one
 * via a wikilink (`[[id]]`, `[[Title]]`) or `/entries/{id}` link, plus the
 * incoming manual graph edges. Wikilink refs come from the new
 * `GET /api/items/{id}/backlinks` endpoint (code blocks excluded
 * server-side); edges are already loaded by the parent.
 */
export function BacklinksPanel({ id, edges }: BacklinksPanelProps) {
  const { data: backlinksResp, isLoading } = useBacklinks(id)
  const { data: dupData } = useDuplicateTitles()
  const dupGroupFor = (title: string) => {
    const key = normalizeTitle(title)
    return dupData?.groups.find((g) => normalizeTitle(g.title) === key) ?? null
  }
  const { data: edgesForItem } = useEdgesForItem(id)
  const liveEdges = edges ?? edgesForItem

  const incoming = (liveEdges ?? []).filter(
    (e) => e.edge_type === "manual" && e.source_id !== id
  )
  const outgoing = (liveEdges ?? []).filter(
    (e) => e.edge_type === "manual" && e.source_id === id
  )

  const incomingIds = new Set(incoming.map((e) => e.source_id))
  const backlinks = (backlinksResp?.backlinks ?? []).filter(
    (b) => !incomingIds.has(b.id)
  )
  const total = incoming.length + backlinks.length

  if (isLoading && !backlinksResp) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        <span className="font-mono text-[11px] uppercase tracking-widest">
          Finding backlinks…
        </span>
      </div>
    )
  }

  if (total === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
        <ArrowLeft className="size-5 text-muted-foreground/30" />
        <p className="text-sm text-muted-foreground">No backlinks yet</p>
        <p className="max-w-60 text-xs leading-relaxed text-muted-foreground/70">
          When another entry links here with{" "}
          <code className="rounded bg-muted px-1 font-mono text-[11px]">
            [[{id}]]
          </code>{" "}
          it will show up here automatically — no manual edge needed.
        </p>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="flex-1 space-y-6 p-4">
        {incoming.length > 0 && (
          <div className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Linked here · {incoming.length}
            </h3>
            <div className="flex flex-col gap-1.5">
              {incoming.map((edge) => (
                <Link
                  key={edge.id}
                  href={`/entries/${encodeURIComponent(edge.source_id)}`}
                  prefetch={false}
                  className="group flex flex-col gap-1 rounded-lg border border-border p-3 transition-colors hover:border-primary/40 hover:bg-primary/5"
                >
                  <span className="truncate text-sm font-medium text-foreground transition-colors group-hover:text-primary">
                    {edgeEndpointTitle(edge.source_id, edge.source_title)}
                  </span>
                  <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground/60">
                    {edge.relationship.replace(/_/g, " ")} · manual edge
                  </span>
                </Link>
              ))}
            </div>
          </div>
        )}

        {backlinks.length > 0 && (
          <div className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Mentioned in · {backlinks.length}
            </h3>
            <div className="flex flex-col gap-1.5">
              {backlinks.map((b) => {
                const dup = dupGroupFor(b.title || b.id)
                return (
                <Link
                  key={b.id}
                  href={`/entries/${encodeURIComponent(b.id)}`}
                  prefetch={false}
                  className="group flex flex-col gap-1.5 rounded-lg border border-border p-3 transition-colors hover:border-primary/40 hover:bg-primary/5"
                >
                  <span className="flex items-center gap-1.5 truncate text-sm font-medium text-foreground transition-colors group-hover:text-primary">
                    <span className="truncate">{b.title || b.id}</span>
                    {dup && <DuplicateTitleBadge group={dup} />}
                  </span>
                  {dup && (
                    <span className="font-mono text-[10px] text-muted-foreground/60">
                      {b.source_id} · {b.id.length > 12 ? `${b.id.slice(0, 8)}…` : b.id}
                    </span>
                  )}
                  <span className="flex items-start gap-1.5 text-xs leading-relaxed text-muted-foreground">
                    <Quote className="mt-0.5 size-3 shrink-0 opacity-50" />
                    <span className="line-clamp-2">{b.context}</span>
                  </span>
                  <span className="font-mono text-[10px] text-muted-foreground/50">
                    {b.matched_ref}
                  </span>
                </Link>
                )
              })}
            </div>
          </div>
        )}

        {outgoing.length > 0 && (
          <div className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Links out · {outgoing.length}
            </h3>
            <div className="flex flex-col gap-1.5">
              {outgoing.map((edge) => (
                <Link
                  key={edge.id}
                  href={`/entries/${encodeURIComponent(edge.target_id)}`}
                  prefetch={false}
                  className="truncate rounded-lg border border-border/60 px-3 py-2 text-xs text-muted-foreground transition-colors hover:border-primary/30 hover:text-primary"
                >
                  {edgeEndpointTitle(edge.target_id, edge.target_title)}
                </Link>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
