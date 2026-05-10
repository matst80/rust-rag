"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import { useTheme } from "next-themes"
import dynamic from "next/dynamic"
import { darkTheme, lightTheme } from "reagraph"
import type { GraphCanvasProps, GraphEdge, GraphNode, InternalGraphEdge, InternalGraphNode, Theme } from "reagraph"
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
import {
  useGraphStatus,
  useItems,
  useSearch,
  useGraphNeighborhood,
  useCreateEdge,
  useDeleteEdge,
} from "@/lib/api"
import { cn } from "@/lib/utils"
import { computeCommunities, getNodeTitle } from "./clusters"

const GraphCanvas = dynamic(
  () => import("reagraph").then((m) => m.GraphCanvas),
  { ssr: false }
) as unknown as React.ComponentType<GraphCanvasProps>

function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value)
  useEffect(() => {
    const handler = setTimeout(() => setDebouncedValue(value), delay)
    return () => clearTimeout(handler)
  }, [value, delay])
  return debouncedValue
}

const MAX_DEPTH = 4
const GRAPH_LIMIT = 80
const SIMILARITY_DRAW_CUTOFF = 0.85

export function GraphView() {
  return <GraphViewContent />
}

function buildReagraphTheme(isDark: boolean): Theme {
  const base = isDark ? darkTheme : lightTheme
  const bg = isDark ? "#0a0a0a" : "#fafafa"
  const labelColor = isDark ? "#e2e8f0" : "#1e293b"
  const subLabelColor = isDark ? "#94a3b8" : "#64748b"
  const clusterStroke = isDark ? "#475569" : "#cbd5e1"
  const clusterLabel = isDark ? "#94a3b8" : "#64748b"

  return {
    ...base,
    canvas: { background: bg, fog: bg },
    node: {
      ...base.node,
      label: {
        ...base.node.label,
        color: labelColor,
        stroke: bg,
        activeColor: isDark ? "#a5b4fc" : "#4338ca",
      },
      subLabel: {
        ...(base.node.subLabel ?? { color: subLabelColor, activeColor: subLabelColor }),
        color: subLabelColor,
        stroke: bg,
        activeColor: isDark ? "#a5b4fc" : "#4338ca",
      },
    },
    edge: {
      ...base.edge,
      label: {
        ...base.edge.label,
        color: subLabelColor,
        stroke: bg,
        activeColor: isDark ? "#a5b4fc" : "#4338ca",
      },
    },
    cluster: {
      // Drop fill entirely so only a thin colored ring remains.
      stroke: clusterStroke,
      opacity: 0.45,
      selectedOpacity: 0.7,
      inactiveOpacity: 0.15,
      label: {
        color: clusterLabel,
        stroke: bg,
        fontSize: 14,
      },
    },
  }
}

function GraphViewContent() {
  const router = useRouter()
  const searchParams = useSearchParams()
  const focusId = searchParams.get("focus")
  const { resolvedTheme } = useTheme()
  const isDark = resolvedTheme === "dark"
  const reagraphTheme = useMemo(() => buildReagraphTheme(isDark), [isDark])

  const {
    data: graphStatus,
    isLoading: isGraphStatusLoading,
    error: graphStatusError,
  } = useGraphStatus()

  const [explorerSearch, setExplorerSearch] = useState("")
  const [targetSearch, setTargetSearch] = useState("")
  const debouncedExplorerSearch = useDebounce(explorerSearch, 300)
  const debouncedTargetSearch = useDebounce(targetSearch, 300)

  const { data: explorerResults, isLoading: isExplorerSearching } = useSearch(debouncedExplorerSearch, undefined, true, 5)
  const { data: targetResults, isLoading: isTargetSearching } = useSearch(debouncedTargetSearch, undefined, true, 5)

  const { data: entries } = useItems()
  const { trigger: createEdge, isMutating: isCreating } = useCreateEdge()
  const { trigger: deleteEdge } = useDeleteEdge()

  const [selectedNode, setSelectedNode] = useState<string | null>(null)
  const [centerNode, setCenterNode] = useState<string | null>(focusId)
  const [depth, setDepth] = useState(1)
  const [open, setOpen] = useState(false)
  const [openTarget, setOpenTarget] = useState(false)
  const [newEdge, setNewEdge] = useState({ target: "", relationship: "" })
  const [detailLevel, setDetailLevel] = useState<"coarse" | "medium" | "fine">("medium")

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
  const pairwise = neighborhood?.pairwise_distances ?? []

  useEffect(() => {
    if (!entries || entries.items.length === 0) return
    if (focusId && entries.items.some((entry) => entry.id === focusId)) {
      setCenterNode(focusId)
      setSelectedNode(focusId)
      setDepth(1)
      return
    }
    setCenterNode((current) => current ?? entries.items[0].id)
    setSelectedNode((current) => current ?? focusId ?? entries.items[0].id)
  }, [entries, focusId])

  useEffect(() => {
    if (!centerNode || focusId === centerNode) return
    const params = new URLSearchParams(searchParams.toString())
    params.set("focus", centerNode)
    router.replace(`/visualize?${params.toString()}`, { scroll: false })
  }, [centerNode, focusId, router, searchParams])

  const resolution = detailLevel === "fine" ? 2.5 : detailLevel === "coarse" ? 0.6 : 1.2
  const clusters = useMemo(
    () => computeCommunities(graphEntries, graphEdges, pairwise, { resolution }),
    [graphEntries, graphEdges, pairwise, resolution]
  )

  const reagraphNodes = useMemo<GraphNode[]>(() => {
    return graphEntries.map((entry) => {
      const cid = clusters.byNode.get(entry.id) ?? "unknown"
      const fill = clusters.colorByCluster.get(cid) ?? "#64748b"
      const isCenter = entry.id === centerNode
      return {
        id: entry.id,
        label: getNodeTitle(entry),
        subLabel: entry.source_id,
        fill,
        size: isCenter ? 18 : 10,
        cluster: cid,
        data: {
          sourceId: entry.source_id,
          text: entry.text,
          cluster: cid,
          rawId: entry.id,
        },
      }
    })
  }, [graphEntries, clusters, centerNode])

  const reagraphEdges = useMemo<GraphEdge[]>(() => {
    const out: GraphEdge[] = []
    const seen = new Set<string>()

    for (const edge of graphEdges) {
      const isSimilarity = edge.edge_type === "similarity"
      const key = `${edge.source_id}::${edge.target_id}`
      seen.add(key)
      seen.add(`${edge.target_id}::${edge.source_id}`)
      out.push({
        id: edge.id,
        source: edge.source_id,
        target: edge.target_id,
        label: isSimilarity ? undefined : edge.relationship,
        size: isSimilarity ? 1 : 2.5,
        data: {
          edgeType: edge.edge_type,
          relationship: edge.relationship,
          weight: edge.weight,
          distance: edge.distance,
        },
      })
    }

    // Synthesize similarity edges from pairwise distances when not already present.
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
        data: {
          edgeType: "similarity",
          distance: d.distance,
        },
      })
    }

    return out
  }, [graphEdges, pairwise])

  const handleCenterNodeChange = useCallback((nodeId: string) => {
    setCenterNode(nodeId)
    setSelectedNode(nodeId)
    setDepth(1)
  }, [])

  const handleExpand = useCallback(() => {
    setDepth((d) => Math.min(MAX_DEPTH, d + 1))
  }, [])

  const handleReset = useCallback(() => setDepth(1), [])

  const handleCenterSelectedNode = useCallback(() => {
    if (selectedNode) handleCenterNodeChange(selectedNode)
  }, [handleCenterNodeChange, selectedNode])

  const handleDeleteEdge = useCallback(
    async (edgeId: string) => {
      await deleteEdge(edgeId)
      await mutateNeighborhood()
    },
    [deleteEdge, mutateNeighborhood]
  )

  const onNodeClick = useCallback((node: InternalGraphNode) => {
    setSelectedNode(node.id)
  }, [])

  const onNodeDoubleClick = useCallback(
    (node: InternalGraphNode) => {
      router.push(`/entries/${encodeURIComponent(node.id)}`)
    },
    [router]
  )

  const onCanvasClick = useCallback(() => setSelectedNode(null), [])

  const onEdgeClick = useCallback(
    (edge: InternalGraphEdge) => {
      const data = edge.data as { edgeType?: string } | undefined
      if (data?.edgeType !== "manual") return
      if (typeof window !== "undefined" && !window.confirm("Delete this manual edge?")) return
      handleDeleteEdge(edge.id)
    },
    [handleDeleteEdge]
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

  const selectedEntryEdges = useMemo(
    () =>
      selectedNode
        ? graphEdges.filter(
            (edge) => edge.source_id === selectedNode || edge.target_id === selectedNode
          )
        : [],
    [graphEdges, selectedNode]
  )

  const clusterCount = clusters.colorByCluster.size
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
            <p>Start the service with graph support enabled before opening the graph explorer.</p>
            <div className="rounded-md border bg-muted/40 p-3 font-mono text-xs text-foreground">
              make RAG_GRAPH_ENABLED=true RAG_GRAPH_BUILD_ON_STARTUP=true run
            </div>
            <p>Current dataset: {itemCount} items, {edgeCount} edges.</p>
          </CardContent>
        </Card>
      </div>
    )
  }

  const enabledGraphStatus = graphStatus

  return (
    <div className="flex h-[calc(100vh-3.5rem)]">
      <div className="relative flex-1">
        <GraphCanvas
          theme={reagraphTheme}
          nodes={reagraphNodes}
          edges={reagraphEdges}
          clusterAttribute="cluster"
          layoutType="forceDirected2d"
          layoutOverrides={{
            linkDistance: 110,
            nodeStrength: -400,
            clusterStrength: 0.6,
          }}
          labelType="all"
          selections={selectedNode ? [selectedNode] : []}
          actives={centerNode ? [centerNode] : []}
          draggable
          edgeArrowPosition="end"
          onNodeClick={onNodeClick}
          onNodeDoubleClick={onNodeDoubleClick}
          onEdgeClick={onEdgeClick}
          onCanvasClick={onCanvasClick}
        />

        <div className="pointer-events-none absolute inset-x-0 top-0 flex justify-center pt-8">
          <div className="pointer-events-auto w-full max-w-2xl">
            <div className="rounded-full bg-background/60 backdrop-blur-3xl border border-primary/5 shadow-2xl p-1.5 flex items-center gap-2 animate-in fade-in slide-in-from-top-4 duration-1000">
              <Popover open={open} onOpenChange={setOpen}>
                <PopoverTrigger asChild>
                  <Button
                    variant="ghost"
                    className="flex-1 justify-start h-10 rounded-full bg-muted/20 hover:bg-muted/40 border-none transition-all font-medium text-sm group px-4"
                  >
                    <Search className="size-4 mr-3 text-muted-foreground group-hover:text-primary transition-colors" />
                    <span className={cn("truncate", !centerNode && "text-muted-foreground opacity-50")}>
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
                            onSelect={() => {
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

            <div className="mt-3 flex items-center justify-center gap-3 px-2 animate-in fade-in duration-1000 delay-500 fill-mode-both">
              <div className="flex items-center gap-0.5 rounded-full bg-background/50 backdrop-blur-md border border-primary/5 p-0.5">
                {(["coarse", "medium", "fine"] as const).map((level) => (
                  <button
                    key={level}
                    onClick={() => setDetailLevel(level)}
                    className={cn(
                      "px-2.5 py-1 rounded-full text-[9px] font-bold uppercase tracking-[0.15em] transition-colors",
                      detailLevel === level
                        ? "bg-primary/15 text-primary"
                        : "text-muted-foreground/40 hover:text-muted-foreground/70"
                    )}
                  >
                    {level}
                  </button>
                ))}
              </div>
            </div>

            <div className="flex items-center justify-center gap-6 mt-3 text-[9px] font-bold uppercase tracking-[0.2em] text-muted-foreground/30 px-2 animate-in fade-in duration-1000 delay-500 fill-mode-both">
              <span className="flex items-center gap-1.5">
                <div className="size-1 rounded-full bg-primary/20" />
                Depth {depth} / {MAX_DEPTH}
              </span>
              <span className="flex items-center gap-1.5">
                <div className="size-1 rounded-full bg-primary/20" />
                {graphEntries.length} Nodes
              </span>
              <span className="flex items-center gap-1.5">
                <div className="size-1 rounded-full bg-primary/20" />
                {clusterCount} Clusters
              </span>
              <span className="flex items-center gap-1.5">
                <div className="size-1 rounded-full bg-primary/20" />
                {enabledGraphStatus.similarity_edge_count} Sim / {enabledGraphStatus.manual_edge_count} Manual
              </span>
            </div>
          </div>
        </div>
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
                onClick={() => setSelectedNode(null)}
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
                onClick={() => router.push(`/entries/${encodeURIComponent(selectedEntry.id)}`)}
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
                              <p className="text-[9px] font-bold uppercase tracking-widest text-center">Type node ID or<br />text to search</p>
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
                    onChange={(e) => setNewEdge((prev) => ({ ...prev, relationship: e.target.value }))}
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
                      edge.source_id === selectedEntry.id ? edge.target_id : edge.source_id
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
                                : `manual weight ${edge.weight?.toFixed(2) ?? "n/a"}`}
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
