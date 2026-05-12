"use client"

import { useState } from "react"
import Link from "next/link"
import { RefreshCw, Sparkles, Plus, X, Save } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { useReanalyzeItem, useUpdateItem, useCreateEdge } from "@/lib/api"
import { useSWRConfig } from "swr"
import { toast } from "sonner"
import type {
  Entry,
  StoreAnalysis,
  StoreAnalysisSuggestedEdge,
} from "@/lib/api/types"

interface AnalysisPanelProps {
  entry: Entry
}

const RELATION_COLOR: Record<string, string> = {
  agrees: "text-emerald-500 border-emerald-500/40",
  refines: "text-sky-500 border-sky-500/40",
  supersedes: "text-amber-500 border-amber-500/40",
  contradicts: "text-red-500 border-red-500/40",
  duplicates: "text-fuchsia-500 border-fuchsia-500/40",
  unrelated: "text-muted-foreground border-border",
}

export function AnalysisPanel({ entry }: AnalysisPanelProps) {
  const { mutate } = useSWRConfig()
  const { trigger: reanalyze, isMutating: reanalyzing } = useReanalyzeItem(entry.id)
  const { trigger: updateItem } = useUpdateItem(entry.id)
  const { trigger: createEdge } = useCreateEdge()

  const analysis: StoreAnalysis | null = entry.analysis ?? null

  const initialOverrides = {
    title: (entry.metadata.title as string | undefined) ?? analysis?.title ?? "",
    summary: (entry.metadata.summary as string | undefined) ?? analysis?.summary ?? "",
    doc_type: (entry.metadata.doc_type as string | undefined) ?? analysis?.doc_type ?? "",
    freshness: (entry.metadata.freshness as string | undefined) ?? analysis?.freshness ?? "",
    cluster_hint:
      (entry.metadata.cluster_hint as string | undefined) ?? analysis?.cluster_hint ?? "",
  }
  const initialTags = (() => {
    const meta = entry.metadata.tags
    if (typeof meta === "string" && meta.length > 0) {
      return meta.split(",").map((t) => t.trim()).filter(Boolean)
    }
    return analysis?.tags ?? []
  })()

  const [editing, setEditing] = useState(false)
  const [tags, setTags] = useState<string[]>(initialTags)
  const [tagDraft, setTagDraft] = useState("")
  const [overrides, setOverrides] = useState(initialOverrides)
  const [dismissedEdges, setDismissedEdges] = useState<Set<string>>(new Set())

  const handleReanalyze = async () => {
    try {
      const updated = await reanalyze()
      mutate(["item", entry.id], updated, { revalidate: false })
      toast.success("Re-analyzed")
    } catch (e) {
      toast.error(`Re-analyze failed: ${e instanceof Error ? e.message : "unknown"}`)
    }
  }

  const handleSaveRefinements = async () => {
    try {
      await updateItem({
        text: entry.text,
        source_id: entry.source_id,
        path: entry.path ?? undefined,
        metadata: {
          ...entry.metadata,
          tags: tags.join(", "),
          title: overrides.title || null,
          summary: overrides.summary || null,
          doc_type: overrides.doc_type || null,
          freshness: overrides.freshness || null,
          cluster_hint: overrides.cluster_hint || null,
        },
      })
      mutate(["item", entry.id])
      setEditing(false)
      toast.success("Refinements saved to metadata")
    } catch (e) {
      toast.error(`Save failed: ${e instanceof Error ? e.message : "unknown"}`)
    }
  }

  const handleApplyEdge = async (edge: StoreAnalysisSuggestedEdge) => {
    try {
      await createEdge({
        source_id: entry.id,
        target_id: edge.target_id,
        relationship: edge.rel || "related",
        weight: edge.weight,
        directed: true,
      })
      mutate(["edges-for-item", entry.id])
      setDismissedEdges((s) => new Set(s).add(edge.target_id))
      toast.success(`Edge to ${edge.target_id.slice(0, 16)}… created`)
    } catch (e) {
      toast.error(`Edge failed: ${e instanceof Error ? e.message : "unknown"}`)
    }
  }

  const addTag = () => {
    const t = tagDraft.trim()
    if (!t || tags.includes(t)) return
    setTags([...tags, t])
    setTagDraft("")
  }

  const removeTag = (t: string) => setTags(tags.filter((x) => x !== t))

  if (!analysis && !reanalyzing) {
    return (
      <div className="space-y-3">
        <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
          Analysis
        </h2>
        <div className="border border-dashed border-border p-6 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-muted-foreground">
            <Sparkles className="size-4" />
            <span className="font-mono text-xs">No analysis yet</span>
          </div>
          <Button size="sm" variant="outline" onClick={handleReanalyze}>
            <RefreshCw className="size-3.5 mr-1.5" /> Run analysis
          </Button>
        </div>
      </div>
    )
  }

  const a = analysis ?? ({} as StoreAnalysis)
  const at = entry.analysis_at ? new Date(entry.analysis_at).toLocaleString() : "—"

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
          Analysis
        </h2>
        <div className="flex items-center gap-2">
          <span className="font-mono text-[10px] text-muted-foreground">
            {entry.analysis_model ?? "—"} · {at}
          </span>
          <Button
            size="sm"
            variant={editing ? "default" : "outline"}
            onClick={editing ? handleSaveRefinements : () => setEditing(true)}
            className="h-7 font-mono text-[10px] uppercase tracking-wider"
          >
            {editing ? (
              <><Save className="size-3 mr-1" /> Save</>
            ) : (
              "Refine"
            )}
          </Button>
          {editing && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setEditing(false)
                setTags(initialTags)
                setOverrides(initialOverrides)
              }}
              className="h-7 font-mono text-[10px] uppercase tracking-wider"
            >
              Cancel
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={handleReanalyze}
            disabled={reanalyzing}
            className="h-7 font-mono text-[10px] uppercase tracking-wider"
          >
            <RefreshCw className={`size-3 mr-1 ${reanalyzing ? "animate-spin" : ""}`} />
            Re-run
          </Button>
        </div>
      </div>

      {/* Title / summary / facets */}
      <div className="border border-border bg-card p-4 space-y-3">
        {editing ? (
          <div className="space-y-3">
            <LabeledInput
              label="Title"
              value={overrides.title}
              onChange={(v) => setOverrides({ ...overrides, title: v })}
            />
            <LabeledTextarea
              label="Summary"
              value={overrides.summary}
              onChange={(v) => setOverrides({ ...overrides, summary: v })}
            />
            <div className="grid grid-cols-3 gap-2">
              <LabeledInput
                label="doc_type"
                value={overrides.doc_type}
                onChange={(v) => setOverrides({ ...overrides, doc_type: v })}
              />
              <LabeledInput
                label="freshness"
                value={overrides.freshness}
                onChange={(v) => setOverrides({ ...overrides, freshness: v })}
              />
              <LabeledInput
                label="cluster_hint"
                value={overrides.cluster_hint}
                onChange={(v) => setOverrides({ ...overrides, cluster_hint: v })}
              />
            </div>
          </div>
        ) : (
          <>
            {a.title && (
              <div>
                <FieldLabel>Title</FieldLabel>
                <p className="text-sm font-medium">{a.title}</p>
              </div>
            )}
            {a.summary && (
              <div>
                <FieldLabel>Summary</FieldLabel>
                <p className="text-sm text-foreground/90">{a.summary}</p>
              </div>
            )}
            <div className="flex flex-wrap gap-2">
              {a.doc_type && <Badge variant="outline">type: {a.doc_type}</Badge>}
              {a.freshness && <Badge variant="outline">freshness: {a.freshness}</Badge>}
              {a.cluster_hint && <Badge variant="outline">cluster: {a.cluster_hint}</Badge>}
              {a.quality && (
                <Badge variant="outline">
                  quality: {(a.quality.score * 100).toFixed(0)}%
                </Badge>
              )}
            </div>
            {a.quality?.issues && a.quality.issues.length > 0 && (
              <ul className="text-xs text-amber-500 list-disc pl-5">
                {a.quality.issues.map((i, idx) => <li key={idx}>{i}</li>)}
              </ul>
            )}
          </>
        )}
      </div>

      {/* Tags */}
      <div className="border border-border bg-card p-4 space-y-2">
        <FieldLabel>Tags</FieldLabel>
        <div className="flex flex-wrap gap-1.5">
          {tags.map((t) => (
            <span
              key={t}
              className="inline-flex items-center gap-1 px-2 py-0.5 border border-border text-xs font-mono"
            >
              {t}
              {editing && (
                <button onClick={() => removeTag(t)} className="hover:text-red-500">
                  <X className="size-3" />
                </button>
              )}
            </span>
          ))}
          {tags.length === 0 && (
            <span className="text-xs text-muted-foreground font-mono">none</span>
          )}
        </div>
        {editing && (
          <div className="flex gap-2">
            <Input
              value={tagDraft}
              onChange={(e) => setTagDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault()
                  addTag()
                }
              }}
              placeholder="add tag…"
              className="h-7 text-xs font-mono"
            />
            <Button size="sm" variant="outline" onClick={addTag} className="h-7">
              <Plus className="size-3" />
            </Button>
          </div>
        )}
      </div>

      {/* Verdicts */}
      {a.verdicts && a.verdicts.length > 0 && (
        <div className="border border-border bg-card p-4 space-y-2">
          <FieldLabel>Verdicts vs neighbors</FieldLabel>
          <div className="flex flex-col gap-1.5">
            {a.verdicts.map((v, idx) => (
              <div
                key={`${v.target_id}-${idx}`}
                className="flex items-start gap-2 border border-border p-2"
              >
                <span
                  className={`font-mono text-[10px] uppercase tracking-wider px-1.5 py-0.5 border shrink-0 ${
                    RELATION_COLOR[v.relation] ?? "text-muted-foreground border-border"
                  }`}
                >
                  {v.relation}
                </span>
                <div className="min-w-0 flex-1">
                  <Link
                    href={`/entries/${encodeURIComponent(v.target_id)}`}
                    className="font-mono text-xs hover:text-primary truncate block"
                  >
                    {v.target_id}
                  </Link>
                  <p className="text-xs text-muted-foreground mt-0.5">{v.reason}</p>
                </div>
                <span className="font-mono text-[10px] text-muted-foreground tabular-nums shrink-0">
                  {(v.confidence * 100).toFixed(0)}%
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Suggested edges */}
      {a.suggested_edges && a.suggested_edges.length > 0 && (
        <div className="border border-border bg-card p-4 space-y-2">
          <FieldLabel>Suggested edges</FieldLabel>
          <div className="flex flex-col gap-1.5">
            {a.suggested_edges
              .filter((e) => !dismissedEdges.has(e.target_id))
              .map((edge, idx) => (
                <div
                  key={`${edge.target_id}-${idx}`}
                  className="flex items-center gap-2 border border-border p-2"
                >
                  <span className="font-mono text-[10px] uppercase tracking-wider px-1.5 py-0.5 border border-primary/30 text-primary shrink-0">
                    {edge.rel}
                  </span>
                  <Link
                    href={`/entries/${encodeURIComponent(edge.target_id)}`}
                    className="font-mono text-xs hover:text-primary truncate flex-1 min-w-0"
                  >
                    {edge.target_id}
                  </Link>
                  <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
                    w={edge.weight.toFixed(2)}
                  </span>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => handleApplyEdge(edge)}
                    className="h-6 font-mono text-[10px] uppercase"
                  >
                    Apply
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      setDismissedEdges((s) => new Set(s).add(edge.target_id))
                    }
                    className="h-6 w-6 p-0"
                  >
                    <X className="size-3" />
                  </Button>
                </div>
              ))}
          </div>
        </div>
      )}

      {/* Raw debug (collapsed) */}
      {a.raw && (
        <details className="border border-border bg-card p-3">
          <summary className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground cursor-pointer">
            Raw model output
          </summary>
          <pre className="text-[10px] font-mono mt-2 whitespace-pre-wrap break-all text-muted-foreground max-h-60 overflow-auto">
            {a.raw}
          </pre>
        </details>
      )}
    </div>
  )
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
      {children}
    </span>
  )
}

function LabeledInput({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (v: string) => void
}) {
  return (
    <div className="space-y-1">
      <FieldLabel>{label}</FieldLabel>
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-8 text-xs font-mono"
      />
    </div>
  )
}

function LabeledTextarea({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (v: string) => void
}) {
  return (
    <div className="space-y-1">
      <FieldLabel>{label}</FieldLabel>
      <Textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="text-xs font-mono min-h-[60px]"
      />
    </div>
  )
}
