import { MarkerType, type Edge as FlowEdge, type Node } from "@xyflow/react"
import { type EntryNodeData } from "./entry-node"
import { type Edge, type Entry, type GraphNodeDistance } from "@/lib/api"

type GraphNode = Node<EntryNodeData>

function createGraphNode(
  entry: Entry,
  position: { x: number; y: number },
  depth: number,
  isCenter: boolean,
  isSelected: boolean
): GraphNode {
  return {
    id: entry.id,
    type: "entry",
    position,
    data: {
      label: entry.id,
      sourceId: entry.source_id,
      text: entry.text,
      depth,
      isCenter,
      isSelected,
    },
  }
}

function buildDepthMap(centerId: string, edges: Edge[]): Map<string, number> {
  const adjacency = new Map<string, string[]>()

  for (const edge of edges) {
    adjacency.set(edge.source_id, [
      ...(adjacency.get(edge.source_id) ?? []),
      edge.target_id,
    ])
    adjacency.set(edge.target_id, [
      ...(adjacency.get(edge.target_id) ?? []),
      edge.source_id,
    ])
  }

  const depthById = new Map<string, number>([[centerId, 0]])
  const queue = [centerId]

  while (queue.length > 0) {
    const currentId = queue.shift()
    if (!currentId) {
      continue
    }

    const currentDepth = depthById.get(currentId) ?? 0
    for (const neighborId of adjacency.get(currentId) ?? []) {
      if (depthById.has(neighborId)) {
        continue
      }

      depthById.set(neighborId, currentDepth + 1)
      queue.push(neighborId)
    }
  }

  return depthById
}

function fallbackLayout(
  entries: Entry[],
  edges: Edge[],
  centerId: string,
  selectedNode: string | null
): GraphNode[] {
  const depthById = buildDepthMap(centerId, edges)
  const entriesByDepth = new Map<number, Entry[]>()

  for (const entry of entries) {
    const depth = depthById.get(entry.id) ?? (entry.id === centerId ? 0 : 1)
    entriesByDepth.set(depth, [...(entriesByDepth.get(depth) ?? []), entry])
  }

  return [...entriesByDepth.entries()]
    .sort(([left], [right]) => left - right)
    .flatMap(([depth, group]) => {
      if (depth === 0) {
        const entry = group[0]
        if (!entry) {
          return []
        }

        return [
          createGraphNode(
            entry,
            { x: 0, y: 0 },
            0,
            true,
            selectedNode === entry.id
          ),
        ]
      }

      const radius = depth * 240
      return group.map((entry, index) => {
        const angle = (2 * Math.PI * index) / group.length
        return createGraphNode(
          entry,
          {
            x: Math.cos(angle) * radius,
            y: Math.sin(angle) * radius,
          },
          depth,
          false,
          selectedNode === entry.id
        )
      })
    })
}

function multiplyMatrixVector(matrix: number[][], vector: number[]): number[] {
  return matrix.map((row) =>
    row.reduce((sum, value, index) => sum + value * vector[index], 0)
  )
}

function dotProduct(left: number[], right: number[]): number {
  return left.reduce((sum, value, index) => sum + value * right[index], 0)
}

function normalize(vector: number[]): number[] | null {
  const norm = Math.hypot(...vector)
  if (!Number.isFinite(norm) || norm < 1e-6) {
    return null
  }

  return vector.map((value) => value / norm)
}

function orthogonalize(vector: number[], basis: number[][]): number[] {
  let next = [...vector]
  for (const base of basis) {
    const projection = dotProduct(next, base)
    next = next.map((value, index) => value - projection * base[index])
  }
  return next
}

function principalEigenpair(matrix: number[][], basis: number[][]): {
  value: number
  vector: number[]
} | null {
  const size = matrix.length
  let vector = Array.from({ length: size }, (_, index) => index + 1)
  vector = orthogonalize(vector, basis)
  const initial = normalize(vector)
  if (!initial) {
    return null
  }

  vector = initial
  for (let iteration = 0; iteration < 80; iteration += 1) {
    let next = multiplyMatrixVector(matrix, vector)
    next = orthogonalize(next, basis)
    const normalized = normalize(next)
    if (!normalized) {
      return null
    }

    const delta = normalized.reduce(
      (sum, value, index) => sum + Math.abs(value - vector[index]),
      0
    )
    vector = normalized
    if (delta < 1e-6) {
      break
    }
  }

  const projected = multiplyMatrixVector(matrix, vector)
  const value = dotProduct(vector, projected)
  if (!Number.isFinite(value) || value <= 1e-6) {
    return null
  }

  return { value, vector }
}

function computeRadialSeed(
  entries: Entry[],
  edges: Edge[],
  centerId: string
): Map<string, { x: number; y: number }> {
  const depthById = buildDepthMap(centerId, edges)
  const entriesByDepth = new Map<number, Entry[]>()

  for (const entry of entries) {
    const depth = depthById.get(entry.id) ?? (entry.id === centerId ? 0 : 1)
    entriesByDepth.set(depth, [...(entriesByDepth.get(depth) ?? []), entry])
  }

  const positions = new Map<string, { x: number; y: number }>()
  
  const sortedDepths = [...entriesByDepth.entries()].sort(([left], [right]) => left - right)
  
  for (const [depth, group] of sortedDepths) {
    if (depth === 0) {
      const entry = group[0]
      if (entry) positions.set(entry.id, { x: 0, y: 0 })
      continue
    }

    const radius = depth * 500
    group.forEach((entry, index) => {
      const angle = (2 * Math.PI * index) / group.length
      positions.set(entry.id, {
        x: Math.cos(angle) * radius,
        y: Math.sin(angle) * radius,
      })
    })
  }

  return positions
}

function refineWithForces(
  entries: Entry[],
  edges: Edge[],
  pairwiseDistances: GraphNodeDistance[] = [],
  initialPositions: Map<string, { x: number; y: number }>,
  centerId: string
): Map<string, { x: number; y: number }> {
  interface SimNode {
    id: string
    x: number
    y: number
    vx: number
    vy: number
  }

  const nodes: SimNode[] = entries.map((e) => {
    const pos = initialPositions.get(e.id) || { x: 0, y: 0 }
    return { id: e.id, x: pos.x, y: pos.y, vx: 0, vy: 0 }
  })

  const nodeById = new Map(nodes.map((n) => [n.id, n]))

  // Simulation constants - Tuned for a wide Graph DB feel
  const ITERATIONS = 120
  const REPULSION_RADIUS = 1000
  const REPULSION_STRENGTH = 40000
  const EDGE_STRENGTH = 0.8
  const EDGE_DISTANCE = 550
  const SIMILARITY_STRENGTH = 0.5
  const CENTER_PULL = 0.04
  const FRICTION = 0.55
  const HORIZONTAL_BIAS = 2.0 // Strong horizontal spread for card readability

  for (let i = 0; i < ITERATIONS; i++) {
    const alpha = 1.0 - i / ITERATIONS 

    // 1. Many-body Repulsion
    for (let u = 0; u < nodes.length; u++) {
      for (let v = u + 1; v < nodes.length; v++) {
        const nodeA = nodes[u]
        const nodeB = nodes[v]
        const dx = (nodeA.x - nodeB.x) / HORIZONTAL_BIAS
        const dy = nodeA.y - nodeB.y
        const dist2 = dx * dx + dy * dy + 1e-6
        const dist = Math.sqrt(dist2)

        if (dist < REPULSION_RADIUS) {
          const force = (REPULSION_STRENGTH * alpha) / dist2
          const fx = (dx / dist) * force * HORIZONTAL_BIAS
          const fy = (dy / dist) * force
          nodeA.vx += fx
          nodeA.vy += fy
          nodeB.vx -= fx
          nodeB.vy -= fy
        }
      }
    }

    // 2. Manual Link Attraction
    for (const edge of edges) {
      const nodeA = nodeById.get(edge.source_id)
      const nodeB = nodeById.get(edge.target_id)
      if (nodeA && nodeB) {
        const dx = nodeB.x - nodeA.x
        const dy = nodeB.y - nodeA.y
        const dist = Math.sqrt(dx * dx + dy * dy) + 1e-6
        const force = (dist - EDGE_DISTANCE) * EDGE_STRENGTH * alpha
        const fx = (dx / dist) * force
        const fy = (dy / dist) * force
        nodeA.vx += fx
        nodeA.vy += fy
        nodeB.vx -= fx
        nodeB.vy -= fy
      }
    }

    // 3. Semantic Similarity Link Attraction
    for (const distInfo of (pairwiseDistances || [])) {
      const nodeA = nodeById.get(distInfo.from_item_id)
      const nodeB = nodeById.get(distInfo.to_item_id)
      if (nodeA && nodeB) {
        const dx = nodeB.x - nodeA.x
        const dy = nodeB.y - nodeA.y
        const dist = Math.sqrt(dx * dx + dy * dy) + 1e-6
        
        const targetDist = 250 + (distInfo.distance * 350) 
        const strength = (1.0 - distInfo.distance) * SIMILARITY_STRENGTH * alpha
        
        const force = (dist - targetDist) * strength
        const fx = (dx / dist) * force
        const fy = (dy / dist) * force
        nodeA.vx += fx
        nodeA.vy += fy
        nodeB.vx -= fx
        nodeB.vy -= fy
      }
    }

    // 4. Center gravity
    const centerNode = nodeById.get(centerId) || nodes[0]
    for (const node of nodes) {
      if (node.id === centerId) continue
      const dx = centerNode.x - node.x
      const dy = centerNode.y - node.y
      node.vx += dx * CENTER_PULL * alpha
      node.vy += dy * CENTER_PULL * alpha
    }

    // 5. Update positions
    for (const node of nodes) {
      node.x += node.vx
      node.y += node.vy
      node.vx *= FRICTION
      node.vy *= FRICTION
    }
  }

  return new Map(nodes.map((n) => [n.id, { x: n.x, y: n.y }]))
}

export function layoutGraphNodes(
  entries: Entry[],
  edges: Edge[],
  pairwiseDistances: GraphNodeDistance[] = [],
  centerId: string,
  selectedNode: string | null
): GraphNode[] {
  const depthById = buildDepthMap(centerId, edges)
  const radialSeed = computeRadialSeed(entries, edges, centerId)
  const refinedPositions = refineWithForces(entries, edges, pairwiseDistances, radialSeed, centerId)

  return entries.map((entry) =>
    createGraphNode(
      entry,
      refinedPositions.get(entry.id) ?? { x: 0, y: 0 },
      depthById.get(entry.id) ?? (entry.id === centerId ? 0 : 1),
      entry.id === centerId,
      selectedNode === entry.id
    )
  )
}

export function convertGraphEdges(
  edges: Edge[],
  pairwiseDistances: GraphNodeDistance[] = [],
  semanticSearchNeighborIds: string[] = [],
  centerId: string | null = null
): FlowEdge[] {
  const flowEdges: FlowEdge[] = edges.map((edge) => {
    const isSimilarity = edge.edge_type === "similarity"
    const strokeOpacity = isSimilarity ? 0.35 : 0.9

    return {
      id: edge.id,
      source: edge.source_id,
      target: edge.target_id,
      label: isSimilarity ? undefined : edge.relationship,
      type: "default",
      animated: isSimilarity,
      markerEnd: {
        type: MarkerType.ArrowClosed,
        width: 14,
        height: 14,
        color: isSimilarity ? "var(--chart-4)" : "var(--primary)",
      },
      style: {
        stroke: isSimilarity ? "var(--chart-4)" : "var(--primary)",
        strokeDasharray: isSimilarity ? "2 4" : undefined, // Dotted for similarity
        strokeOpacity,
        strokeWidth: isSimilarity ? 1.5 : 2,
      },
      labelStyle: { 
        fontSize: 8, 
        fontWeight: "bold",
        fill: "var(--primary)",
        opacity: 0.7
      },
      labelBgStyle: { fill: "var(--background)", opacity: 0.95 },
      labelBgPadding: [4, 2],
      labelBgBorderRadius: 4,
    }
  })

  // Ensure Semantic Search results also have dotted lines to the focal node
  if (centerId) {
    semanticSearchNeighborIds.forEach((neighborId) => {
      if (neighborId === centerId) return
      
      const exists = flowEdges.some(e => 
        (e.source === centerId && e.target === neighborId) ||
        (e.source === neighborId && e.target === centerId)
      )

      if (!exists) {
        flowEdges.push({
          id: `related-${centerId}-${neighborId}`,
          source: centerId,
          target: neighborId,
          type: "default",
          animated: true,
          style: {
            stroke: "var(--chart-4)",
            strokeDasharray: "2 4", // Dotted line
            strokeOpacity: 0.25,
            strokeWidth: 1.25,
          },
        })
      }
    })
  }

  // Ensure ALL semantic similarities from pairwise distances are drawn if nodes are present
  (pairwiseDistances || []).forEach((dist) => {
    const exists = flowEdges.some(e => 
      (e.source === dist.from_item_id && e.target === dist.to_item_id) ||
      (e.source === dist.to_item_id && e.target === dist.from_item_id)
    )

    if (!exists && dist.distance < 0.85) {
      flowEdges.push({
        id: `sim-${dist.from_item_id}-${dist.to_item_id}`,
        source: dist.from_item_id,
        target: dist.to_item_id,
        type: "default",
        animated: true,
        style: {
          stroke: "var(--chart-4)",
          strokeDasharray: "2 4", // Dotted line
          strokeOpacity: 0.3,
          strokeWidth: 1.25,
        },
      })
    }
  })

  return flowEdges
}