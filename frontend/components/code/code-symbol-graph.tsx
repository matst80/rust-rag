"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useTheme } from "next-themes"
import dynamic from "next/dynamic"
import { darkTheme, lightTheme } from "reagraph"
import type { GraphCanvasProps, GraphEdge, GraphNode, InternalGraphNode, Theme } from "reagraph"
import {
  ArrowDownLeft,
  ArrowUpRight,
  FileCode,
  LoaderCircle,
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

  useEffect(() => {
    if (!open || !target) return
    let cancelled = false
    setLoading(true)
    setError(null)
    setGraph(null)
    setSelected(null)
    getCodeSymbolGraph(repo, target.path, target.symbol)
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
  }, [open, repo, target])

  const nodes = useMemo<GraphNode[]>(() => {
    if (!graph) return []
    const out: GraphNode[] = [
      {
        id: nodeKey(graph.symbol),
        label: graph.symbol.name,
        subLabel: basename(graph.symbol.path),
        fill: ANCHOR_COLOR,
        size: 15,
      },
    ]
    for (const c of graph.callers) {
      out.push({
        id: nodeKey(c),
        label: c.name,
        subLabel: basename(c.path),
        fill: CALLER_COLOR,
        size: 9,
      })
    }
    for (const c of graph.callees) {
      if (c.path === graph.symbol.path && c.name === graph.symbol.name) continue
      out.push({
        id: nodeKey(c),
        label: c.name,
        subLabel: basename(c.path),
        fill: CALLEE_COLOR,
        size: 9,
      })
    }
    return out
  }, [graph])

  const edges = useMemo<GraphEdge[]>(() => {
    if (!graph) return []
    const anchorKey = nodeKey(graph.symbol)
    const out: GraphEdge[] = []
    for (const c of graph.callers) {
      out.push({
        id: `caller-${nodeKey(c)}`,
        source: nodeKey(c),
        target: anchorKey,
        size: 1.5,
        fill: CALLER_COLOR,
      })
    }
    for (const c of graph.callees) {
      const key = nodeKey(c)
      if (key === anchorKey) continue
      out.push({
        id: `callee-${key}`,
        source: anchorKey,
        target: key,
        size: 1.5,
        fill: CALLEE_COLOR,
      })
    }
    return out
  }, [graph])

  const onNodeClick = useCallback((node: InternalGraphNode) => {
    setSelected(node.id)
  }, [])

  const onNodeDoubleClick = useCallback(
    (node: InternalGraphNode) => {
      const all = [...(graph?.callers ?? []), ...(graph?.callees ?? [])]
      const ref = all.find((r) => nodeKey(r) === node.id)
      if (ref) onNavigate({ path: ref.path, symbol: ref.name })
    },
    [graph, onNavigate]
  )

  const onCanvasClick = useCallback(() => setSelected(null), [])

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-full overflow-y-auto sm:max-w-xl p-0 flex flex-col">
        {graph ? (
          <>
            <SheetHeader className="p-6 pb-4 space-y-2">
              <div className="flex items-center gap-2">
                <Badge variant="outline" className="text-[9px] uppercase font-bold tracking-widest">
                  {graph.symbol.kind || "symbol"}
                </Badge>
                <SheetTitle className="font-mono text-lg tracking-tight">
                  {graph.symbol.name}
                </SheetTitle>
              </div>
              <SheetDescription className="font-mono text-xs">
                {graph.repo}/{graph.symbol.path}
                <span className="ml-2 text-muted-foreground/60">
                  L{graph.symbol.line}–{graph.symbol.end_line}
                </span>
              </SheetDescription>
              {graph.symbol.signature && (
                <pre className="overflow-x-auto rounded-lg bg-muted/50 px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground/80">
                  {graph.symbol.signature}
                </pre>
              )}
              <div className="flex items-center gap-3 pt-1 text-[9px] font-bold uppercase tracking-[0.2em] text-muted-foreground/60">
                <span className="flex items-center gap-1.5">
                  <span className="size-1.5 rounded-full" style={{ backgroundColor: CALLER_COLOR }} />
                  {graph.callers.length} callers
                </span>
                <span className="flex items-center gap-1.5">
                  <span className="size-1.5 rounded-full" style={{ backgroundColor: CALLEE_COLOR }} />
                  {graph.callees.length} callees
                </span>
              </div>
            </SheetHeader>

            <div className="px-6">
              <div className="h-[320px] overflow-hidden rounded-2xl border border-primary/10">
                <GraphCanvas
                  theme={theme}
                  nodes={nodes}
                  edges={edges}
                  layoutType="forceDirected2d"
                  layoutOverrides={{ linkDistance: 120, nodeStrength: -600 }}
                  labelType="auto"
                  selections={selected ? [selected] : []}
                  actives={selected ? [selected] : []}
                  draggable
                  edgeArrowPosition="end"
                  onNodeClick={onNodeClick}
                  onNodeDoubleClick={onNodeDoubleClick}
                  onCanvasClick={onCanvasClick}
                />
              </div>
              <p className="mt-2 text-center text-[9px] font-medium uppercase tracking-widest text-muted-foreground/40">
                Click to select · double-click to explore that symbol
              </p>
            </div>

            <div className="flex-1 space-y-6 overflow-y-auto p-6 pt-4 scrollbar-thin">
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
          </>
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
