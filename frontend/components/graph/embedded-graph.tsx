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
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import { LoaderCircle } from "lucide-react"
import { EntryNode, type EntryNodeData } from "./entry-node"
import { useGraphNeighborhood } from "@/lib/api"
import { convertGraphEdges, layoutGraphNodes } from "./graph-layout"

const nodeTypes = {
  entry: EntryNode,
}

type GraphNode = Node<EntryNodeData>

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
      setNodes(
        layoutGraphNodes(
          neighborhood.nodes,
          neighborhood.edges,
          neighborhood.pairwise_distances,
          centerId,
          centerId
        )
      )
      setEdges(convertGraphEdges(neighborhood.edges))
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
