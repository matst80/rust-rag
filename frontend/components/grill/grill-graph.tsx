"use client"

import { useMemo } from "react"
import dynamic from "next/dynamic"
import { LoaderCircle } from "lucide-react"
import type { GraphCanvasProps, GraphEdge, GraphNode, Theme } from "reagraph"
import { useGraphNeighborhood } from "@/lib/api"
import type { HarnessTreeResponse } from "@/lib/api"

const GraphCanvas = dynamic(
  () => import("reagraph").then((m) => m.GraphCanvas),
  { ssr: false }
) as unknown as React.ComponentType<GraphCanvasProps>

const TYPE_COLORS: Record<string, string> = {
  harness_plan: "#00f2ff",
  harness_sprint: "#22d3ee",
  harness_todo: "#67e8f9",
  harness_doc: "#f59e0b",
  harness_agent: "#a78bfa",
  harness_stream: "#34d399",
  harness_audit: "#94a3b8",
  harness_repo: "#e879f9",
  harness_poc: "#f472b6",
  decision: "#60a5fa",
  harness_risk: "#ef4444",
  harness_compliance: "#fbbf24",
  harness_resource: "#a3e635",
  harness_scaling: "#fb923c",
  harness_validation: "#2dd4bf",
  harness_rollout: "#818cf8",
}

const BADGE_COLORS: Record<string, string> = {
  red: "#ef4444",
  yellow: "#f59e0b",
  green: "#10b981",
}

const DARK_THEME: Theme = {
  canvas: { background: "transparent", fog: null },
} as Theme

function GrillGraphInner({
  centerId,
  onNodeClick,
  tree,
}: {
  centerId: string
  onNodeClick: (id: string) => void
  tree: HarnessTreeResponse | null
}) {
  const { data: neighborhood, isLoading } = useGraphNeighborhood(centerId, 2, 100)

  const metaById = useMemo(() => {
    const map = new Map<string, HarnessTreeResponse["nodes"][number]>()
    for (const node of tree?.nodes ?? []) map.set(node.id, node)
    return map
  }, [tree])

  const nodes = useMemo<GraphNode[]>(() => {
    const entries = neighborhood?.nodes ?? []
    return entries.map((entry) => {
      const meta = metaById.get(entry.id)
      const fill = meta
        ? BADGE_COLORS[meta.badge ?? ""] ?? TYPE_COLORS[meta.type_name] ?? "#64748b"
        : "#64748b"
      return {
        id: entry.id,
        label: meta?.title ?? entry.text.slice(0, 40),
        subLabel: meta?.type_name ?? undefined,
        fill,
        size: entry.id === centerId ? 14 : 8,
      }
    })
  }, [neighborhood, metaById, centerId])

  const edges = useMemo<GraphEdge[]>(() => {
    const out: GraphEdge[] = []
    const seen = new Set<string>()
    for (const edge of neighborhood?.edges ?? []) {
      const key = `${edge.source_id}::${edge.target_id}`
      if (seen.has(key)) continue
      seen.add(key)
      out.push({
        id: edge.id,
        source: edge.source_id,
        target: edge.target_id,
        label: edge.relationship,
        size: 1.5,
        // CONFLICTS_WITH was folded into the canonical `contradicts` predicate.
        fill:
          edge.relationship === "CONFLICTS_WITH" || edge.relationship === "contradicts"
            ? "#ef4444"
            : undefined,
      })
    }
    return out
  }, [neighborhood])

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <LoaderCircle className="size-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="h-full w-full">
      <GraphCanvas
        theme={DARK_THEME}
        nodes={nodes}
        edges={edges}
        layoutType="forceDirected2d"
        labelType="auto"
        actives={[centerId]}
        draggable
        onNodeClick={(node) => onNodeClick(node.id)}
      />
    </div>
  )
}

export function GrillGraph(props: {
  centerId: string
  onNodeClick: (id: string) => void
  tree: HarnessTreeResponse | null
}) {
  return <GrillGraphInner {...props} />
}
