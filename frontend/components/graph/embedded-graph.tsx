"use client"

import { useMemo } from "react"
import dynamic from "next/dynamic"
import type { GraphCanvasProps, GraphEdge, GraphNode, InternalGraphNode } from "reagraph"
import { LoaderCircle } from "lucide-react"
import { useGraphNeighborhood } from "@/lib/api"
import { computeCommunities } from "./clusters"

const GraphCanvas = dynamic(
  () => import("reagraph").then((m) => m.GraphCanvas),
  { ssr: false }
) as unknown as React.ComponentType<GraphCanvasProps>

const SIMILARITY_DRAW_CUTOFF = 0.85

interface EmbeddedGraphProps {
  centerId: string
  onNodeClick?: (id: string) => void
}

export function EmbeddedGraph({ centerId, onNodeClick }: EmbeddedGraphProps) {
  const { data: neighborhood, isLoading } = useGraphNeighborhood(centerId, 1, 30)

  const entries = neighborhood?.nodes ?? []
  const edges = neighborhood?.edges ?? []
  const pairwise = neighborhood?.pairwise_distances ?? []

  const clusters = useMemo(
    () => computeCommunities(entries, edges, pairwise),
    [entries, edges, pairwise]
  )

  const nodes = useMemo<GraphNode[]>(
    () =>
      entries.map((entry) => {
        const cid = clusters.byNode.get(entry.id) ?? "unknown"
        const fill = clusters.colorByCluster.get(cid) ?? "#64748b"
        return {
          id: entry.id,
          label: entry.id.length > 28 ? `${entry.id.slice(0, 25)}…` : entry.id,
          fill,
          size: entry.id === centerId ? 14 : 8,
          cluster: cid,
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
      out.push({
        id: edge.id,
        source: edge.source_id,
        target: edge.target_id,
        label: edge.edge_type === "similarity" ? undefined : edge.relationship,
        size: edge.edge_type === "similarity" ? 1 : 2,
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
    return out
  }, [edges, pairwise])

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
        nodes={nodes}
        edges={reagraphEdges}
        clusterAttribute="cluster"
        layoutType="forceDirected2d"
        actives={[centerId]}
        draggable
        onNodeClick={(node: InternalGraphNode) => onNodeClick?.(node.id)}
      />
    </div>
  )
}
