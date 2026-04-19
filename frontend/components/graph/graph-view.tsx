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
  ReactFlowProvider,
  useReactFlow,
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import { ChevronsRight, Compass, LoaderCircle, Plus, RotateCcw, ShieldAlert, X, Search, ChevronDown } from "lucide-react"
import { Button } from "@/components/ui/button"
import { ComboButton } from "@/components/ui/combo-button"
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
} from "@/lib/api"
import { cn } from "@/lib/utils"
import { convertGraphEdges, layoutGraphNodes } from "./graph-layout"

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

export function GraphView() {
  return (
    <ReactFlowProvider>
      <GraphViewContent />
    </ReactFlowProvider>
  )
}

function GraphViewContent() {
  const router = useRouter()
  const searchParams = useSearchParams()
  const focusId = searchParams.get("focus")
  const { fitView } = useReactFlow()

  const {
    data: graphStatus,
    isLoading: isGraphStatusLoading,
    error: graphStatusError,
  } = useGraphStatus()
  
  // Search states for different fields
  const [explorerSearch, setExplorerSearch] = useState("")
  const [targetSearch, setTargetSearch] = useState("")
  
  const debouncedExplorerSearch = useDebounce(explorerSearch, 300)
  const debouncedTargetSearch = useDebounce(targetSearch, 300)

  // Use search endpoint for selectors
  const { data: explorerResults, isLoading: isExplorerSearching } = useSearch(debouncedExplorerSearch, undefined, 5)
  const { data: targetResults, isLoading: isTargetSearching } = useSearch(debouncedTargetSearch, undefined, 5)

  const { data: entries } = useItems()
  const { trigger: createEdge, isMutating: isCreating } = useCreateEdge()
  const { trigger: deleteEdge, isMutating: isDeleting } = useDeleteEdge()

  const [nodes, setNodes, onNodesChange] = useNodesState<GraphNode>([])
  const [edges, setEdges, onEdgesChange] = useEdgesState<FlowEdge>([])
  const [selectedNode, setSelectedNode] = useState<string | null>(null)
  const [centerNode, setCenterNode] = useState<string | null>(focusId)
  const [depth, setDepth] = useState(1)
  const [open, setOpen] = useState(false)
  const [openTarget, setOpenTarget] = useState(false)
  const [newEdge, setNewEdge] = useState({
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

  // Fetch semantic neighbors for the center node to ensure they are visible 
  // even if they aren't explicitly connected in the graph DB.
  const centerEntry = useMemo(() => entries?.items.find(e => e.id === centerNode), [entries, centerNode])
  const { data: centerSemanticResults } = useSearch(
    centerEntry?.text ? centerEntry.text : "", 
    undefined, 
    8
  )

  const combinedEntries = useMemo(() => {
    const base = neighborhood?.nodes ?? []
    if (!centerSemanticResults) return base

    const semantic = centerSemanticResults.results.map(r => ({
      id: r.id,
      text: r.text,
      metadata: r.metadata,
      source_id: r.source_id,
      created_at: r.created_at
    }))
    
    const seen = new Set(base.map(e => e.id))
    const merged = [...base]
    for (const s of semantic) {
      if (!seen.has(s.id)) {
        merged.push(s)
        seen.add(s.id)
      }
    }
    return merged
  }, [neighborhood?.nodes, centerSemanticResults])

  const graphEntries = combinedEntries
  const graphEdges = neighborhood?.edges ?? []

  useEffect(() => {
    if (!entries || entries.items.length === 0) {
      return
    }

    if (focusId && entries.items.some((entry) => entry.id === focusId)) {
      setCenterNode(focusId)
      setSelectedNode(focusId)
      setDepth(1)
      return
    }

    setCenterNode((currentCenter) => currentCenter ?? entries.items[0].id)
    setSelectedNode((currentSelected) => currentSelected ?? focusId ?? entries.items[0].id)
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
      selectedNode && combinedEntries.some((entry) => entry.id === selectedNode)
        ? selectedNode
        : centerNode

    const layoutedNodes = layoutGraphNodes(
      combinedEntries,
      neighborhood.edges,
      neighborhood.pairwise_distances,
      centerNode,
      nextSelectedNode
    )

    setNodes(layoutedNodes)
    setEdges(convertGraphEdges(
      neighborhood.edges, 
      neighborhood.pairwise_distances,
      centerSemanticResults?.results.map(r => r.id),
      centerNode
    ))
    setSelectedNode(nextSelectedNode)
    
    // Fit view after layout update
    // We use a small timeout to ensure React Flow has processed the new nodes
    setTimeout(() => {
      fitView({ duration: 800, padding: 0.2 })
    }, 50)
  }, [centerNode, neighborhood, combinedEntries, selectedNode, setEdges, setNodes, fitView])

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
    if (!selectedNode || !newEdge.target || !newEdge.relationship) return

    await createEdge({
      source_id: selectedNode,
      target_id: newEdge.target,
      relationship: newEdge.relationship,
    })

    await mutateNeighborhood()
    setNewEdge({ target: "", relationship: "" })
  }

  const selectedEntry = useMemo(
    () => graphEntries.find((entry) => entry.id === selectedNode),
    [graphEntries, selectedNode]
  )

  const entryOptions = useMemo(
    () =>
      entries?.items
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
          <Panel position="top-center" className="w-full max-w-2xl mt-8">
            <div className="rounded-full bg-background/60 backdrop-blur-3xl border border-primary/5 shadow-2xl p-1.5 flex items-center gap-2 animate-in fade-in slide-in-from-top-4 duration-1000">
              <Popover open={open} onOpenChange={setOpen}>
                <PopoverTrigger asChild>
                  <Button
                    variant="ghost"
                    className="flex-1 justify-start h-10 rounded-full bg-muted/20 hover:bg-muted/40 border-none transition-all font-medium text-sm group px-4"
                  >
                    <Search className="size-4 mr-3 text-muted-foreground group-hover:text-primary transition-colors" />
                    <span className={cn(
                      "truncate",
                      !centerNode && "text-muted-foreground opacity-50"
                    )}>
                      {centerNode || "Search to start exploration..."}
                    </span>
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
                      {isExplorerSearching && (
                        <div className="py-10 flex flex-col items-center gap-2 opacity-40">
                          <LoaderCircle className="size-8 animate-spin" />
                          <p className="text-[10px] font-black uppercase tracking-[0.2em]">Neural Scanning...</p>
                        </div>
                      )}
                      {!isExplorerSearching && !explorerSearch && (
                        <div className="py-10 flex flex-col items-center gap-2 opacity-40">
                          <Search className="size-8" />
                          <p className="text-[10px] font-black uppercase tracking-[0.2em]">Type to Query Memories</p>
                        </div>
                      )}
                      {!isExplorerSearching && explorerSearch && explorerResults?.results.length === 0 && (
                        <CommandEmpty className="py-10 flex flex-col items-center gap-2 opacity-60">
                          <Search className="size-8 mb-2" />
                          <p className="text-sm font-medium">No intelligence fragments found.</p>
                        </CommandEmpty>
                      )}
                      <CommandGroup heading="Neural Search Results">
                        {explorerResults?.results.map((res) => (
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
                                <div className="flex items-center gap-2">
                                  <span className="font-bold text-sm text-primary">{res.id}</span>
                                  <Badge variant="outline" className="text-[8px] px-1.5 py-0 border-primary/20 text-primary/60">
                                    {Math.round(res.score * 100)}% Match
                                  </Badge>
                                </div>
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
 
              <div className="flex items-center gap-1.5 px-1">
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-10 rounded-full hover:bg-primary/5 transition-colors"
                  onClick={handleReset}
                  disabled={!centerNode || depth === 1 || isNeighborhoodLoading}
                  title="Reset Depth"
                >
                  <RotateCcw className="size-4 opacity-40" />
                </Button>
 
                <div className="h-4 w-px bg-muted/20 mx-1" />
 
                <Button
                  className="h-10 px-4 rounded-full font-bold uppercase text-[10px] tracking-widest text-primary hover:bg-primary/5 transition-all"
                  variant="ghost"
                  onClick={handleExpand}
                  disabled={!centerNode || !canExpand || isNeighborhoodLoading}
                >
                  {isNeighborhoodLoading ? (
                    <LoaderCircle className="size-3.5 animate-spin mr-2" />
                  ) : (
                    <ChevronsRight className="size-3.5 mr-2" />
                  )}
                  Expand
                </Button>
              </div>
            </div>
 
            <div className="flex items-center justify-center gap-6 mt-3 text-[9px] font-bold uppercase tracking-[0.2em] text-muted-foreground/30 px-2 animate-in fade-in duration-1000 delay-500 fill-mode-both">
              <span className="flex items-center gap-1.5">
                <div className="size-1 rounded-full bg-primary/20" />
                Depth {depth} / {MAX_DEPTH}
              </span>
              <span className="flex items-center gap-1.5">
                <div className="size-1 rounded-full bg-primary/20" />
                {graphEntries.length} Active Nodes
              </span>
              <span className="flex items-center gap-1.5">
                <div className="size-1 rounded-full bg-primary/20" />
                {enabledGraphStatus.similarity_edge_count} Similarity / {enabledGraphStatus.manual_edge_count} Manual
              </span>
            </div>
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

            <div className="flex flex-col gap-6 pt-8 border-t border-primary/5">
              <div className="flex items-center gap-2">
                <div className="size-1.5 rounded-full bg-primary/40 shadow-[0_0_8px_rgba(var(--primary),0.5)]" />
                <h4 className="text-[10px] font-black uppercase tracking-[0.2em] text-primary/80">Establish Link</h4>
              </div>
              
              <div className="space-y-4">
                <div className="space-y-2">
                  <Label className="text-[9px] font-bold uppercase tracking-widest text-muted-foreground/50 ml-1">Terminal Point</Label>
                  <Popover open={openTarget} onOpenChange={setOpenTarget}>
                    <PopoverTrigger asChild>
                      <Button variant="secondary" role="combobox" className="w-full justify-between h-11 rounded-2xl bg-muted/5 border-muted/10 hover:bg-muted/10 transition-all text-xs font-medium px-4">
                        <span className="truncate">{newEdge.target || "Search target..."}</span>
                        <ChevronDown className="size-3.5 opacity-30" />
                      </Button>
                    </PopoverTrigger>
                    <PopoverContent className="w-[var(--radix-popover-trigger-width)] p-0 rounded-2xl border-muted-foreground/10 shadow-2xl overflow-hidden">
                      <Command className="bg-transparent" loop shouldFilter={false}>
                        <CommandInput 
                          placeholder="Search destination..." 
                          className="h-11 border-none ring-0 focus:ring-0 text-sm" 
                          value={targetSearch}
                          onValueChange={setTargetSearch}
                        />
                        <CommandList> 
                          {isTargetSearching && (
                            <div className="py-6 flex flex-col items-center gap-2 opacity-40">
                              <LoaderCircle className="size-5 animate-spin" />
                              <p className="text-[9px] font-bold uppercase tracking-widest">Scanning...</p>
                            </div>
                          )}
                          {!isTargetSearching && !targetSearch && (
                            <div className="py-6 flex flex-col items-center gap-2 opacity-40">
                               <p className="text-[9px] font-bold uppercase tracking-widest text-center">Type node ID or<br/>text to search</p>
                            </div>
                          )}
                          {!isTargetSearching && targetSearch && targetResults?.results.length === 0 && (
                            <CommandEmpty className="py-6 text-sm opacity-50 text-center">No results.</CommandEmpty>
                          )}
                          <CommandGroup>
                            {targetResults?.results.map((res) => (
                              <CommandItem
                                key={res.id}
                                value={res.id}
                                onSelect={() => {
                                  setNewEdge((prev) => ({ ...prev, target: res.id }))
                                  setOpenTarget(false)
                                }}
                                className="rounded-xl m-1 p-2 cursor-pointer transition-all hover:bg-primary/10 h-auto"
                              >
                                <div className="flex flex-col gap-1 w-full">
                                  <div className="flex items-center justify-between">
                                    <div className="flex items-center gap-1.5">
                                      <span className="font-bold text-[11px] text-primary">{res.id}</span>
                                      <span className="text-[9px] font-bold text-primary/40">{Math.round(res.score * 100)}%</span>
                                    </div>
                                    <span className="text-[8px] font-bold uppercase text-muted-foreground/30">{res.source_id}</span>
                                  </div>
                                  <span className="text-[9px] text-muted-foreground line-clamp-1 italic">{res.text}</span>
                                </div>
                              </CommandItem>
                            ))}
                          </CommandGroup>
                        </CommandList>
                      </Command>
                    </PopoverContent>
                  </Popover>
                </div>

                <div className="space-y-2">
                  <Label className="text-[9px] font-bold uppercase tracking-widest text-muted-foreground/50 ml-1">Relationship</Label>
                  <Input
                    value={newEdge.relationship}
                    onChange={(e) =>
                      setNewEdge((prev) => ({
                        ...prev,
                        relationship: e.target.value,
                      }))
                    }
                    className="h-11 rounded-2xl bg-muted/5 border-muted/10 hover:border-primary/20 transition-all text-xs font-bold px-4"
                    placeholder="e.g. references, contradicts"
                  />
                </div>

                <Button
                  onClick={handleCreateEdge}
                  className="w-full h-11 rounded-2xl font-bold uppercase text-[10px] tracking-widest bg-primary hover:bg-primary/90 shadow-lg shadow-primary/20"
                  disabled={isCreating || !newEdge.target || !newEdge.relationship}
                >
                  {isCreating ? (
                    <LoaderCircle className="size-4 animate-spin text-white" />
                  ) : (
                    <>
                      <Plus className="size-4 mr-2" />
                      Synthesize Link
                    </>
                  )}
                </Button>
              </div>
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
                            <p className="text-[10px] text-muted-foreground/60">
                              {edge.edge_type === "similarity"
                                ? `semantic distance ${edge.distance?.toFixed(3) ?? "n/a"}`
                                : `manual weight ${edge.weight.toFixed(2)}`}
                            </p>
                          </div>
                          {edge.edge_type === "manual" ? (
                            <ComboButton
                              onConfirm={() => handleDeleteEdge(edge.id)}
                              className="size-8 rounded-full opacity-0 group-hover:opacity-100"
                            />
                          ) : null}
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

    </div>
  )
}
