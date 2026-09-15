"use client"

import { useMemo, useState } from "react"
import { ChevronDown, ChevronRight, FileText, GitBranch, Link2 } from "lucide-react"
import { cn, entryTitle, edgeEndpointTitle } from "@/lib/utils"
import { useEdges, useItems } from "@/lib/api"
import type { Edge, Entry } from "@/lib/api"

interface GraphNode {
  entry: Entry
  children: GraphNode[]
  /** Titles of connected entries that already live in the wiki structure — a filing hint. */
  linkedOrganized: { id: string; title: string }[]
}

function buildGraphTree(entries: Entry[], edges: Edge[]): GraphNode[] {
  const byId = new Map(entries.map((e) => [e.id, e]))
  const parentOf = new Map<string, string>()
  const childrenOf = new Map<string, string[]>()
  const linkedOrganized = new Map<string, { id: string; title: string }[]>()

  const isAncestor = (candidateAncestor: string, id: string): boolean => {
    let cur: string | undefined = id
    const seen = new Set<string>()
    while (cur && !seen.has(cur)) {
      seen.add(cur)
      if (cur === candidateAncestor) return true
      cur = parentOf.get(cur)
    }
    return false
  }

  // Manual edges first (intentional links), then similarity — both directions considered.
  const sorted = [...edges].sort((a, b) => {
    if (a.edge_type === "manual" && b.edge_type !== "manual") return -1
    if (a.edge_type !== "manual" && b.edge_type === "manual") return 1
    return 0
  })

  for (const edge of sorted) {
    const sourceIn = byId.has(edge.source_id)
    const targetIn = byId.has(edge.target_id)

    if (sourceIn && targetIn) {
      // Both unorganized: source becomes parent of target, first edge wins, no cycles.
      if (
        edge.source_id !== edge.target_id &&
        !parentOf.has(edge.target_id) &&
        !isAncestor(edge.target_id, edge.source_id)
      ) {
        parentOf.set(edge.target_id, edge.source_id)
        childrenOf.set(edge.source_id, [...(childrenOf.get(edge.source_id) ?? []), edge.target_id])
      }
      continue
    }

    // One side already organized (has a wiki path) — surface as a filing hint, not a tree edge.
    if (sourceIn && !targetIn) {
      const list = linkedOrganized.get(edge.source_id) ?? []
      list.push({ id: edge.target_id, title: edgeEndpointTitle(edge.target_id, edge.target_title) })
      linkedOrganized.set(edge.source_id, list)
    } else if (targetIn && !sourceIn) {
      const list = linkedOrganized.get(edge.target_id) ?? []
      list.push({ id: edge.source_id, title: edgeEndpointTitle(edge.source_id, edge.source_title) })
      linkedOrganized.set(edge.target_id, list)
    }
  }

  const buildNode = (id: string): GraphNode => ({
    entry: byId.get(id)!,
    children: (childrenOf.get(id) ?? []).map(buildNode),
    linkedOrganized: linkedOrganized.get(id) ?? [],
  })

  const roots = entries.filter((e) => !parentOf.has(e.id)).map((e) => buildNode(e.id))
  roots.sort((a, b) => b.entry.created_at - a.entry.created_at)
  return roots
}

interface GraphTreeProps {
  sourceId?: string
}

/** Tree of entries with no wiki path yet, clustered by their graph edges — drag any node onto a folder to file it. */
export function GraphTree({ sourceId }: GraphTreeProps) {
  const { data: itemsResp, isLoading } = useItems({ has_path: false, source_id: sourceId, limit: 300 })
  const { data: edges } = useEdges()

  const roots = useMemo(
    () => buildGraphTree(itemsResp?.items ?? [], edges ?? []),
    [itemsResp, edges],
  )

  if (isLoading) {
    return <p className="text-xs text-muted-foreground px-3">Loading…</p>
  }

  if (roots.length === 0) {
    return (
      <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
        <GitBranch className="size-5 text-muted-foreground/30" />
        <p className="text-xs text-muted-foreground">
          Nothing unorganized — every entry has a wiki path.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col py-2">
      <p className="px-3 pb-2 text-xs text-muted-foreground">
        {itemsResp?.total_count ?? roots.length} entr
        {(itemsResp?.total_count ?? roots.length) === 1 ? "y" : "ies"} without a wiki path.
        Drag one onto a folder to file it.
      </p>
      {roots.map((node) => (
        <GraphTreeItem key={node.entry.id} node={node} depth={0} />
      ))}
    </div>
  )
}

function GraphTreeItem({ node, depth }: { node: GraphNode; depth: number }) {
  const [open, setOpen] = useState(depth < 1)
  const expandable = node.children.length > 0
  const padding = `calc(${depth} * 0.75rem + 0.25rem)`

  return (
    <div className="flex flex-col">
      <div
        draggable
        onDragStart={(evt) => {
          evt.dataTransfer.setData(
            "application/json",
            JSON.stringify({ id: node.entry.id, source_id: node.entry.source_id, path: node.entry.path }),
          )
          evt.dataTransfer.setData("text/plain", node.entry.id)
          evt.dataTransfer.effectAllowed = "move"
        }}
        className="flex items-center gap-1 cursor-grab active:cursor-grabbing transition-colors hover:bg-card rounded-sm"
      >
        <button
          type="button"
          onClick={() => expandable && setOpen((v) => !v)}
          className={cn(
            "shrink-0 flex items-center justify-center size-5 text-muted-foreground hover:text-foreground transition-colors",
            !expandable && "opacity-30 pointer-events-none",
          )}
          style={{ marginLeft: padding }}
          aria-label={open ? "Collapse" : "Expand"}
        >
          {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
        </button>
        <div className="flex flex-1 min-w-0 items-center gap-2 px-1 py-1" title={node.entry.id}>
          <FileText className="size-3.5 text-muted-foreground shrink-0" />
          <span className="truncate text-xs">{entryTitle(node.entry)}</span>
          {node.entry.source_id && (
            <span className="ml-auto shrink-0 text-[9px] font-medium uppercase tracking-wide text-muted-foreground/60">
              {node.entry.source_id}
            </span>
          )}
        </div>
      </div>

      {node.linkedOrganized.length > 0 && (
        <div
          className="flex flex-wrap gap-1 pb-1"
          style={{ marginLeft: `calc(${depth + 1} * 0.75rem + 1.5rem)` }}
        >
          {node.linkedOrganized.slice(0, 3).map((l) => (
            <span
              key={l.id}
              className="flex items-center gap-1 rounded border border-border/60 bg-muted/20 px-1.5 py-0.5 text-[9px] text-muted-foreground"
              title={`Already linked to "${l.title}" in the wiki`}
            >
              <Link2 className="size-2.5" />
              {l.title}
            </span>
          ))}
        </div>
      )}

      {open && expandable && (
        <div className="flex flex-col">
          {node.children.map((child) => (
            <GraphTreeItem key={child.entry.id} node={child} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  )
}
