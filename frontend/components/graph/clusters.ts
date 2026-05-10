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
  /** Stable cluster label per node (used as `cluster` attribute on reagraph nodes). */
  byNode: Map<string, string>
  /** Per-cluster fill color. */
  colorByCluster: Map<string, string>
  /** Display label per cluster (same as the key — kept for symmetry). */
  labels: Map<string, string>
}

export function getNodeTitle(entry: Entry): string {
  const meta = entry.metadata ?? {}
  const explicit = meta.title ?? meta.name ?? meta.label
  if (typeof explicit === "string" && explicit.trim().length > 0) {
    return explicit.trim().slice(0, 60)
  }
  const firstLine = (entry.text ?? "").split(/\r?\n/).map((l) => l.trim()).find(Boolean)
  if (firstLine) {
    // strip leading markdown header markers
    const cleaned = firstLine.replace(/^#+\s*/, "").replace(/^[-*+]\s+/, "")
    return cleaned.slice(0, 60) + (cleaned.length > 60 ? "…" : "")
  }
  return entry.id.slice(0, 60)
}

function extractTags(entry: Entry): string[] {
  const raw = entry.metadata?.tags
  if (typeof raw === "string") {
    return raw.split(/[,;]/).map((t) => t.trim()).filter(Boolean)
  }
  return []
}

function topByCount<K>(items: Iterable<K>): K | undefined {
  const counts = new Map<K, number>()
  for (const item of items) counts.set(item, (counts.get(item) ?? 0) + 1)
  let best: { key: K; n: number } | undefined
  for (const [key, n] of counts) {
    if (!best || n > best.n) best = { key, n }
  }
  return best?.key
}

export interface CommunityOptions {
  /**
   * Louvain resolution. >1 produces more, smaller communities; <1 merges
   * into fewer, broader communities. Default 1.
   */
  resolution?: number
}

export function computeCommunities(
  entries: Entry[],
  edges: Edge[],
  pairwiseDistances: GraphNodeDistance[] = [],
  options: CommunityOptions = {}
): ClusterAssignment {
  const byNode = new Map<string, string>()
  const colorByCluster = new Map<string, string>()
  const labels = new Map<string, string>()

  if (entries.length === 0) return { byNode, colorByCluster, labels }

  const graph = new Graph({ type: "undirected", multi: false })
  for (const entry of entries) graph.addNode(entry.id)

  const addEdge = (a: string, b: string, weight: number) => {
    if (a === b) return
    if (!graph.hasNode(a) || !graph.hasNode(b)) return
    if (graph.hasEdge(a, b)) {
      const k = graph.edge(a, b)
      const existing = graph.getEdgeAttribute(k, "weight") as number
      graph.setEdgeAttribute(k, "weight", Math.max(existing, weight))
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

  const resolution = options.resolution ?? 1
  let louvainAssignments: Record<string, number>
  try {
    louvainAssignments = louvain(graph, { getEdgeWeight: "weight", resolution })
  } catch {
    louvainAssignments = Object.fromEntries(entries.map((e, i) => [e.id, i]))
  }

  // Group entries by Louvain community.
  const groups = new Map<number, Entry[]>()
  const entryById = new Map(entries.map((e) => [e.id, e]))
  for (const entry of entries) {
    const cid = louvainAssignments[entry.id] ?? -1
    const list = groups.get(cid) ?? []
    list.push(entry)
    groups.set(cid, list)
  }

  // Build labels — dominant source_id + top tag, dedupe with suffix.
  const sortedGroups = [...groups.entries()].sort(
    ([, a], [, b]) => b.length - a.length
  )
  const usedLabels = new Map<string, number>()
  const groupLabel = new Map<number, string>()

  for (const [cid, members] of sortedGroups) {
    const sourceId = topByCount(members.map((e) => e.source_id)) ?? "cluster"
    const tags = members.flatMap(extractTags)
    const topTag = topByCount(tags)
    const base = topTag ? `${sourceId} · ${topTag}` : sourceId
    const seen = usedLabels.get(base) ?? 0
    const label = seen === 0 ? base : `${base} (${seen + 1})`
    usedLabels.set(base, seen + 1)
    groupLabel.set(cid, label)
  }

  for (const entry of entries) {
    const cid = louvainAssignments[entry.id] ?? -1
    const label = groupLabel.get(cid) ?? entry.source_id
    byNode.set(entry.id, label)
  }

  const uniq = Array.from(new Set(byNode.values()))
  uniq.forEach((label, idx) => {
    colorByCluster.set(label, CLUSTER_PALETTE[idx % CLUSTER_PALETTE.length])
    labels.set(label, label)
  })

  // Reference to silence unused-var warnings if entryById is unused in callers.
  void entryById

  return { byNode, colorByCluster, labels }
}
