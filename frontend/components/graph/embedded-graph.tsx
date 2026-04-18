"use client"

import { useCallback, useEffect, useState } from "react"
import {
  ReactFlow,
  Background,
  Controls,
  useNodesState,
  useEdgesState,
  type Node,
  type Edge as FlowEdge,
  ConnectionMode,
  MarkerType,
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import { LoaderCircle } from "lucide-react"
import { EntryNode, type EntryNodeData } from "./entry-node"
import {
  useGraphNeighborhood,
  type Entry,
  type Edge,
} from "@/lib/api"

const nodeTypes = {
  entry: EntryNode,
}

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
    adjacency.set(edge.source_id, [...(adjacency.get(edge.source_id) ?? []), edge.target_id])
    adjacency.set(edge.target_id, [...(adjacency.get(edge.target_id) ?? []), edge.source_id])
  }
  const depthById = new Map<string, number>([[centerId, 0]])
  const queue = [centerId]
  while (queue.length > 0) {
    const currentId = queue.shift()!
    const currentDepth = depthById.get(currentId) ?? 0
    for (const neighborId of adjacency.get(currentId) ?? []) {
      if (!depthById.has(neighborId)) {
        depthById.set(neighborId, currentDepth + 1)
        queue.push(neighborId)
      }
    }
  }
  return depthById
}

function layoutNodes(entries: Entry[], edges: Edge[], centerId: string): GraphNode[] {
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
        return [createGraphNode(group[0], { x: 0, y: 0 }, 0, true, true)]
      }
      const radius = depth * 200
      return group.map((entry, index) => {
        const angle = (2 * Math.PI * index) / group.length
        return createGraphNode(entry, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius }, depth, false, false)
      })
    })
}

function convertEdges(edges: Edge[]): FlowEdge[] {
  return edges.map((edge) => ({
    id: edge.id,
    source: edge.source_id,
    target: edge.target_id,
    label: edge.relationship,
    type: "default",
    markerEnd: { type: MarkerType.ArrowClosed, color: "var(--muted-foreground)" },
    style: { stroke: "var(--muted-foreground)" },
    labelStyle: { fontSize: 8, fill: "var(--muted-foreground)" },
    labelBgStyle: { fill: "var(--background)", opacity: 0.8 },
  }))
}

interface EmbeddedGraphProps {
  centerId: string
  onNodeClick?: (id: string) => void
}

export function EmbeddedGraph({ centerId, onNodeClick }: EmbeddedGraphProps) {
  const { data: neighborhood, isLoading } = useGraphNeighborhood(centerId, 1, 30)
  const [nodes, setNodes, onNodesChange] = useNodesState<GraphNode>([])
  const [edges, setEdges, onEdgesChange] = useEdgesState<FlowEdge>([])

  useEffect(() => {
    if (neighborhood) {
      setNodes(layoutNodes(neighborhood.nodes, neighborhood.edges, centerId))
      setEdges(convertEdges(neighborhood.edges))
    }
  }, [neighborhood, centerId, setNodes, setEdges])

  const handleNodeClick = useCallback((_: any, node: Node) => {
    onNodeClick?.(node.id)
  }, [onNodeClick])

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <LoaderCircle className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        nodeTypes={nodeTypes}
        connectionMode={ConnectionMode.Loose}
        fitView
        className="bg-muted/5"
      >
        <Background gap={20} size={1} />
        <Controls />
      </ReactFlow>
    </div>
  )
}
