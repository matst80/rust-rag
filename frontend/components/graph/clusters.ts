import Graph from "graphology"
import louvain from "graphology-communities-louvain"
import type { Edge, Entry, GraphNodeDistance } from "@/lib/api"

const SIMILARITY_CUTOFF = 0.85

const CLUSTER_PALETTE = [
  "#6366f1",
  "#06b6d4",
  "#10b981",
  "#f59e0b",
  "#ef4444",
  "#a855f7",
  "#ec4899",
  "#14b8a6",
  "#eab308",
  "#f97316",
  "#3b82f6",
  "#84cc16",
]

export interface ClusterAssignment {
  byNode: Map<string, string>
  colorByCluster: Map<string, string>
}

export function computeCommunities(
  entries: Entry[],
  edges: Edge[],
  pairwiseDistances: GraphNodeDistance[] = []
): ClusterAssignment {
  const byNode = new Map<string, string>()
  const colorByCluster = new Map<string, string>()

  if (entries.length === 0) {
    return { byNode, colorByCluster }
  }

  const graph = new Graph({ type: "undirected", multi: false })
  for (const entry of entries) {
    graph.addNode(entry.id)
  }

  const addEdge = (a: string, b: string, weight: number) => {
    if (a === b) return
    if (!graph.hasNode(a) || !graph.hasNode(b)) return
    if (graph.hasEdge(a, b)) {
      const existing = graph.getEdgeAttribute(graph.edge(a, b), "weight") as number
      graph.setEdgeAttribute(graph.edge(a, b), "weight", Math.max(existing, weight))
      return
    }
    graph.addEdge(a, b, { weight })
  }

  for (const edge of edges) {
    const w =
      edge.edge_type === "similarity"
        ? Math.max(0.05, 1 - (edge.distance ?? 0.5))
        : Math.max(0.5, edge.weight ?? 1)
    addEdge(edge.source_id, edge.target_id, w)
  }

  for (const d of pairwiseDistances) {
    if (!Number.isFinite(d.distance) || d.distance >= SIMILARITY_CUTOFF) continue
    addEdge(d.from_item_id, d.to_item_id, 1 - d.distance)
  }

  // Isolated nodes get their own singleton cluster from Louvain naturally.
  let assignments: Record<string, number>
  try {
    assignments = louvain(graph, { getEdgeWeight: "weight" })
  } catch {
    assignments = Object.fromEntries(entries.map((e, i) => [e.id, i]))
  }

  for (const entry of entries) {
    const cid = String(assignments[entry.id] ?? entry.id)
    byNode.set(entry.id, cid)
  }

  const uniq = Array.from(new Set(byNode.values()))
  uniq.forEach((cid, idx) => {
    colorByCluster.set(cid, CLUSTER_PALETTE[idx % CLUSTER_PALETTE.length])
  })

  return { byNode, colorByCluster }
}
