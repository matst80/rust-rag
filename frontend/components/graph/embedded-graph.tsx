"use client"

import { useMemo } from "react"
import { useTheme } from "next-themes"
import dynamic from "next/dynamic"
import { darkTheme, lightTheme } from "reagraph"
import type { GraphCanvasProps, GraphEdge, GraphNode, InternalGraphNode, Theme } from "reagraph"
import { LoaderCircle } from "lucide-react"
import { useGraphNeighborhood, useItem, useSearch } from "@/lib/api"
import type { Entry } from "@/lib/api"
import { computeCommunities, getNodeTitle } from "./clusters"

const GraphCanvas = dynamic(
  () => import("reagraph").then((m) => m.GraphCanvas),
  { ssr: false }
) as unknown as React.ComponentType<GraphCanvasProps>

const SIMILARITY_DRAW_CUTOFF = 0.85

interface EmbeddedGraphProps {
  centerId: string
  onNodeClick?: (id: string) => void
}

function buildEmbeddedTheme(isDark: boolean): Theme {
  const base = isDark ? darkTheme : lightTheme
  const bg = isDark ? "#0a0a0a" : "#fafafa"
  return {
    ...base,
    canvas: { background: bg, fog: null },
    node: {
      ...base.node,
      label: {
        ...base.node.label,
        color: isDark ? "#e2e8f0" : "#1e293b",
        stroke: bg,
      },
      subLabel: {
        ...(base.node.subLabel ?? { color: isDark ? "#94a3b8" : "#64748b", activeColor: "#4338ca" }),
        color: isDark ? "#94a3b8" : "#64748b",
        stroke: bg,
      },
    },
    cluster: {
      stroke: isDark ? "#475569" : "#cbd5e1",
      opacity: 0.4,
      selectedOpacity: 0.6,
      inactiveOpacity: 0.15,
      label: {
        color: isDark ? "#94a3b8" : "#64748b",
        stroke: bg,
        fontSize: 11,
      },
    },
  }
}

const SEMANTIC_TOP_K = 8

export function EmbeddedGraph({ centerId, onNodeClick }: EmbeddedGraphProps) {
  const { data: neighborhood, isLoading } = useGraphNeighborhood(centerId, 1, 30)
  const { data: centerItem } = useItem(centerId)
  const { resolvedTheme } = useTheme()
  const isDark = resolvedTheme === "dark"
  const theme = useMemo(() => buildEmbeddedTheme(isDark), [isDark])

  // Pull top semantic neighbors so the embedded view stays useful even when
  // the entry has no manual or graph-DB-similarity edges yet.
  const { data: semanticBundle } = useSearch(
    centerItem?.text ?? "",
    undefined,
    true,
    SEMANTIC_TOP_K
  )

  const baseEntries = neighborhood?.nodes ?? []
  const edges = neighborhood?.edges ?? []
  const pairwise = neighborhood?.pairwise_distances ?? []

  const entries = useMemo<Entry[]>(() => {
    const merged = [...baseEntries]
    const seen = new Set(merged.map((e) => e.id))
    for (const hit of semanticBundle?.results ?? []) {
      if (seen.has(hit.id)) continue
      seen.add(hit.id)
      merged.push({
        id: hit.id,
        text: hit.text,
        metadata: hit.metadata,
        source_id: hit.source_id,
        created_at: hit.created_at,
      })
    }
    return merged
  }, [baseEntries, semanticBundle])

  const semanticEdges = useMemo(() => {
    const present = new Set<string>()
    for (const e of edges) {
      present.add(`${e.source_id}::${e.target_id}`)
      present.add(`${e.target_id}::${e.source_id}`)
    }
    for (const d of pairwise) {
      present.add(`${d.from_item_id}::${d.to_item_id}`)
      present.add(`${d.to_item_id}::${d.from_item_id}`)
    }
    const out: { from: string; to: string; distance: number }[] = []
    for (const hit of semanticBundle?.results ?? []) {
      if (hit.id === centerId) continue
      const key = `${centerId}::${hit.id}`
      if (present.has(key)) continue
      // hit.score is roughly 0..1 similarity; convert to a distance proxy.
      const dist = Math.max(0.05, Math.min(0.95, 1 - hit.score))
      out.push({ from: centerId, to: hit.id, distance: dist })
    }
    return out
  }, [semanticBundle, edges, pairwise, centerId])

  const pairwiseForClustering = useMemo(
    () => [
      ...pairwise,
      ...semanticEdges.map((s) => ({
        from_item_id: s.from,
        to_item_id: s.to,
        distance: s.distance,
      })),
    ],
    [pairwise, semanticEdges]
  )

  const clusters = useMemo(
    () => computeCommunities(entries, edges, pairwiseForClustering),
    [entries, edges, pairwiseForClustering]
  )

  const nodes = useMemo<GraphNode[]>(
    () =>
      entries.map((entry) => {
        const cid = clusters.byNode.get(entry.id) ?? "unknown"
        const fill = clusters.colorByCluster.get(cid) ?? "#64748b"
        return {
          id: entry.id,
          label: getNodeTitle(entry),
          subLabel: entry.source_id,
          fill,
          size: entry.id === centerId ? 14 : 8,
          cluster: cid,
          data: { cluster: cid, sourceId: entry.source_id },
        }
      }),
    [entries, clusters, centerId]
  )

  const reagraphEdges = useMemo<GraphEdge[]>(() => {
    const out: GraphEdge[] = []
    const seen = new Set<string>()

    for (const edge of edges) {
      seen.add(`${edge.source_id}::${edge.target_id}`)
      seen.add(`${edge.target_id}::${edge.source_id}`)
      const relationship = edge.relationship?.toLowerCase()
      const isUnrelated = relationship === "unrelated"
      const isContradicts = relationship === "contradicts"

      let fill: string | undefined
      if (isUnrelated) {
        fill = "#ef4444" // red
      } else if (isContradicts) {
        fill = "#f97316" // orange
      } else if (edge.edge_type === "manual") {
        fill = "#3b82f6" // blue
      }

      out.push({
        id: edge.id,
        source: edge.source_id,
        target: edge.target_id,
        label: edge.edge_type === "similarity" ? undefined : edge.relationship,
        size: edge.edge_type === "similarity" ? 1 : 2,
        fill,
      })
    }
    for (const d of pairwise) {
      if (!Number.isFinite(d.distance) || d.distance >= SIMILARITY_DRAW_CUTOFF) continue
      const key = `${d.from_item_id}::${d.to_item_id}`
      if (seen.has(key)) continue
      seen.add(key)
      seen.add(`${d.to_item_id}::${d.from_item_id}`)
      out.push({
        id: `sim-${d.from_item_id}-${d.to_item_id}`,
        source: d.from_item_id,
        target: d.to_item_id,
        size: 0.6,
      })
    }
    for (const s of semanticEdges) {
      const key = `${s.from}::${s.to}`
      if (seen.has(key)) continue
      seen.add(key)
      seen.add(`${s.to}::${s.from}`)
      out.push({
        id: `sem-${s.from}-${s.to}`,
        source: s.from,
        target: s.to,
        size: 0.5,
      })
    }
    return out
  }, [edges, pairwise, semanticEdges])

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <LoaderCircle className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="h-full w-full">
      <GraphCanvas
        theme={theme}
        nodes={nodes}
        edges={reagraphEdges}
        clusterAttribute="cluster"
        layoutType="forceDirected2d"
        layoutOverrides={{
          clusterType: "treemap",
          linkStrengthIntraCluster: 0.85,
          linkStrengthInterCluster: 0.02,
        }}
        labelType="auto"
        actives={[centerId]}
        draggable
        onNodeClick={(node: InternalGraphNode) => onNodeClick?.(node.id)}
      />
    </div>
  )
}
