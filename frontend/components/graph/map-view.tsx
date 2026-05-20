"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { RefreshCw, Map as MapIcon, Loader2, Info, ZoomIn, ZoomOut, Maximize2, Maximize, Minimize, Box, Square } from "lucide-react"
import dynamic from "next/dynamic"
import { useMap, useRebuildMap } from "@/lib/api"
import type { MapPoint } from "@/lib/api/types"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { toast } from "sonner"

const MapView3D = dynamic(() => import("./map-view-3d").then(m => m.MapView3D), { ssr: false })

const CLUSTER_COLORS = [
  "#6366f1", "#8b5cf6", "#ec4899", "#f43f5e", "#f97316",
  "#eab308", "#22c55e", "#06b6d4", "#3b82f6", "#64748b",
  "#a855f7", "#f97316", "#14b8a6", "#facc15", "#ef4444",
]

const POINT_RADIUS = 5
const HOVER_RADIUS = 12
const PADDING = 32

interface Transform {
  scale: number
  tx: number
  ty: number
}

interface Bounds {
  minX: number
  maxX: number
  minY: number
  maxY: number
}

function computeBounds(points: MapPoint[]): Bounds {
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity
  for (const p of points) {
    if (p.x < minX) minX = p.x
    if (p.x > maxX) maxX = p.x
    if (p.y < minY) minY = p.y
    if (p.y > maxY) maxY = p.y
  }
  if (!isFinite(minX)) { minX = -1; maxX = 1; minY = -1; maxY = 1 }
  const padX = (maxX - minX) * 0.05 || 1
  const padY = (maxY - minY) * 0.05 || 1
  return { minX: minX - padX, maxX: maxX + padX, minY: minY - padY, maxY: maxY + padY }
}

export function MapView() {
  const router = useRouter()
  const { data: mapPoints, isLoading, mutate } = useMap()
  const { trigger: rebuildMap, isMutating: isRebuilding } = useRebuildMap()

  const containerRef = React.useRef<HTMLDivElement>(null)
  const svgRef = React.useRef<SVGSVGElement>(null)
  const [size, setSize] = React.useState({ w: 800, h: 600 })
  const [transform, setTransform] = React.useState<Transform>({ scale: 1, tx: 0, ty: 0 })
  const [hovered, setHovered] = React.useState<{ point: MapPoint; cx: number; cy: number } | null>(null)
  const [cursor, setCursor] = React.useState<{ x: number; y: number } | null>(null)
  const [isDragging, setIsDragging] = React.useState(false)
  const [isFullscreen, setIsFullscreen] = React.useState(false)
  const [focusedCluster, setFocusedCluster] = React.useState<number | null>(null)
  const [showLegend, setShowLegend] = React.useState(true)
  const [mode, setMode] = React.useState<"2d" | "3d">("2d")
  const dragStart = React.useRef<{ x: number; y: number; tx: number; ty: number } | null>(null)
  const didDrag = React.useRef(false)

  React.useEffect(() => {
    if (!containerRef.current) return
    const el = containerRef.current
    const update = () => {
      const rect = el.getBoundingClientRect()
      setSize({ w: rect.width, h: rect.height })
    }
    update()
    const ro = new ResizeObserver(update)
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  const bounds = React.useMemo(() => mapPoints ? computeBounds(mapPoints) : null, [mapPoints])

  const projected = React.useMemo(() => {
    if (!mapPoints || !bounds) return [] as { p: MapPoint; sx: number; sy: number }[]
    const innerW = Math.max(1, size.w - PADDING * 2)
    const innerH = Math.max(1, size.h - PADDING * 2)
    const rangeX = bounds.maxX - bounds.minX || 1
    const rangeY = bounds.maxY - bounds.minY || 1
    return mapPoints.map(p => {
      const baseX = PADDING + ((p.x - bounds.minX) / rangeX) * innerW
      const baseY = PADDING + (1 - (p.y - bounds.minY) / rangeY) * innerH
      const sx = baseX * transform.scale + transform.tx
      const sy = baseY * transform.scale + transform.ty
      return { p, sx, sy }
    })
  }, [mapPoints, bounds, size, transform])

  const handleMouseMove = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!svgRef.current) return
    const rect = svgRef.current.getBoundingClientRect()
    const mx = e.clientX - rect.left
    const my = e.clientY - rect.top
    setCursor({ x: mx, y: my })

    if (isDragging && dragStart.current) {
      const dx = e.clientX - dragStart.current.x
      const dy = e.clientY - dragStart.current.y
      if (Math.abs(dx) > 2 || Math.abs(dy) > 2) didDrag.current = true
      setTransform(t => ({ ...t, tx: dragStart.current!.tx + dx, ty: dragStart.current!.ty + dy }))
      return
    }

    let best: { point: MapPoint; cx: number; cy: number; d: number } | null = null
    const r = HOVER_RADIUS * HOVER_RADIUS
    for (const { p, sx, sy } of projected) {
      const dx = sx - mx
      const dy = sy - my
      const d = dx * dx + dy * dy
      if (d < r && (!best || d < best.d)) {
        best = { point: p, cx: sx, cy: sy, d }
      }
    }
    setHovered(best ? { point: best.point, cx: best.cx, cy: best.cy } : null)
  }

  const handleMouseDown = (e: React.MouseEvent<SVGSVGElement>) => {
    if (e.button !== 0) return
    setIsDragging(true)
    didDrag.current = false
    dragStart.current = { x: e.clientX, y: e.clientY, tx: transform.tx, ty: transform.ty }
  }

  const handleMouseUp = () => {
    setIsDragging(false)
    dragStart.current = null
  }

  const handleClick = () => {
    if (didDrag.current) return
    if (hovered) router.push(`/entries/${encodeURIComponent(hovered.point.id)}`)
  }

  const handleWheel = (e: React.WheelEvent<SVGSVGElement>) => {
    if (!svgRef.current) return
    e.preventDefault()
    const rect = svgRef.current.getBoundingClientRect()
    const mx = e.clientX - rect.left
    const my = e.clientY - rect.top
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15
    setTransform(t => {
      const newScale = Math.min(20, Math.max(0.2, t.scale * factor))
      const k = newScale / t.scale
      return {
        scale: newScale,
        tx: mx - (mx - t.tx) * k,
        ty: my - (my - t.ty) * k,
      }
    })
  }

  const resetView = () => setTransform({ scale: 1, tx: 0, ty: 0 })
  const toggleFullscreen = () => setIsFullscreen(f => !f)

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isFullscreen) setIsFullscreen(false)
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [isFullscreen])
  const zoomBy = (factor: number) => {
    setTransform(t => {
      const newScale = Math.min(20, Math.max(0.2, t.scale * factor))
      const cx = size.w / 2
      const cy = size.h / 2
      const k = newScale / t.scale
      return { scale: newScale, tx: cx - (cx - t.tx) * k, ty: cy - (cy - t.ty) * k }
    })
  }

  const handleRebuild = async () => {
    try {
      await rebuildMap()
      toast.success("Projection rebuild started in background. This may take a minute.")
    } catch (e) {
      toast.error("Failed to trigger rebuild: " + (e as any).message)
    }
  }

  React.useEffect(() => {
    const svg = svgRef.current
    if (!svg) return
    const wheel = (e: WheelEvent) => e.preventDefault()
    svg.addEventListener("wheel", wheel, { passive: false })
    return () => svg.removeEventListener("wheel", wheel)
  }, [])

  const clusterSummary = React.useMemo(() => {
    if (!mapPoints) return [] as { id: number; name?: string; description?: string; count: number }[]
    const map = new Map<number, { name?: string; description?: string; count: number }>()
    for (const p of mapPoints) {
      const existing = map.get(p.cluster)
      if (existing) {
        existing.count += 1
        if (!existing.name && p.cluster_name) existing.name = p.cluster_name
        if (!existing.description && p.cluster_description) existing.description = p.cluster_description
      } else {
        map.set(p.cluster, { name: p.cluster_name, description: p.cluster_description, count: 1 })
      }
    }
    return Array.from(map.entries())
      .map(([id, v]) => ({ id, ...v }))
      .sort((a, b) => b.count - a.count)
  }, [mapPoints])

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-muted-foreground opacity-50">
        <Loader2 className="size-6 animate-spin" />
        <span className="font-mono text-sm uppercase tracking-widest">Loading projection manifold...</span>
      </div>
    )
  }

  if (!mapPoints || mapPoints.length === 0) {
    return (
      <div className="flex flex-col h-full items-center justify-center p-12 text-center">
        <MapIcon className="size-16 text-muted-foreground/20 mb-6" />
        <h3 className="text-xl font-bold mb-2">No projection map found</h3>
        <p className="text-muted-foreground max-w-md mb-8">
          The knowledge map projects your embeddings into 2D space.
          You need to trigger a rebuild to generate the first coordinates.
        </p>
        <Button
          onClick={handleRebuild}
          disabled={isRebuilding}
          className="rounded-2xl h-12 px-8 font-bold uppercase tracking-widest"
        >
          {isRebuilding ? (
            <Loader2 className="size-4 mr-2 animate-spin" />
          ) : (
            <RefreshCw className="size-4 mr-2" />
          )}
          Generate Knowledge Map
        </Button>
      </div>
    )
  }

  const clusterCount = clusterSummary.length

  return (
    <div className={
      isFullscreen
        ? "fixed inset-0 z-[100] flex flex-col bg-background"
        : "flex flex-col h-full bg-background/50"
    }>
      <div className="flex items-center justify-between px-8 py-4 border-b border-primary/5 bg-background/30 backdrop-blur-md">
        <div className="flex flex-col">
          <h2 className="text-xs font-black uppercase tracking-[0.2em] text-foreground/80">Neural Manifold</h2>
          <p className="text-[10px] text-muted-foreground font-mono uppercase tracking-widest mt-1 opacity-50">
            PCA · {mode.toUpperCase()} · {mapPoints.length} points · {clusterCount} clusters{mode === "2d" ? ` · zoom ${transform.scale.toFixed(2)}×` : ""}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={() => zoomBy(1.3)} className="h-9 w-9 rounded-full hover:bg-primary/5" title="Zoom in">
            <ZoomIn className="size-4 opacity-60" />
          </Button>
          <Button variant="ghost" size="sm" onClick={() => zoomBy(1 / 1.3)} className="h-9 w-9 rounded-full hover:bg-primary/5" title="Zoom out">
            <ZoomOut className="size-4 opacity-60" />
          </Button>
          <Button variant="ghost" size="sm" onClick={resetView} className="h-9 w-9 rounded-full hover:bg-primary/5" title="Reset view">
            <Maximize2 className="size-4 opacity-60" />
          </Button>
          <Button variant="ghost" size="sm" onClick={toggleFullscreen} className="h-9 w-9 rounded-full hover:bg-primary/5" title={isFullscreen ? "Exit fullscreen" : "Fullscreen"}>
            {isFullscreen ? <Minimize className="size-4 opacity-60" /> : <Maximize className="size-4 opacity-60" />}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setMode(m => m === "2d" ? "3d" : "2d")} className="h-9 w-9 rounded-full hover:bg-primary/5" title={mode === "2d" ? "Switch to 3D" : "Switch to 2D"}>
            {mode === "2d" ? <Box className="size-4 opacity-60" /> : <Square className="size-4 opacity-60" />}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => mutate()} className="h-9 w-9 rounded-full hover:bg-primary/5" title="Refresh map">
            <RefreshCw className="size-4 opacity-50" />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleRebuild}
            disabled={isRebuilding}
            className="h-9 px-4 rounded-full font-bold uppercase text-[9px] tracking-widest border-primary/20 hover:bg-primary/5 transition-all"
          >
            {isRebuilding && <Loader2 className="size-3 mr-2 animate-spin" />}
            Rebuild Map
          </Button>
        </div>
      </div>

      <div
        ref={containerRef}
        className="flex-1 relative overflow-hidden"
        style={{ cursor: mode === "3d" ? "default" : isDragging ? "grabbing" : hovered ? "pointer" : "grab" }}
      >
        {mode === "3d" && (
          <div className="absolute inset-0">
            <React.Suspense fallback={<div className="flex h-full items-center justify-center"><Loader2 className="size-6 animate-spin opacity-50" /></div>}>
              <MapView3D
                points={mapPoints}
                colors={CLUSTER_COLORS}
                focusedCluster={focusedCluster}
                onPick={(p) => router.push(`/entries/${encodeURIComponent(p.id)}`)}
                onHover={(p, screen) => {
                  if (p && screen) {
                    setHovered({ point: p, cx: screen.x, cy: screen.y })
                    setCursor({ x: screen.x, y: screen.y })
                  } else {
                    setHovered(null)
                    setCursor(null)
                  }
                }}
              />
            </React.Suspense>
          </div>
        )}
        {mode === "2d" && (
        <svg
          ref={svgRef}
          width={size.w}
          height={size.h}
          onMouseMove={handleMouseMove}
          onMouseLeave={() => { setHovered(null); setCursor(null); handleMouseUp() }}
          onMouseDown={handleMouseDown}
          onMouseUp={handleMouseUp}
          onClick={handleClick}
          onWheel={handleWheel}
          className="block select-none"
        >
          <defs>
            <radialGradient id="point-glow">
              <stop offset="0%" stopColor="currentColor" stopOpacity="0.8" />
              <stop offset="100%" stopColor="currentColor" stopOpacity="0" />
            </radialGradient>
          </defs>

          {projected.map(({ p, sx, sy }, i) => {
            const isHover = hovered?.point.id === p.id
            const inFocus = focusedCluster === null || focusedCluster === p.cluster
            const color = CLUSTER_COLORS[p.cluster % CLUSTER_COLORS.length]
            let opacity = 0.7
            if (!inFocus) opacity = 0.08
            else if (hovered && !isHover) opacity = 0.25
            return (
              <circle
                key={p.id ?? i}
                cx={sx}
                cy={sy}
                r={POINT_RADIUS}
                fill={color}
                opacity={opacity}
                style={{ transition: "opacity 150ms ease" }}
              />
            )
          })}

          {hovered && (
            <g pointerEvents="none">
              <circle
                cx={hovered.cx}
                cy={hovered.cy}
                r={POINT_RADIUS + 6}
                fill="none"
                stroke={CLUSTER_COLORS[hovered.point.cluster % CLUSTER_COLORS.length]}
                strokeWidth={1.5}
                opacity={0.8}
              />
              <circle
                cx={hovered.cx}
                cy={hovered.cy}
                r={POINT_RADIUS + 12}
                fill="none"
                stroke={CLUSTER_COLORS[hovered.point.cluster % CLUSTER_COLORS.length]}
                strokeWidth={1}
                opacity={0.3}
              />
            </g>
          )}
        </svg>
        )}

        {showLegend && clusterSummary.length > 0 && (
          <div className="absolute top-3 left-3 max-h-[calc(100%-1.5rem)] w-64 overflow-y-auto rounded-xl border border-primary/10 bg-background/90 backdrop-blur-xl shadow-2xl">
            <div className="px-3 py-2 border-b border-primary/5 flex items-center justify-between">
              <span className="text-[9px] font-black uppercase tracking-widest text-foreground/60">
                Clusters · {clusterSummary.length}
              </span>
              <button
                onClick={() => setShowLegend(false)}
                className="text-[9px] text-muted-foreground hover:text-foreground"
                title="Hide legend"
              >
                ×
              </button>
            </div>
            <ul className="p-1.5 space-y-0.5">
              {clusterSummary.map(c => {
                const color = CLUSTER_COLORS[c.id % CLUSTER_COLORS.length]
                const active = focusedCluster === c.id
                return (
                  <li key={c.id}>
                    <button
                      onClick={() => setFocusedCluster(active ? null : c.id)}
                      className={
                        "w-full flex items-start gap-2 px-2 py-1.5 rounded-md text-left transition-colors " +
                        (active ? "bg-primary/10" : "hover:bg-muted/40")
                      }
                    >
                      <span
                        className="mt-1 size-2.5 rounded-full shrink-0"
                        style={{ backgroundColor: color }}
                      />
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-[11px] font-semibold truncate">
                            {c.name || `Cluster ${c.id}`}
                          </span>
                          <span className="text-[9px] text-muted-foreground font-mono shrink-0">
                            {c.count}
                          </span>
                        </div>
                        {c.description && (
                          <p className="text-[9px] text-muted-foreground/80 leading-snug line-clamp-2 mt-0.5">
                            {c.description}
                          </p>
                        )}
                      </div>
                    </button>
                  </li>
                )
              })}
            </ul>
          </div>
        )}
        {!showLegend && (
          <button
            onClick={() => setShowLegend(true)}
            className="absolute top-3 left-3 px-3 py-1.5 rounded-full bg-background/80 backdrop-blur-xl border border-primary/10 text-[9px] font-bold uppercase tracking-widest hover:bg-primary/5"
          >
            Show legend
          </button>
        )}
        {focusedCluster !== null && (
          <button
            onClick={() => setFocusedCluster(null)}
            className="absolute top-3 right-3 px-3 py-1.5 rounded-full bg-background/80 backdrop-blur-xl border border-primary/10 text-[9px] font-bold uppercase tracking-widest hover:bg-primary/5"
          >
            Clear filter
          </button>
        )}

        {hovered && cursor && (
          <div
            className="absolute pointer-events-none z-10"
            style={{
              left: Math.min(cursor.x + 16, size.w - 340),
              top: Math.min(cursor.y + 16, Math.max(8, size.h - 260)),
            }}
          >
            <Card className="border-primary/10 shadow-2xl bg-background/95 backdrop-blur-xl w-[320px]">
              <CardHeader className="p-3 pb-1 space-y-1">
                <CardTitle className="text-sm font-bold leading-tight line-clamp-2">
                  {hovered.point.title || hovered.point.id}
                </CardTitle>
                {hovered.point.title && (
                  <div className="text-[9px] text-muted-foreground font-mono truncate opacity-60">
                    {hovered.point.id}
                  </div>
                )}
              </CardHeader>
              <CardContent className="p-3 pt-2 space-y-2">
                <div className="flex flex-wrap items-center gap-1">
                  <Badge
                    variant="outline"
                    className="text-[8px] uppercase tracking-widest border-none"
                    style={{
                      backgroundColor: CLUSTER_COLORS[hovered.point.cluster % CLUSTER_COLORS.length] + "30",
                      color: CLUSTER_COLORS[hovered.point.cluster % CLUSTER_COLORS.length],
                    }}
                  >
                    {hovered.point.cluster_name || `Cluster ${hovered.point.cluster}`}
                  </Badge>
                  {hovered.point.doc_type && (
                    <Badge variant="outline" className="text-[8px] uppercase tracking-widest bg-muted/50 border-none">
                      {hovered.point.doc_type}
                    </Badge>
                  )}
                  {hovered.point.source_id && (
                    <Badge variant="outline" className="text-[8px] uppercase tracking-widest bg-muted/50 border-none">
                      {hovered.point.source_id}
                    </Badge>
                  )}
                </div>
                {hovered.point.cluster_description && (
                  <p className="text-[10px] leading-relaxed text-muted-foreground italic line-clamp-2">
                    {hovered.point.cluster_description}
                  </p>
                )}
                {hovered.point.path && (
                  <div className="text-[10px] font-mono text-muted-foreground truncate">
                    {hovered.point.path}
                  </div>
                )}
                {hovered.point.snippet && (
                  <p className="text-[11px] leading-relaxed text-foreground/80 line-clamp-4">
                    {hovered.point.snippet}
                  </p>
                )}
                {hovered.point.tags && hovered.point.tags.length > 0 && (
                  <div className="flex flex-wrap gap-1">
                    {hovered.point.tags.slice(0, 6).map(tag => (
                      <span key={tag} className="text-[8px] px-1.5 py-0.5 rounded bg-primary/10 text-primary/80 font-mono">
                        {tag}
                      </span>
                    ))}
                  </div>
                )}
                <div className="flex items-center justify-between gap-2 pt-1 border-t border-border/40">
                  <span className="text-[9px] text-muted-foreground font-mono opacity-60">
                    {hovered.point.x.toFixed(2)}, {hovered.point.y.toFixed(2)}
                  </span>
                  <span className="text-[9px] text-primary/70 font-bold uppercase tracking-widest">
                    Click to open
                  </span>
                </div>
              </CardContent>
            </Card>
          </div>
        )}
      </div>

      <div className="px-8 py-3 bg-muted/5 border-t border-primary/5 flex items-center justify-center gap-6">
        <div className="flex items-center gap-2">
          <Info className="size-3 text-primary/40" />
          <span className="text-[9px] font-bold uppercase tracking-widest text-muted-foreground/40">
            {mode === "3d" ? "Drag to rotate · right-drag to pan · scroll to zoom · click point" : "Drag to pan · scroll to zoom · click point to inspect"}
          </span>
        </div>
      </div>
    </div>
  )
}
