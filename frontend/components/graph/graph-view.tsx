"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  type Node,
  type Edge as FlowEdge,
  ConnectionMode,
  Panel,
  MarkerType,
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import { ChevronsRight, Compass, LoaderCircle, Plus, RotateCcw, ShieldAlert, Trash2, X, Search, ChevronDown } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Badge } from "@/components/ui/badge"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { EntryNode, type EntryNodeData } from "./entry-node"
import {
  useGraphStatus,
  useItems,
  useSearch,
  useGraphNeighborhood,
  useCreateEdge,
  useDeleteEdge,
  type Entry,
  type Edge,
} from "@/lib/api"
import { cn } from "@/lib/utils"

const nodeTypes = {
  entry: EntryNode,
}

type GraphNode = Node<EntryNodeData>

// Helper to debounce values
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value)
  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value)
    }, delay)
    return () => clearTimeout(handler)
  }, [value, delay])
  return debouncedValue
}

const MAX_DEPTH = 3
const GRAPH_LIMIT = 50

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

function layoutNodes(
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

function convertEdges(edges: Edge[]): FlowEdge[] {
  return edges.map((edge) => ({
    id: edge.id,
    source: edge.source_id,
    target: edge.target_id,
    label: edge.relationship,
    type: "default",
    animated: false,
    markerEnd: {
      type: MarkerType.ArrowClosed,
      width: 18,
      height: 18,
      color: "var(--muted-foreground)",
    },
    style: { stroke: "var(--muted-foreground)" },
    labelStyle: { fontSize: 10, fill: "var(--muted-foreground)" },
    labelBgStyle: { fill: "var(--background)", opacity: 0.8 },
  }))
}

export function GraphView() {
  const router = useRouter()
  const searchParams = useSearchParams()
  const focusId = searchParams.get("focus")

  const {
    data: graphStatus,
    isLoading: isGraphStatusLoading,
    error: graphStatusError,
  } = useGraphStatus()
  
  // Search states for different fields
  const [explorerSearch, setExplorerSearch] = useState("")
  const [sourceSearch, setSourceSearch] = useState("")
  const [targetSearch, setTargetSearch] = useState("")
  
  const debouncedExplorerSearch = useDebounce(explorerSearch, 300)
  const debouncedSourceSearch = useDebounce(sourceSearch, 300)
  const debouncedTargetSearch = useDebounce(targetSearch, 300)

  // Use search endpoint for selectors
  const { data: explorerResults } = useSearch(debouncedExplorerSearch, undefined, 5)
  const { data: sourceResults } = useSearch(debouncedSourceSearch, undefined, 5)
  const { data: targetResults } = useSearch(debouncedTargetSearch, undefined, 5)

  const { data: entries } = useItems()
  const { trigger: createEdge, isMutating: isCreating } = useCreateEdge()
  const { trigger: deleteEdge, isMutating: isDeleting } = useDeleteEdge()

  const [nodes, setNodes, onNodesChange] = useNodesState<GraphNode>([])
  const [edges, setEdges, onEdgesChange] = useEdgesState<FlowEdge>([])
  const [selectedNode, setSelectedNode] = useState<string | null>(null)
  const [centerNode, setCenterNode] = useState<string | null>(focusId)
  const [depth, setDepth] = useState(1)
  const [showEdgeForm, setShowEdgeForm] = useState(false)
  const [open, setOpen] = useState(false)
  const [openSource, setOpenSource] = useState(false)
  const [openTarget, setOpenTarget] = useState(false)
  const [newEdge, setNewEdge] = useState({
    source: "",
    target: "",
    relationship: "",
  })

  const {
    data: neighborhood,
    isLoading: isNeighborhoodLoading,
    error: neighborhoodError,
    mutate: mutateNeighborhood,
  } = useGraphNeighborhood(
    graphStatus?.enabled ? centerNode : null,
    depth,
    GRAPH_LIMIT
  )

  const graphEntries = neighborhood?.nodes ?? []
  const graphEdges = neighborhood?.edges ?? []

  useEffect(() => {
    if (!entries || entries.length === 0) {
      return
    }

    if (focusId && entries.some((entry) => entry.id === focusId)) {
      setCenterNode(focusId)
      setSelectedNode(focusId)
      setDepth(1)
      return
    }

    setCenterNode((currentCenter) => currentCenter ?? entries[0].id)
    setSelectedNode((currentSelected) => currentSelected ?? focusId ?? entries[0].id)
  }, [entries, focusId])

  useEffect(() => {
    if (!centerNode || focusId === centerNode) {
      return
    }

    const params = new URLSearchParams(searchParams.toString())
    params.set("focus", centerNode)
    router.replace(`/visualize?${params.toString()}`, { scroll: false })
  }, [centerNode, focusId, router, searchParams])

  useEffect(() => {
    if (!centerNode || !neighborhood) {
      return
    }

    const nextSelectedNode =
      selectedNode && neighborhood.nodes.some((entry) => entry.id === selectedNode)
        ? selectedNode
        : centerNode

    const layoutedNodes = layoutNodes(
      neighborhood.nodes,
      neighborhood.edges,
      centerNode,
      nextSelectedNode
    )

    setNodes(layoutedNodes)
    setEdges(convertEdges(neighborhood.edges))
    setSelectedNode(nextSelectedNode)
  }, [centerNode, neighborhood, selectedNode, setEdges, setNodes])

  const handleCenterNodeChange = useCallback((nodeId: string) => {
    setCenterNode(nodeId)
    setSelectedNode(nodeId)
    setDepth(1)
  }, [])

  const handleExpand = useCallback(() => {
    setDepth((currentDepth) => Math.min(MAX_DEPTH, currentDepth + 1))
  }, [])

  const handleReset = useCallback(() => {
    setDepth(1)
  }, [])

  const handleCenterSelectedNode = useCallback(() => {
    if (!selectedNode) {
      return
    }

    handleCenterNodeChange(selectedNode)
  }, [handleCenterNodeChange, selectedNode])

  const handleDeleteEdge = useCallback(
    async (edgeId: string) => {
      if (!confirm("Delete this edge?")) {
        return
      }

      await deleteEdge(edgeId)
      await mutateNeighborhood()
    },
    [deleteEdge, mutateNeighborhood]
  )

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: GraphNode) => {
      setSelectedNode(node.id)
      setNodes((nds) =>
        nds.map((n) => ({
          ...n,
          data: { ...n.data, isSelected: n.id === node.id },
        }))
      )
    },
    [setNodes]
  )

  const onPaneClick = useCallback(() => {
    setSelectedNode(null)
    setNodes((nds) =>
      nds.map((n) => ({
        ...n,
        data: { ...n.data, isSelected: false },
      }))
    )
  }, [setNodes])

  const handleNodeDoubleClick = useCallback(
    (_: React.MouseEvent, node: GraphNode) => {
      router.push(`/entries/${encodeURIComponent(node.id)}`)
    },
    [router]
  )

  const handleCreateEdge = async () => {
    if (!newEdge.source || !newEdge.target || !newEdge.relationship) return

    await createEdge({
      source_id: newEdge.source,
      target_id: newEdge.target,
      relationship: newEdge.relationship,
    })

    await mutateNeighborhood()
    setShowEdgeForm(false)
    setNewEdge({ source: "", target: "", relationship: "" })
  }

  const selectedEntry = useMemo(
    () => graphEntries.find((entry) => entry.id === selectedNode),
    [graphEntries, selectedNode]
  )

  const entryOptions = useMemo(
    () =>
      entries
        ?.filter((entry) => entry.id.trim().length > 0)
        .map((entry) => ({ value: entry.id, label: entry.id })) ?? [],
    [entries]
  )

  const selectedEntryEdges = useMemo(
    () =>
      selectedNode
        ? graphEdges.filter(
            (edge) =>
              edge.source_id === selectedNode || edge.target_id === selectedNode
          )
        : [],
    [graphEdges, selectedNode]
  )

  const canExpand = depth < MAX_DEPTH && graphEntries.length < GRAPH_LIMIT

  if (isGraphStatusLoading) {
    return (
      <div className="flex h-[calc(100vh-3.5rem)] items-center justify-center gap-2 text-sm text-muted-foreground">
        <LoaderCircle className="size-4 animate-spin" />
        Checking graph status...
      </div>
    )
  }

  if (graphStatusError) {
    return (
      <div className="mx-auto flex h-[calc(100vh-3.5rem)] max-w-xl items-center justify-center p-6">
        <Card className="w-full">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ShieldAlert className="size-5 text-destructive" />
              Graph status unavailable
            </CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            {graphStatusError instanceof Error
              ? graphStatusError.message
              : "The frontend could not determine whether graph support is available."}
          </CardContent>
        </Card>
      </div>
    )
  }

  if (!graphStatus?.enabled) {
    const itemCount = graphStatus?.item_count ?? 0
    const edgeCount = graphStatus?.edge_count ?? 0

    return (
      <div className="mx-auto flex h-[calc(100vh-3.5rem)] max-w-xl items-center justify-center p-6">
        <Card className="w-full">
          <CardHeader>
            <CardTitle>Graph support is disabled</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm text-muted-foreground">
            <p>
              Start the service with graph support enabled before opening the graph explorer.
            </p>
            <div className="rounded-md border bg-muted/40 p-3 font-mono text-xs text-foreground">
              make RAG_GRAPH_ENABLED=true RAG_GRAPH_BUILD_ON_STARTUP=true run
            </div>
            <p>
              Current dataset: {itemCount} items, {edgeCount} edges.
            </p>
          </CardContent>
        </Card>
      </div>
    )
  }

  const enabledGraphStatus = graphStatus

  return (
    <div className="flex h-[calc(100vh-3.5rem)]">
      <div className="flex-1">
        <ReactFlow<GraphNode, FlowEdge>
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeClick={onNodeClick}
          onNodeDoubleClick={handleNodeDoubleClick}
          onPaneClick={onPaneClick}
          nodeTypes={nodeTypes}
          connectionMode={ConnectionMode.Loose}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          className="bg-background"
        >
          <Background gap={16} size={1} />
          <Controls className="!bg-card !border !shadow-sm" />
          <MiniMap
            nodeStrokeWidth={3}
            className="!bg-card !border !shadow-sm"
          />
          <Panel position="top-center" className="w-full max-w-lg mt-8">
            <Card className="shadow-2xl border-primary/10 rounded-[28px] bg-background/80 backdrop-blur-xl overflow-hidden animate-in fade-in slide-in-from-top-4 duration-700">
              <CardContent className="flex flex-col gap-4 p-5">
                <div className="flex items-center justify-between gap-4">
                  <div className="space-y-1">
                    <p className="text-[10px] font-bold uppercase tracking-[0.2em] text-primary/60">Neural Network Explorer</p>
                    <p className="text-sm text-foreground font-semibold">
                      Visualize Knowledge Connections
                    </p>
                  </div>
                  <Button size="icon" variant="ghost" className="rounded-full size-10 hover:bg-primary/5 text-primary" onClick={() => setShowEdgeForm(true)}>
                    <Plus className="size-5" />
                  </Button>
                </div>

                <div className="flex flex-col gap-3">
                  <Popover open={open} onOpenChange={setOpen}>
                    <PopoverTrigger asChild>
                      <Button
                        variant="secondary"
                        role="combobox"
                        aria-expanded={open}
                        className="w-full justify-between h-14 rounded-2xl bg-muted/30 border-none hover:bg-muted/50 transition-all font-medium text-base shadow-sm group"
                      >
                        <div className="flex items-center gap-3 truncate">
                          <Search className="size-5 text-muted-foreground group-hover:text-primary transition-colors" />
                          <span className={cn(
                            "truncate",
                            !centerNode && "text-muted-foreground opacity-50"
                          )}>
                            {centerNode || "Search to start exploration..."}
                          </span>
                        </div>
                        <RotateCcw className="ml-2 h-4 w-4 shrink-0 opacity-30 group-hover:opacity-100 transition-opacity" />
                      </Button>
                    </PopoverTrigger>
                    <PopoverContent className="w-[var(--radix-popover-trigger-width)] p-0 rounded-2xl border-muted-foreground/10 shadow-2xl overflow-hidden" align="center">
                      <Command className="bg-transparent" loop shouldFilter={false}>
                        <CommandInput 
                          placeholder="Type an entry ID or keywords..." 
                          className="h-14 border-none ring-0 focus:ring-0 text-base"
                          value={explorerSearch}
                          onValueChange={setExplorerSearch}
                        />
                        <CommandList>
                          <CommandEmpty className="py-10 flex flex-col items-center gap-2 opacity-60">
                            <Search className="size-8" />
                            <p className="text-sm font-medium">No results found on server.</p>
                          </CommandEmpty>
                          <CommandGroup heading="Neural Search Results">
                            {explorerResults?.map((res) => (
                              <CommandItem
                                key={res.id}
                                value={res.id}
                                onSelect={(currentValue) => {
                                  handleCenterNodeChange(res.id)
                                  setOpen(false)
                                }}
                                className="rounded-xl m-1.5 cursor-pointer transition-all hover:bg-primary/10 aria-selected:bg-primary/10 p-3 h-auto"
                              >
                                <div className="flex flex-col gap-1 w-full">
                                  <div className="flex items-center justify-between">
                                    <span className="font-bold text-sm text-primary">{res.id}</span>
                                    <span className="text-[10px] font-bold uppercase text-muted-foreground/50">{res.source_id}</span>
                                  </div>
                                  <span className="text-[11px] text-muted-foreground leading-relaxed line-clamp-2 italic">
                                    {res.text}
                                  </span>
                                </div>
                              </CommandItem>
                            ))}
                          </CommandGroup>
                        </CommandList>
                      </Command>
                    </PopoverContent>
                  </Popover>

                  <div className="flex gap-2.5">
                    <Button
                      variant="ghost"
                      className="flex-1 h-11 rounded-2xl font-bold uppercase text-[10px] tracking-widest hover:bg-muted/50 transition-colors"
                      onClick={handleReset}
                      disabled={!centerNode || depth === 1 || isNeighborhoodLoading}
                    >
                      <RotateCcw className="size-3.5 mr-2 opacity-50" />
                      Reset Depth
                    </Button>

                    <Button
                      className="flex-1 h-11 rounded-2xl font-bold uppercase text-[10px] tracking-widest shadow-xl shadow-primary/20 bg-primary hover:bg-primary/90 hover:scale-[1.02] transition-all"
                      onClick={handleExpand}
                      disabled={!centerNode || !canExpand || isNeighborhoodLoading}
                    >
                      {isNeighborhoodLoading ? (
                        <LoaderCircle className="size-4 animate-spin mr-2" />
                      ) : (
                        <ChevronsRight className="size-4 mr-2" />
                      )}
                      Expand Context
                    </Button>
                  </div>
                </div>

                <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground/40 px-2">
                  <span>Depth {depth} / {MAX_DEPTH}</span>
                  <span className="flex items-center gap-2">
                    <div className="size-1.5 rounded-full bg-primary/20" />
                    {graphEntries.length} Loaded Nodes
                  </span>
                </div>
              </CardContent>
            </Card>
          </Panel>
        </ReactFlow>
      </div>

      {/* Sidebar */}
      <aside className="w-80 border-l bg-card/10 backdrop-blur-md p-6 overflow-y-auto scrollbar-thin">
        {neighborhoodError ? (
          <div className="rounded-2xl border border-destructive/20 bg-destructive/5 p-4 text-xs font-semibold text-destructive animate-in slide-in-from-right-4 duration-500">
            {neighborhoodError instanceof Error
              ? neighborhoodError.message
              : "Failed to load graph neighborhood."}
          </div>
        ) : isNeighborhoodLoading && graphEntries.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-4 text-muted-foreground opacity-30">
            <LoaderCircle className="size-10 animate-spin" />
            <p className="text-xs font-bold uppercase tracking-widest">Hydrating Network...</p>
          </div>
        ) : selectedEntry ? (
          <div className="flex flex-col gap-8 animate-in fade-in slide-in-from-right-8 duration-700">
            <div className="flex items-center justify-between">
              <h3 className="text-[10px] font-bold uppercase tracking-[0.2em] text-primary">Metadata Insight</h3>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 rounded-full hover:bg-primary/5"
                onClick={() => {
                  setSelectedNode(null)
                  onPaneClick()
                }}
              >
                <X className="size-4" />
              </Button>
            </div>
            
            <div className="space-y-2">
              <h4 className="text-xl font-bold tracking-tight">{selectedEntry.id}</h4>
              <Badge variant="indigo" className="text-[9px] uppercase font-bold px-2 py-0.5 tracking-widest">
                {selectedEntry.source_id}
              </Badge>
            </div>

            <div className="bg-muted/10 rounded-2xl border border-muted-foreground/10 p-5 shadow-inner">
              <p className="text-sm leading-relaxed text-muted-foreground italic">
                {selectedEntry.text}
              </p>
            </div>
            
            <div className="flex gap-2">
              <Button
                className="flex-1 h-11 rounded-2xl font-bold uppercase text-[10px] tracking-widest bg-secondary text-secondary-foreground hover:bg-secondary/80 shadow-md"
                onClick={() =>
                  router.push(`/entries/${encodeURIComponent(selectedEntry.id)}`)
                }
              >
                View Hub
              </Button>
              <Button
                className="flex-1 h-11 rounded-2xl font-bold uppercase text-[10px] tracking-widest shadow-xl shadow-primary/10"
                onClick={handleCenterSelectedNode}
                disabled={selectedEntry.id === centerNode}
              >
                <Compass className="size-4 mr-2" />
                Focus
              </Button>
            </div>

            <div className="flex flex-col gap-4">
              <h4 className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">Network Relations</h4>
              {selectedEntryEdges.length === 0 ? (
                <p className="text-xs text-muted-foreground/50 font-medium italic">
                  No visible connections at current abstraction level.
                </p>
              ) : (
                <div className="space-y-3">
                  {selectedEntryEdges.map((edge) => {
                    const neighborId =
                      edge.source_id === selectedEntry.id
                        ? edge.target_id
                        : edge.source_id

                    return (
                      <div
                        key={edge.id}
                        className="group rounded-2xl border border-muted-foreground/10 bg-muted/5 p-4 transition-all hover:border-primary/30 hover:shadow-lg"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0 space-y-1">
                            <p className="truncate font-bold text-sm text-foreground/80">{neighborId}</p>
                            <Badge variant="outline" className="text-[8px] font-black uppercase py-0 px-1 border-primary/20 text-primary/60">
                              {edge.relationship}
                            </Badge>
                          </div>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-8 rounded-full opacity-0 group-hover:opacity-100 transition-opacity text-destructive hover:bg-destructive/5"
                            onClick={() => void handleDeleteEdge(edge.id)}
                            disabled={isDeleting}
                          >
                            <Trash2 className="size-4" />
                          </Button>
                        </div>
                      </div>
                    )
                  })}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex h-full flex-col items-center justify-center text-center gap-4 opacity-30">
            <Compass className="size-12" />
            <div className="space-y-2">
              <p className="text-xs font-bold uppercase tracking-widest">Selection Terminal</p>
              <p className="text-[10px] font-medium max-w-[180px]">Interact with the manifold to inspect neural data points.</p>
            </div>
          </div>
        )}
      </aside>

      {/* Edge Creation Modal */}
      {showEdgeForm && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-background/60 backdrop-blur-md animate-in fade-in duration-300">
          <Card className="w-[400px] border-primary/10 rounded-[32px] shadow-[0_0_100px_-20px_rgba(0,0,0,0.3)] bg-background">
            <CardHeader className="pb-4">
              <div className="flex items-center justify-between">
                <CardTitle className="text-xl font-black uppercase tracking-widest text-primary/80">Establish Link</CardTitle>
                <Button
                  variant="ghost"
                  size="icon"
                  className="rounded-full size-10"
                  onClick={() => setShowEdgeForm(false)}
                >
                  <X className="size-5" />
                </Button>
              </div>
            </CardHeader>
            <CardContent className="flex flex-col gap-6 p-6 pt-0">
              <div className="flex flex-col gap-2">
                <Label className="text-[10px] items-center flex gap-2 font-bold uppercase tracking-widest text-muted-foreground/60 mb-1">
                  <div className="size-1 rounded-full bg-primary" />
                  Origin Point
                </Label>
                <Popover open={openSource} onOpenChange={setOpenSource}>
                  <PopoverTrigger asChild>
                    <Button variant="secondary" role="combobox" className="w-full justify-between h-12 rounded-xl bg-muted/30 border-none shadow-inner">
                      <span className="truncate">{newEdge.source || "Select source node..."}</span>
                      <ChevronDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-[var(--radix-popover-trigger-width)] p-0 rounded-2xl border-muted-foreground/10 shadow-2xl overflow-hidden">
                    <Command className="bg-transparent" loop shouldFilter={false}>
                      <CommandInput 
                        placeholder="Search origin..." 
                        className="h-12 border-none ring-0 focus:ring-0" 
                        value={sourceSearch}
                        onValueChange={setSourceSearch}
                      />
                      <CommandList>
                        <CommandEmpty className="py-6 text-sm opacity-50">No origins found.</CommandEmpty>
                        <CommandGroup>
                          {sourceResults?.map((res) => (
                            <CommandItem
                              key={res.id}
                              value={res.id}
                              onSelect={() => {
                                setNewEdge((prev) => ({ ...prev, source: res.id }))
                                setOpenSource(false)
                              }}
                              className="rounded-lg m-1 p-2"
                            >
                              <div className="flex flex-col">
                                <span className="font-bold text-xs">{res.id}</span>
                                <span className="text-[9px] text-muted-foreground line-clamp-1">{res.text}</span>
                              </div>
                            </CommandItem>
                          ))}
                        </CommandGroup>
                      </CommandList>
                    </Command>
                  </PopoverContent>
                </Popover>
              </div>

              <div className="flex flex-col gap-2">
                <Label className="text-[10px] items-center flex gap-2 font-bold uppercase tracking-widest text-muted-foreground/60 mb-1">
                   <div className="size-1 rounded-full bg-primary" />
                   Terminal Point
                </Label>
                <Popover open={openTarget} onOpenChange={setOpenTarget}>
                  <PopoverTrigger asChild>
                    <Button variant="secondary" role="combobox" className="w-full justify-between h-12 rounded-xl bg-muted/30 border-none shadow-inner">
                      <span className="truncate">{newEdge.target || "Select target node..."}</span>
                      <ChevronDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-[var(--radix-popover-trigger-width)] p-0 rounded-2xl border-muted-foreground/10 shadow-2xl overflow-hidden">
                    <Command className="bg-transparent" loop shouldFilter={false}>
                      <CommandInput 
                        placeholder="Search terminal..." 
                        className="h-12 border-none ring-0 focus:ring-0" 
                        value={targetSearch}
                        onValueChange={setTargetSearch}
                      />
                      <CommandList>
                        <CommandEmpty className="py-6 text-sm opacity-50">No terminals found.</CommandEmpty>
                        <CommandGroup>
                          {targetResults?.map((res) => (
                            <CommandItem
                              key={res.id}
                              value={res.id}
                              onSelect={() => {
                                setNewEdge((prev) => ({ ...prev, target: res.id }))
                                setOpenTarget(false)
                              }}
                              className="rounded-lg m-1 p-2"
                            >
                              <div className="flex flex-col">
                                <span className="font-bold text-xs">{res.id}</span>
                                <span className="text-[9px] text-muted-foreground line-clamp-1">{res.text}</span>
                              </div>
                            </CommandItem>
                          ))}
                        </CommandGroup>
                      </CommandList>
                    </Command>
                  </PopoverContent>
                </Popover>
              </div>

              <div className="flex flex-col gap-2">
                <Label className="text-[10px] items-center flex gap-2 font-bold uppercase tracking-widest text-muted-foreground/60 mb-1">
                   <div className="size-1 rounded-full bg-primary" />
                   Connection Log
                </Label>
                <Input
                  value={newEdge.relationship}
                  onChange={(e) =>
                    setNewEdge((prev) => ({
                      ...prev,
                      relationship: e.target.value,
                    }))
                  }
                  className="h-12 rounded-xl bg-muted/30 border-none shadow-inner placeholder:text-muted-foreground/40 font-bold text-sm px-4"
                  placeholder="e.g. references, contradicts, correlates"
                />
              </div>

              <div className="flex justify-end gap-3 mt-4">
                <Button variant="ghost" className="rounded-2xl font-bold uppercase text-[10px] tracking-widest px-6" onClick={() => setShowEdgeForm(false)}>
                  Abort
                </Button>
                <Button
                  onClick={handleCreateEdge}
                  className="rounded-2xl font-bold uppercase text-[10px] tracking-widest px-6 shadow-xl shadow-primary/20"
                  disabled={
                    isCreating ||
                    !newEdge.source ||
                    !newEdge.target ||
                    !newEdge.relationship
                  }
                >
                  {isCreating ? "Synthesizing..." : "Initialize Link"}
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  )
}
