"use client"

import * as React from "react"
import { motion, AnimatePresence } from "framer-motion"
import { Check, X, Info, Network, Zap, ShieldCheck, Play, Loader2, Compass } from "lucide-react"
import { api, OntologyRunReport } from "@/lib/api/client"
import { Edge } from "@/lib/api/types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import { toast } from "sonner"
import { cn } from "@/lib/utils"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { ArrowDownAZ, ArrowUpAZ, SortAsc, SortDesc } from "lucide-react"

interface EdgeReviewListProps {
  onReviewComplete?: () => void
  onFocusNode?: (nodeId: string) => void
}

export function EdgeReviewList({ onReviewComplete, onFocusNode }: EdgeReviewListProps) {
  const [edges, setEdges] = React.useState<Edge[]>([])
  const [itemTitles, setItemTitles] = React.useState<Record<string, string>>({})
  const [loading, setLoading] = React.useState(true)
  const [runningBatch, setRunningBatch] = React.useState(false)
  const [runningItem, setRunningItem] = React.useState(false)
  const [itemIdInput, setItemIdInput] = React.useState("")
  const [lastRun, setLastRun] = React.useState<
    | {
      kind: "batch" | "item"
      target?: string
      startedAt: number
      elapsedMs: number
      report: OntologyRunReport
    }
    | null
  >(null)
  const [sortBy, setSortBy] = React.useState<string>("confidence-desc")

  const sortedEdges = React.useMemo(() => {
    return [...edges].sort((a, b) => {
      const confA = (a.metadata?.confidence as number) ?? 0
      const confB = (b.metadata?.confidence as number) ?? 0

      if (sortBy === "confidence-desc") return confB - confA
      if (sortBy === "confidence-asc") return confA - confB
      
      if (sortBy === "alpha-asc") return a.relationship.localeCompare(b.relationship)
      if (sortBy === "alpha-desc") return b.relationship.localeCompare(a.relationship)

      // Secondary sort by source_id to keep it stable
      return a.source_id.localeCompare(b.source_id)
    })
  }, [edges, sortBy])

  const fetchSuggestedEdges = React.useCallback(async () => {
    try {
      setLoading(true)
      const allEdges = await api.edges.list()
      const suggested = allEdges.filter(
        (e) => e.metadata?.status === "suggested"
      )
      setEdges(suggested)
    } catch (error) {
      console.error("Failed to fetch edges:", error)
      toast.error("Failed to load suggested edges")
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    fetchSuggestedEdges()
  }, [fetchSuggestedEdges])

  // Fetch titles for unique IDs when edges change
  React.useEffect(() => {
    const fetchTitles = async () => {
      const uniqueIds = Array.from(new Set(edges.flatMap(e => [e.source_id, e.target_id])))
      const missingIds = uniqueIds.filter(id => !itemTitles[id])

      if (missingIds.length === 0) return

      const titles: Record<string, string> = { ...itemTitles }
      await Promise.all(missingIds.map(async (id) => {
        try {
          const item = await api.items.get(id)
          const meta = item.metadata ?? {}
          const explicit = meta.title ?? meta.name ?? meta.label
          if (typeof explicit === "string" && explicit.trim().length > 0) {
            titles[id] = explicit.trim().slice(0, 60)
          } else {
            const firstLine = (item.text ?? "").split(/\r?\n/).map((l) => l.trim()).find(Boolean)
            if (firstLine) {
              const cleaned = firstLine.replace(/^#+\s*/, "").replace(/^[-*+]\s+/, "")
              titles[id] = cleaned.slice(0, 60) + (cleaned.length > 60 ? "…" : "")
            } else {
              titles[id] = id
            }
          }
        } catch (e) {
          titles[id] = id
        }
      }))
      setItemTitles(titles)
    }

    if (edges.length > 0) {
      fetchTitles()
    }
  }, [edges])

  const handleApprove = async (edge: Edge) => {
    try {
      const updatedMetadata = {
        ...(edge.metadata || {}),
        status: "confirmed",
      }
      await api.edges.update(edge.id, { metadata: updatedMetadata })
      setEdges((prev) => prev.filter((e) => e.id !== edge.id))
      toast.success("Edge confirmed")
      onReviewComplete?.()
    } catch (error) {
      console.error("Failed to approve edge:", error)
      toast.error("Failed to approve edge")
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await api.edges.delete(id)
      setEdges((prev) => prev.filter((e) => e.id !== id))
      toast.success("Edge rejected and deleted")
      onReviewComplete?.()
    } catch (error) {
      console.error("Failed to delete edge:", error)
      toast.error("Failed to delete edge")
    }
  }

  const summarizeReport = (
    report: Awaited<ReturnType<typeof api.ontology.runBatch>>
  ) => {
    const suggested = report.edges_committed.filter(
      (e) => e.status === "suggested"
    ).length
    const confirmed = report.edges_committed.filter(
      (e) => e.status === "confirmed"
    ).length
    return `${report.items_processed} item(s) processed · ${report.edges_committed.length} edge(s) (${suggested} suggested, ${confirmed} auto-confirmed)${report.items_skipped_no_neighbors > 0
      ? ` · ${report.items_skipped_no_neighbors} skipped (no neighbors)`
      : ""
      }`
  }

  const handleRunBatch = async () => {
    const startedAt = Date.now()
    try {
      setRunningBatch(true)
      const report = await api.ontology.runBatch()
      const elapsedMs = Date.now() - startedAt
      setLastRun({ kind: "batch", startedAt, elapsedMs, report })
      toast.success(summarizeReport(report))
      await fetchSuggestedEdges()
      onReviewComplete?.()
    } catch (error) {
      console.error("Failed to run ontology batch:", error)
      toast.error(
        error instanceof Error
          ? `Ontology run failed: ${error.message}`
          : "Ontology run failed"
      )
    } finally {
      setRunningBatch(false)
    }
  }

  const handleRunForItem = async () => {
    const id = itemIdInput.trim()
    if (!id) {
      toast.error("Enter an item id")
      return
    }
    const startedAt = Date.now()
    try {
      setRunningItem(true)
      const report = await api.ontology.runForItem(id)
      const elapsedMs = Date.now() - startedAt
      setLastRun({ kind: "item", target: id, startedAt, elapsedMs, report })
      toast.success(summarizeReport(report))
      setItemIdInput("")
      await fetchSuggestedEdges()
      onReviewComplete?.()
    } catch (error) {
      console.error("Failed to run ontology for item:", error)
      toast.error(
        error instanceof Error
          ? `Run failed: ${error.message}`
          : "Run failed"
      )
    } finally {
      setRunningItem(false)
    }
  }

  if (loading && edges.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-primary"></div>
      </div>
    )
  }

  if (edges.length === 0) {
    return (
      <Card className="border-dashed bg-muted/50">
        <CardContent className="flex flex-col items-center justify-center py-12 text-center gap-6">
          <div className="flex flex-col items-center">
            <ShieldCheck className="h-12 w-12 text-muted-foreground mb-4 opacity-20" />
            <h3 className="text-lg font-medium">Clear Queue</h3>
            <p className="text-sm text-muted-foreground max-w-[280px]">
              No suggested edges awaiting review. The ontology is synchronized.
            </p>
          </div>

          <div className="w-full max-w-sm flex flex-col gap-3 border-t border-border/40 pt-6">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">
              Force a run
            </p>
            <Button
              variant="secondary"
              onClick={handleRunBatch}
              disabled={runningBatch || runningItem}
              className="w-full"
            >
              {runningBatch ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <Play className="h-4 w-4 mr-2" />
              )}
              Run ontology batch
            </Button>
            <div className="flex gap-2">
              <Input
                placeholder="item id"
                value={itemIdInput}
                onChange={(e) => setItemIdInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !runningItem && itemIdInput.trim()) {
                    e.preventDefault()
                    handleRunForItem()
                  }
                }}
                disabled={runningItem || runningBatch}
                className="font-mono text-xs"
              />
              <Button
                variant="outline"
                onClick={handleRunForItem}
                disabled={runningItem || runningBatch || !itemIdInput.trim()}
              >
                {runningItem ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Play className="h-4 w-4" />
                )}
              </Button>
            </div>
            <p className="text-[10px] text-muted-foreground text-left">
              Batch processes pending items. Single-item run force-re-extracts
              regardless of status.
            </p>
          </div>

          {lastRun && <LastRunDebug run={lastRun} />}
        </CardContent>
      </Card>
    )
  }

  return (
    <ScrollArea className="h-full pr-4">
      <div className="space-y-4 pb-8">
        <div className="flex items-center justify-between gap-4 px-1">
          <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <Network className="h-3.5 w-3.5" />
            <span>{edges.length} Suggestions</span>
          </div>
          <Select value={sortBy} onValueChange={setSortBy}>
            <SelectTrigger className="h-8 w-[180px] text-xs bg-background/50 border-primary/10">
              <div className="flex items-center gap-2">
                <SortDesc className="h-3.5 w-3.5 text-muted-foreground" />
                <SelectValue placeholder="Sort by" />
              </div>
            </SelectTrigger>
            <SelectContent align="end">
              <SelectItem value="confidence-desc" className="text-xs">
                <div className="flex items-center gap-2">
                  <SortDesc className="h-3.5 w-3.5" />
                  <span>Confidence (High-Low)</span>
                </div>
              </SelectItem>
              <SelectItem value="confidence-asc" className="text-xs">
                <div className="flex items-center gap-2">
                  <SortAsc className="h-3.5 w-3.5" />
                  <span>Confidence (Low-High)</span>
                </div>
              </SelectItem>
              <SelectItem value="alpha-asc" className="text-xs">
                <div className="flex items-center gap-2">
                  <ArrowDownAZ className="h-3.5 w-3.5" />
                  <span>Relation (A-Z)</span>
                </div>
              </SelectItem>
              <SelectItem value="alpha-desc" className="text-xs">
                <div className="flex items-center gap-2">
                  <ArrowUpAZ className="h-3.5 w-3.5" />
                  <span>Relation (Z-A)</span>
                </div>
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <AnimatePresence mode="popLayout">
          {sortedEdges.map((edge) => (
            <motion.div
              key={edge.id}
              layout
              initial={{ opacity: 0, scale: 0.95, y: 10 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.9, x: -20 }}
              transition={{ duration: 0.2 }}
            >
              <Card className="py-0 overflow-hidden border-primary/10 hover:border-primary/30 transition-all group bg-background/40 backdrop-blur-sm">
                <div className="flex items-stretch gap-0">
                  {/* Left Marker */}
                  <div className={cn(
                    "w-1 transition-colors",
                    (edge.metadata?.confidence as number) > 0.8 ? "bg-emerald-500/50" :
                      (edge.metadata?.confidence as number) > 0.5 ? "bg-amber-500/50" : "bg-primary/30"
                  )} />
                  <div className="flex-1 min-w-0 p-4">
                    <div className="grid grid-cols-[1fr_auto] gap-4 items-start">
                      <div className="min-w-0 space-y-4">
                        {/* Header info */}
                        <div className="flex items-center gap-3">
                          <Badge variant="outline" className="bg-primary/5 text-primary border-primary/20 flex gap-1 items-center px-1.5 py-0 h-5 text-[10px] font-bold uppercase tracking-wider shrink-0">
                            <Zap className="h-3 w-3" />
                            AI Inferred
                          </Badge>
                          <span className="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-widest truncate">
                            Confidence: {Math.round((edge.metadata?.confidence as number || 0) * 100)}%
                          </span>
                        </div>

                        {/* Relationship Flow - Using Grid to prevent overflow */}
                        <div className="grid grid-cols-[1fr_auto_1fr] items-center gap-2">
                          {/* Source Node */}
                          <div className="min-w-0 bg-muted/20 hover:bg-muted/40 rounded-lg p-2 border border-border/30 transition-colors relative group/node">
                            <div className="flex items-center justify-between gap-1">
                              <p className="text-xs font-bold truncate text-foreground/90" title={itemTitles[edge.source_id] || edge.source_id}>
                                {itemTitles[edge.source_id] || edge.source_id}
                              </p>
                              {onFocusNode && (
                                <Button 
                                  variant="ghost" 
                                  size="icon" 
                                  className="h-5 w-5 shrink-0 opacity-0 group-hover/node:opacity-100 transition-opacity hover:bg-primary/10"
                                  onClick={() => onFocusNode(edge.source_id)}
                                >
                                  <Compass className="h-3 w-3" />
                                </Button>
                              )}
                            </div>
                            {itemTitles[edge.source_id] && itemTitles[edge.source_id] !== edge.source_id && (
                              <p className="text-[9px] font-mono text-muted-foreground truncate opacity-50">
                                {edge.source_id}
                              </p>
                            )}
                          </div>

                          {/* Relationship Indicator */}
                          <div className="flex flex-col items-center px-1 shrink-0">
                            <Badge variant="secondary" className="font-mono text-[9px] px-1.5 py-0 h-4 bg-background border shadow-sm">
                              {edge.relationship}
                            </Badge>
                            <div className="w-6 h-px bg-primary/20 relative mt-1">
                              <div className="absolute right-0 top-1/2 -translate-y-1/2 size-0.5 rounded-full bg-primary/40" />
                            </div>
                          </div>

                          {/* Target Node */}
                          <div className="min-w-0 bg-muted/20 hover:bg-muted/40 rounded-lg p-2 border border-border/30 transition-colors relative group/node text-right">
                            <div className="flex items-center justify-between gap-1 flex-row-reverse">
                              <p className="text-xs font-bold truncate text-foreground/90" title={itemTitles[edge.target_id] || edge.target_id}>
                                {itemTitles[edge.target_id] || edge.target_id}
                              </p>
                              {onFocusNode && (
                                <Button 
                                  variant="ghost" 
                                  size="icon" 
                                  className="h-5 w-5 shrink-0 opacity-0 group-hover/node:opacity-100 transition-opacity hover:bg-primary/10"
                                  onClick={() => onFocusNode(edge.target_id)}
                                >
                                  <Compass className="h-3 w-3" />
                                </Button>
                              )}
                            </div>
                            {itemTitles[edge.target_id] && itemTitles[edge.target_id] !== edge.target_id && (
                              <p className="text-[9px] font-mono text-muted-foreground truncate opacity-50">
                                {edge.target_id}
                              </p>
                            )}
                          </div>
                        </div>

                        {/* Reasoning */}
                        {edge.metadata?.reasoning && (
                          <div className="text-[10px] text-muted-foreground italic leading-snug pl-2 border-l border-primary/20 py-0.5">
                            &quot;{edge.metadata.reasoning as string}&quot;
                          </div>
                        )}
                      </div>

                      {/* Actions */}
                      <div className="flex flex-col gap-2 shrink-0 border-l border-border/40 pl-3">
                        <Button
                          size="icon"
                          variant="ghost"
                          className="h-8 w-8 text-destructive hover:bg-destructive/10 rounded-full"
                          onClick={() => handleDelete(edge.id)}
                        >
                          <X className="h-4 w-4" />
                        </Button>
                        <Button
                          size="icon"
                          variant="default"
                          className="h-8 w-8 bg-emerald-500 hover:bg-emerald-600 shadow-lg shadow-emerald-500/20 rounded-full"
                          onClick={() => handleApprove(edge)}
                        >
                          <Check className="h-4 w-4" />
                        </Button>
                      </div>
                    </div>
                  </div>
                </div>
              </Card>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ScrollArea>
  )
}

interface LastRunDebugProps {
  run: {
    kind: "batch" | "item"
    target?: string
    startedAt: number
    elapsedMs: number
    report: OntologyRunReport
  }
}

function LastRunDebug({ run }: LastRunDebugProps) {
  const { report, kind, target, elapsedMs, startedAt } = run
  const noEdges =
    report.items_processed === 0 && report.items_skipped_no_neighbors === 0
  return (
    <div className="w-full max-w-sm border-t border-border/40 pt-4 text-left">
      <details open className="text-xs">
        <summary className="cursor-pointer text-muted-foreground hover:text-foreground select-none flex items-center justify-between">
          <span>
            Last run · {kind === "item" ? "single" : "batch"}
            {target ? ` · ${target}` : ""}
          </span>
          <span className="font-mono text-[10px] opacity-60">
            {new Date(startedAt).toLocaleTimeString()} · {elapsedMs} ms
          </span>
        </summary>
        <div className="mt-3 space-y-2 font-mono text-[11px]">
          <div className="grid grid-cols-2 gap-x-3 gap-y-1">
            <span className="text-muted-foreground">items processed</span>
            <span>{report.items_processed}</span>
            <span className="text-muted-foreground">skipped (no neighbors)</span>
            <span>{report.items_skipped_no_neighbors}</span>
            <span className="text-muted-foreground">edges committed</span>
            <span>{report.edges_committed.length}</span>
            <span className="text-muted-foreground">~tokens/call</span>
            <span>{report.estimated_input_tokens_per_item}</span>
          </div>

          {noEdges && (
            <div className="rounded border border-amber-500/30 bg-amber-500/5 p-2 text-[10px] text-amber-700 dark:text-amber-300 font-sans not-italic">
              Worker pulled 0 pending items. Either nothing has
              <code className="mx-1">ontology_status=pending</code> or the
              embedder is still warming up. Try the single-item form with a
              known id to force a re-run.
            </div>
          )}

          {report.edges_committed.length > 0 && (
            <div className="space-y-1.5">
              <div className="text-[10px] uppercase tracking-wide text-muted-foreground font-sans">
                Edges
              </div>
              <ul className="space-y-1.5">
                {report.edges_committed.map((e, i) => (
                  <li
                    key={`${e.from_id}-${e.predicate}-${e.to_id}-${i}`}
                    className="rounded bg-muted/40 p-2 space-y-1"
                  >
                    <div className="flex items-center gap-1 flex-wrap">
                      <span className="truncate max-w-[90px]" title={e.from_id}>
                        {e.from_id}
                      </span>
                      <span className="text-muted-foreground">—[</span>
                      <span className="text-primary">{e.predicate}</span>
                      <span className="text-muted-foreground">]→</span>
                      <span className="truncate max-w-[90px]" title={e.to_id}>
                        {e.to_id}
                      </span>
                      <Badge
                        variant={
                          e.status === "confirmed" ? "default" : "secondary"
                        }
                        className="ml-auto text-[9px] h-4 px-1"
                      >
                        {e.status} · {Math.round(e.confidence * 100)}%
                      </Badge>
                    </div>
                    {e.reasoning && (
                      <div className="text-muted-foreground italic font-sans text-[10px] leading-snug">
                        &quot;{e.reasoning}&quot;
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {report.debug && report.debug.length > 0 && (
            <div className="space-y-1.5">
              <div className="text-[10px] uppercase tracking-wide text-muted-foreground font-sans">
                Per-item trace
              </div>
              <ul className="space-y-2">
                {report.debug.map((d, i) => {
                  const drops = d.filter_drops
                  const totalDropped =
                    drops.bad_predicate +
                    drops.unknown_id +
                    drops.target_not_involved +
                    drops.self_loop +
                    drops.below_threshold
                  return (
                    <li
                      key={`${d.item_id}-${i}`}
                      className="rounded bg-muted/30 p-2 space-y-1.5"
                    >
                      <div
                        className="truncate text-[10px]"
                        title={d.item_id}
                      >
                        {d.item_id}
                      </div>
                      {d.error && (
                        <div className="rounded border border-destructive/30 bg-destructive/5 p-1.5 font-sans text-[10px] text-destructive">
                          {d.error}
                        </div>
                      )}
                      <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-[10px]">
                        <span className="text-muted-foreground">neighbors</span>
                        <span>{d.neighbors}</span>
                        <span className="text-muted-foreground">predicates seeded</span>
                        <span>{d.valid_predicates.length}</span>
                        {d.proposed_edges !== null && (
                          <>
                            <span className="text-muted-foreground">proposed by LLM</span>
                            <span>{d.proposed_edges}</span>
                          </>
                        )}
                      </div>
                      {totalDropped > 0 && (
                        <div className="space-y-0.5">
                          <div className="text-[10px] uppercase tracking-wide text-muted-foreground font-sans">
                            Filtered ({totalDropped})
                          </div>
                          <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-[10px]">
                            {drops.bad_predicate > 0 && (
                              <>
                                <span className="text-muted-foreground">predicate not in schema</span>
                                <span>{drops.bad_predicate}</span>
                              </>
                            )}
                            {drops.unknown_id > 0 && (
                              <>
                                <span className="text-muted-foreground">id hallucinated</span>
                                <span>{drops.unknown_id}</span>
                              </>
                            )}
                            {drops.target_not_involved > 0 && (
                              <>
                                <span className="text-muted-foreground">target not in edge</span>
                                <span>{drops.target_not_involved}</span>
                              </>
                            )}
                            {drops.self_loop > 0 && (
                              <>
                                <span className="text-muted-foreground">self-loop</span>
                                <span>{drops.self_loop}</span>
                              </>
                            )}
                            {drops.below_threshold > 0 && (
                              <>
                                <span className="text-muted-foreground">below confidence threshold</span>
                                <span>{drops.below_threshold}</span>
                              </>
                            )}
                          </div>
                        </div>
                      )}
                      {d.valid_predicates.length === 0 && (
                        <div className="rounded border border-amber-500/30 bg-amber-500/5 p-1.5 font-sans text-[10px] text-amber-700 dark:text-amber-300">
                          No predicates are seeded for this source_id — every
                          edge the LLM proposes will be dropped. Seed the
                          ontology predicate table first.
                        </div>
                      )}
                      {d.raw_llm_output && (
                        <details className="font-sans text-[10px]">
                          <summary className="cursor-pointer text-muted-foreground hover:text-foreground select-none">
                            Raw LLM output
                          </summary>
                          <pre className="mt-1 max-h-48 overflow-auto rounded bg-muted/60 p-1.5 text-[9px] whitespace-pre-wrap break-all">
                            {d.raw_llm_output}
                          </pre>
                        </details>
                      )}
                    </li>
                  )
                })}
              </ul>
            </div>
          )}
        </div>
      </details>
    </div>
  )
}
