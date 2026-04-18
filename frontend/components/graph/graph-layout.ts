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

function buildDistanceMatrix(
  entries: Entry[],
  pairwiseDistances: GraphNodeDistance[]
): number[][] | null {
  if (entries.length < 2 || pairwiseDistances.length === 0) {
    return null
  }

  const ids = entries.map((entry) => entry.id)
  const indexById = new Map(ids.map((id, index) => [id, index]))
  const matrix = Array.from({ length: ids.length }, () =>
    Array.from({ length: ids.length }, () => 0)
  )
  const observedDistances: number[] = []

  for (const pair of pairwiseDistances) {
    const left = indexById.get(pair.from_item_id)
    const right = indexById.get(pair.to_item_id)
    if (left === undefined || right === undefined) {
      continue
    }

    matrix[left][right] = pair.distance
    matrix[right][left] = pair.distance
    observedDistances.push(pair.distance)
  }

  if (observedDistances.length === 0) {
    return null
  }

  const fallbackDistance = Math.max(...observedDistances) * 1.15
  for (let row = 0; row < matrix.length; row += 1) {
    for (let column = 0; column < matrix.length; column += 1) {
      if (row === column) {
        matrix[row][column] = 0
      } else if (matrix[row][column] <= 0) {
        matrix[row][column] = fallbackDistance
      }
    }
  }

  return matrix
}

function computeDistanceAwarePositions(
  entries: Entry[],
  pairwiseDistances: GraphNodeDistance[],
  centerId: string
): Map<string, { x: number; y: number }> | null {
  const distanceMatrix = buildDistanceMatrix(entries, pairwiseDistances)
  if (!distanceMatrix) {
    return null
  }

  const size = distanceMatrix.length
  const squaredMeansByRow = distanceMatrix.map(
    (row) => row.reduce((sum, value) => sum + value * value, 0) / size
  )
  const grandMean = squaredMeansByRow.reduce((sum, value) => sum + value, 0) / size
  const centered = Array.from({ length: size }, (_, row) =>
    Array.from({ length: size }, (_, column) => {
      const squaredDistance = distanceMatrix[row][column] * distanceMatrix[row][column]
      return -0.5 * (
        squaredDistance - squaredMeansByRow[row] - squaredMeansByRow[column] + grandMean
      )
    })
  )

  const first = principalEigenpair(centered, [])
  const second = first ? principalEigenpair(centered, [first.vector]) : null
  if (!first) {
    return null
  }

  const xScale = Math.sqrt(first.value)
  const yScale = second ? Math.sqrt(second.value) : 0
  const rawPositions = entries.map((entry, index) => ({
    id: entry.id,
    x: first.vector[index] * xScale,
    y: second ? second.vector[index] * yScale : 0,
  }))

  const centerPosition =
    rawPositions.find((position) => position.id === centerId) ?? rawPositions[0]
  const shifted = rawPositions.map((position) => ({
    ...position,
    x: position.x - centerPosition.x,
    y: position.y - centerPosition.y,
  }))

  const maxAbs = shifted.reduce(
    (max, position) => Math.max(max, Math.abs(position.x), Math.abs(position.y)),
    0
  )
  const scale = maxAbs > 0 ? 360 / maxAbs : 1

  return new Map(
    shifted.map((position) => [
      position.id,
      {
        x: position.x * scale,
        y: position.y * scale,
      },
    ])
  )
}

export function layoutGraphNodes(
  entries: Entry[],
  edges: Edge[],
  pairwiseDistances: GraphNodeDistance[],
  centerId: string,
  selectedNode: string | null
): GraphNode[] {
  const depthById = buildDepthMap(centerId, edges)
  const positions = computeDistanceAwarePositions(entries, pairwiseDistances, centerId)
  if (!positions) {
    return fallbackLayout(entries, edges, centerId, selectedNode)
  }

  return entries.map((entry) =>
    createGraphNode(
      entry,
      positions.get(entry.id) ?? { x: 0, y: 0 },
      depthById.get(entry.id) ?? (entry.id === centerId ? 0 : 1),
      entry.id === centerId,
      selectedNode === entry.id
    )
  )
}

export function convertGraphEdges(edges: Edge[]): FlowEdge[] {
  return edges.map((edge) => {
    const isSimilarity = edge.edge_type === "similarity"
    const strokeOpacity = isSimilarity
      ? Math.max(0.2, 1 - (edge.distance ?? 0.75))
      : 0.95

    return {
      id: edge.id,
      source: edge.source_id,
      target: edge.target_id,
      label: isSimilarity ? undefined : edge.relationship,
      type: "default",
      animated: false,
      markerEnd: edge.directed
        ? {
            type: MarkerType.ArrowClosed,
            width: 18,
            height: 18,
            color: isSimilarity ? "var(--chart-4)" : "var(--primary)",
          }
        : undefined,
      style: {
        stroke: isSimilarity ? "var(--chart-4)" : "var(--primary)",
        strokeDasharray: isSimilarity ? "6 4" : undefined,
        strokeOpacity,
        strokeWidth: isSimilarity ? 1 + edge.weight * 2 : 1.75 + edge.weight,
      },
      labelStyle: { fontSize: 10, fill: "var(--foreground)" },
      labelBgStyle: { fill: "var(--background)", opacity: 0.85 },
    }
  })
}