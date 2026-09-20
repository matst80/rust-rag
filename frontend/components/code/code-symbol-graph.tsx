"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useTheme } from "next-themes"
import dynamic from "next/dynamic"
import { darkTheme, lightTheme } from "reagraph"
import type { GraphCanvasProps, GraphEdge, GraphNode, InternalGraphNode, Theme } from "reagraph"
import {
  ArrowDownLeft,
  ArrowUpRight,
  FileCode,
  LoaderCircle,
  Minus,
  Plus,
  ShieldAlert,
  X,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { getCodeSymbolGraph } from "@/lib/api/client"
import type { CodeSymbolGraph, CodeSymbolRef } from "@/lib/api/types"

const GraphCanvas = dynamic(
  () => import("reagraph").then((m) => m.GraphCanvas),
  { ssr: false }
) as unknown as React.ComponentType<GraphCanvasProps>

const ANCHOR_COLOR = "#6366f1"
const CALLER_COLOR = "#f59e0b"
const CALLEE_COLOR = "#10b981"

// BFS hops the server supports (validated 1..=5); the sheet opens 2 deep —
// enough to see the surrounding connections without pulling in a hub's whole
// transitive closure. The BFS itself is cycle-safe (visited set server-side).
const MAX_DEPTH = 5
const DEFAULT_DEPTH = 2

// Bottom-bar geometry: default half the viewport, resizable via the drag
// strip on the bar's top edge.
const MIN_HEIGHT_VH = 25
const MAX_HEIGHT_VH = 85
const DEFAULT_HEIGHT_VH = 50

function clampHeightVh(vh: number): number {
  return Math.round(Math.min(MAX_HEIGHT_VH, Math.max(MIN_HEIGHT_VH, vh)))
}

function basename(path: string): string {
  return path.split("/").pop() ?? path
}

function nodeKey(ref: CodeSymbolRef): string {
  return `${ref.path}:${ref.name}:${ref.line}`
}

function buildTheme(isDark: boolean): Theme {
  const base = isDark ? darkTheme : lightTheme
  const bg = isDark ? "#0a0a0a" : "#fafafa"
  const label = isDark ? "#e2e8f0" : "#1e293b"
  const subLabel = isDark ? "#94a3b8" : "#64748b"
  return {
    ...base,
    canvas: { background: bg, fog: bg },
    node: {
      ...base.node,
      label: {
        ...base.node.label,
        color: label,
        stroke: bg,
        activeColor: isDark ? "#a5b4fc" : "#4338ca",
      },
      subLabel: {
        ...(base.node.subLabel ?? { color: subLabel, activeColor: subLabel }),
        color: subLabel,
        stroke: bg,
        activeColor: isDark ? "#a5b4fc" : "#4338ca",
      },
    },
  }
}

export interface SymbolGraphTarget {
  path: string
  symbol: string
}

interface SymbolGraphSheetProps {
  repo: string
  target: SymbolGraphTarget | null
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Load the graph for another symbol (node double-click, list click). */
  onNavigate: (target: SymbolGraphTarget) => void
  onViewFile: (path: string) => void
}

export function SymbolGraphSheet({
  repo,
  target,
  open,
  onOpenChange,
  onNavigate,
  onViewFile,
}: SymbolGraphSheetProps) {
  const { resolvedTheme } = useTheme()
  const isDark = resolvedTheme === "dark"
  const theme = useMemo(() => buildTheme(isDark), [isDark])

  const [graph, setGraph] = useState<CodeSymbolGraph | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const [depth, setDepth] = useState(DEFAULT_DEPTH)
  // Bottom-bar height in vh; persists across open/close for the page session.
  const [heightVh, setHeightVh] = useState(DEFAULT_HEIGHT_VH)
  const resizeDrag = useRef<{ startY: number; startVh: number } | null>(null)
  // Remembers the fetched target so a depth change refreshes in place (the
  // current graph stays visible); a new target starts from an empty canvas.
  // Depth intentionally stays sticky across re-anchors.
  const fetchedTarget = useRef<SymbolGraphTarget | null>(null)

  useEffect(() => {
    if (!open || !target) return
    const prev = fetchedTarget.current
    const newTarget =
      !prev || prev.path !== target.path || prev.symbol !== target.symbol
    fetchedTarget.current = target
    let cancelled = false
    setLoading(true)
    setError(null)
    setSelected(null)
    if (newTarget) setGraph(null)
    getCodeSymbolGraph(repo, target.path, target.symbol, 24, depth)
      .then((g) => {
        if (!cancelled) setGraph(g)
      })
      .catch((e) => {
        if (!cancelled) setError(String(e))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [open, repo, target, depth])

  const nodes = useMemo<GraphNode[]>(() => {
    if (!graph) return []
    const anchorKey = nodeKey(graph.symbol)
    // Amber nodes only ever appear as edge sources (they call into the
    // cluster); anything reached by a call is green.
    const sources = new Set(graph.edges.map((e) => nodeKey(e.from)))
    const targets = new Set(graph.edges.map((e) => nodeKey(e.to)))
    return graph.nodes.map((n) => {
      const key = nodeKey(n.symbol)
      const isAnchor = key === anchorKey
      const callerOnly = sources.has(key) && !targets.has(key)
      return {
        id: key,
        label: n.symbol.name,
        subLabel: basename(n.symbol.path),
        fill: isAnchor ? ANCHOR_COLOR : callerOnly ? CALLER_COLOR : CALLEE_COLOR,
        size: isAnchor ? 15 : 9,
      }
    })
  }, [graph])

  const edges = useMemo<GraphEdge[]>(() => {
    if (!graph) return []
    const sources = new Set(graph.edges.map((e) => nodeKey(e.from)))
    const targets = new Set(graph.edges.map((e) => nodeKey(e.to)))
    const callerOnly = (key: string) => sources.has(key) && !targets.has(key)
    return graph.edges.map((e) => {
      const sourceKey = nodeKey(e.from)
      return {
        id: `call-${sourceKey}-${nodeKey(e.to)}`,
        source: sourceKey,
        target: nodeKey(e.to),
        size: 1.5,
        fill: callerOnly(sourceKey) ? CALLER_COLOR : CALLEE_COLOR,
      }
    })
  }, [graph])

  const onCanvasClick = useCallback(() => setSelected(null), [])

  // Re-center action for the selected connected node (the anchor has nothing
  // to re-center to). Same navigation as double-click, just discoverable.
  const recenterTarget = useMemo(() => {
    if (!graph || !selected || selected === nodeKey(graph.symbol)) return null
    return graph.nodes.find((n) => nodeKey(n.symbol) === selected)?.symbol ?? null
  }, [graph, selected])

  const recenterOn = useCallback(
    (nodeId: string) => {
      const ref = graph?.nodes.find((n) => nodeKey(n.symbol) === nodeId)?.symbol
      if (ref) onNavigate({ path: ref.path, symbol: ref.name })
    },
    [graph, onNavigate]
  )

  // Double-click detection on the click handler itself — reagraph's
  // onNodeDoubleClick relies on the canvas's dblclick raycast, which proves
  // unreliable, and one click always selects first anyway.
  const lastNodeClick = useRef<{ id: string; time: number } | null>(null)

  const onNodeClick = useCallback(
    (node: InternalGraphNode) => {
      setSelected(node.id)
      const now = performance.now()
      const last = lastNodeClick.current
      lastNodeClick.current = { id: node.id, time: now }
      if (last && last.id === node.id && now - last.time < 400) {
        lastNodeClick.current = null
        recenterOn(node.id)
      }
    },
    [recenterOn]
  )

  const onResizePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault()
      resizeDrag.current = { startY: e.clientY, startVh: heightVh }
      e.currentTarget.setPointerCapture(e.pointerId)
    },
    [heightVh]
  )

  const onResizePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const drag = resizeDrag.current
    if (!drag) return
    const deltaVh = ((drag.startY - e.clientY) / window.innerHeight) * 100
    setHeightVh(clampHeightVh(drag.startVh + deltaVh))
  }, [])

  const onResizePointerEnd = useCallback(() => {
    resizeDrag.current = null
  }, [])

  const onResizeDoubleClick = useCallback(() => {
    setHeightVh(DEFAULT_HEIGHT_VH)
  }, [])

  const onResizeKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "ArrowUp") {
      e.preventDefault()
      setHeightVh((h) => clampHeightVh(h + 2))
    } else if (e.key === "ArrowDown") {
      e.preventDefault()
      setHeightVh((h) => clampHeightVh(h - 2))
    }
  }, [])

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="bottom"
        style={{ height: `${heightVh}vh` }}
        className="w-full overflow-hidden p-0 flex flex-col"
      >
        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize graph panel"
          tabIndex={0}
          title="Drag to resize · double-click to reset"
          className="group absolute inset-x-0 top-0 z-10 flex h-3 cursor-row-resize touch-none items-center justify-center outline-none focus-visible:bg-primary/10"
          onPointerDown={onResizePointerDown}
          onPointerMove={onResizePointerMove}
          onPointerUp={onResizePointerEnd}
          onPointerCancel={onResizePointerEnd}
          onDoubleClick={onResizeDoubleClick}
          onKeyDown={onResizeKeyDown}
        >
          <div className="h-1 w-16 rounded-full bg-muted-foreground/25 transition-colors group-hover:bg-primary/50 group-focus-visible:bg-primary/50" />
        </div>
        <SheetHeader className="shrink-0 space-y-2 px-6 pt-5 pb-3">
          <div className="flex items-center gap-2">
            <Badge variant="outline" className="text-[9px] uppercase font-bold tracking-widest">
              {graph?.symbol.kind || "symbol"}
            </Badge>
            <SheetTitle className="font-mono text-lg tracking-tight">
              {graph?.symbol.name ?? target?.symbol ?? "Symbol graph"}
            </SheetTitle>
          </div>
          <SheetDescription className="font-mono text-xs">
            {repo}/{graph?.symbol.path ?? target?.path}
            {graph && (
              <span className="ml-2 text-muted-foreground/60">
                L{graph.symbol.line}–{graph.symbol.end_line}
              </span>
            )}
          </SheetDescription>
          {graph?.symbol.signature && (
            <pre className="overflow-x-auto rounded-lg bg-muted/50 px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground/80">
              {graph.symbol.signature}
            </pre>
          )}
          {graph && (
          <div className="flex items-center justify-between gap-2 pt-1">
                <div className="flex items-center gap-3 text-[9px] font-bold uppercase tracking-[0.2em] text-muted-foreground/60">
                  <span className="flex items-center gap-1.5">
                    <span className="size-1.5 rounded-full" style={{ backgroundColor: CALLER_COLOR }} />
                    {graph.callers.length} callers
                  </span>
                  <span className="flex items-center gap-1.5">
                    <span className="size-1.5 rounded-full" style={{ backgroundColor: CALLEE_COLOR }} />
                    {graph.callees.length} callees
                  </span>
                  <span className="text-muted-foreground/40">
                    {graph.nodes.length} nodes · {graph.edges.length} edges
                  </span>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  {loading && (
                    <LoaderCircle className="size-3.5 animate-spin text-muted-foreground" />
                  )}
                  <Button
                    variant="outline"
                    size="icon"
                    className="size-6 rounded-md"
                    disabled={depth <= 1 || loading}
                    onClick={() => setDepth((d) => Math.max(1, d - 1))}
                    title="Shrink graph"
                  >
                    <Minus className="size-3" />
                  </Button>
                  <span className="w-10 text-center text-[10px] font-bold text-muted-foreground">
                    {depth} hop{depth === 1 ? "" : "s"}
                  </span>
                  <Button
                    variant="outline"
                    size="icon"
                    className="size-6 rounded-md"
                    disabled={depth >= MAX_DEPTH || loading}
                    onClick={() => setDepth((d) => Math.min(MAX_DEPTH, d + 1))}
                    title="Expand graph"
                  >
                    <Plus className="size-3" />
                  </Button>
                </div>
          </div>
          )}
        </SheetHeader>

        {graph ? (
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 pb-4 scrollbar-thin md:flex-row md:overflow-hidden">
              <div className="flex shrink-0 flex-col md:min-h-0 md:flex-1">
                <div className="relative h-52 min-h-[180px] overflow-hidden rounded-2xl border border-primary/10 md:h-auto md:min-h-0 md:flex-1">
                  <GraphCanvas
                    theme={theme}
                    nodes={nodes}
                    edges={edges}
                    layoutType="forceDirected2d"
                    layoutOverrides={{ linkDistance: 120, nodeStrength: -600 }}
                    labelType="all"
                    // Serve the label font locally — troika's default pulls
                    // Roboto from a Google Fonts CDN, which flakes on
                    // self-hosted/offline deployments (labels silently
                    // vanish).
                    labelFontUrl="/fonts/Roboto-Variable.ttf"
                    animated={false}
                    selections={selected ? [selected] : []}
                    actives={selected ? [selected] : []}
                    draggable
                    edgeArrowPosition="end"
                    onNodeClick={onNodeClick}
                    onCanvasClick={onCanvasClick}
                  />
                  {recenterTarget && (
                    <div className="absolute inset-x-0 bottom-2 flex justify-center">
                      <div className="flex items-center gap-1.5 rounded-full border bg-background/90 py-1 pl-3 pr-1 shadow-lg backdrop-blur">
                        <span
                          className="max-w-[220px] truncate font-mono text-xs font-semibold"
                          title={recenterTarget.path}
                        >
                          {recenterTarget.name}
                        </span>
                        <Button
                          size="sm"
                          className="h-6 rounded-full px-2.5 text-[10px] font-bold uppercase tracking-wider"
                          onClick={() =>
                            onNavigate({ path: recenterTarget.path, symbol: recenterTarget.name })
                          }
                        >
                          Re-center
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="size-6 rounded-full"
                          onClick={() => setSelected(null)}
                          title="Clear selection"
                        >
                          <X className="size-3" />
                        </Button>
                      </div>
                    </div>
                  )}
                </div>
                <p className="mt-2 text-center text-[9px] font-medium uppercase tracking-widest text-muted-foreground/40">
                  Click to select · double-click to re-center · ± to change depth
                </p>
              </div>

              <div className="min-h-0 space-y-6 md:w-80 md:shrink-0 md:overflow-y-auto md:border-l md:border-primary/10 md:pr-1 md:pl-5 md:scrollbar-thin">
              <SymbolEdgeList
                title="Called by"
                icon={<ArrowDownLeft className="size-3.5 text-amber-500" />}
                refs={graph.callers}
                onNavigate={onNavigate}
                onViewFile={onViewFile}
                emptyText="No indexed callers — nothing in the snapshot calls this symbol by name."
              />
              <SymbolEdgeList
                title="Calls"
                icon={<ArrowUpRight className="size-3.5 text-emerald-500" />}
                refs={graph.callees}
                onNavigate={onNavigate}
                onViewFile={onViewFile}
                emptyText="No resolvable callees in this snapshot."
              />
              {graph.unresolved_callees.length > 0 && (
                <div className="space-y-2">
                  <h4 className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
                    Unresolved call targets
                  </h4>
                  <div className="flex flex-wrap gap-1.5">
                    {graph.unresolved_callees.map((name) => (
                      <span
                        key={name}
                        className="rounded-full border bg-muted/30 px-2 py-0.5 font-mono text-[10px] text-muted-foreground"
                      >
                        {name}()
                      </span>
                    ))}
                  </div>
                  <p className="text-[10px] text-muted-foreground/50">
                    Dynamic or external calls that matched no indexed symbol.
                  </p>
                </div>
              )}
              </div>
            </div>
        ) : (
          <div className="flex flex-1 items-center justify-center">
            {loading ? (
              <div className="flex flex-col items-center gap-4 text-muted-foreground opacity-40">
                <LoaderCircle className="size-10 animate-spin" />
                <p className="text-xs font-bold uppercase tracking-widest">Tracing call graph…</p>
              </div>
            ) : error ? (
              <div className="mx-6 space-y-4 text-center">
                <ShieldAlert className="mx-auto size-10 text-destructive/60" />
                <p className="text-sm text-destructive">{error}</p>
                <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
                  <X className="size-3.5 mr-2" />
                  Close
                </Button>
              </div>
            ) : null}
          </div>
        )}
      </SheetContent>
    </Sheet>
  )
}

function SymbolEdgeList({
  title,
  icon,
  refs,
  onNavigate,
  onViewFile,
  emptyText,
}: {
  title: string
  icon: React.ReactNode
  refs: CodeSymbolRef[]
  onNavigate: (target: SymbolGraphTarget) => void
  onViewFile: (path: string) => void
  emptyText: string
}) {
  return (
    <div className="space-y-2">
      <h4 className="flex items-center gap-2 text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
        {icon}
        {title}
        <span className="text-muted-foreground/40">({refs.length})</span>
      </h4>
      {refs.length === 0 ? (
        <p className="text-xs italic text-muted-foreground/50">{emptyText}</p>
      ) : (
        <div className="space-y-1.5">
          {refs.map((ref) => (
            <div
              key={`${ref.path}:${ref.name}:${ref.line}`}
              className="group flex items-center justify-between gap-2 rounded-xl border border-transparent bg-muted/5 p-2.5 transition-all hover:border-primary/10 hover:bg-muted/10"
            >
              <button
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
                onClick={() => onNavigate({ path: ref.path, symbol: ref.name })}
                title="Explore this symbol's call graph"
              >
                <Badge variant="outline" className="shrink-0 text-[8px] uppercase">
                  {ref.kind || "sym"}
                </Badge>
                <span className="truncate font-mono text-xs font-semibold">{ref.name}</span>
                <span className="truncate text-[10px] text-muted-foreground/50">{ref.path}</span>
              </button>
              <Button
                variant="ghost"
                size="icon"
                className="size-7 shrink-0 rounded-full opacity-0 transition-opacity hover:bg-primary/10 group-hover:opacity-100"
                onClick={() => onViewFile(ref.path)}
                title="Open file"
              >
                <FileCode className="size-3.5" />
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

export function ScoreBadge({ score }: { score: number }) {
  const pct = Math.round(score * 100)
  return (
    <div className="flex items-center gap-2" title={`cosine similarity ${score.toFixed(3)}`}>
      <Progress value={pct} className="h-1 w-14" />
      <span className="w-8 text-right text-[10px] font-bold text-muted-foreground">{pct}%</span>
    </div>
  )
}
