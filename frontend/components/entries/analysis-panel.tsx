"use client";

import { useState } from "react";
import Link from "next/link";
import {
  RefreshCw,
  Sparkles,
  Plus,
  X,
  Save,
  GitBranch,
  Terminal,
  AlertTriangle,
  CheckCircle2,
  Zap,
  ArrowUpCircle,
  AlertCircle,
  Copy,
  Ghost,
  Activity,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useReanalyzeItem, useUpdateItem, useCreateEdge } from "@/lib/api";
import { useSWRConfig } from "swr";
import { toast } from "sonner";
import { Editor } from "@monaco-editor/react";
import { EntryTagList } from "../ui/entry-tag";
import { cn } from "@/lib/utils";
import type {
  Entry,
  StoreAnalysis,
  StoreAnalysisSuggestedEdge,
  Edge,
  StoreAnalysisVerdict,
} from "@/lib/api/types";

interface AnalysisPanelProps {
  entry: Entry;
  edges?: Edge[];
}

const RELATION_CONFIG: Record<
  string,
  { color: string; icon: any; label: string }
> = {
  agrees: {
    color: "text-emerald-500 border-emerald-500/30 bg-emerald-500/5",
    icon: CheckCircle2,
    label: "Agrees",
  },
  refines: {
    color: "text-sky-500 border-sky-500/30 bg-sky-500/5",
    icon: Zap,
    label: "Refines",
  },
  supersedes: {
    color: "text-amber-500 border-amber-500/30 bg-amber-500/5",
    icon: ArrowUpCircle,
    label: "Supersedes",
  },
  contradicts: {
    color: "text-red-500 border-red-500/30 bg-red-500/5",
    icon: AlertCircle,
    label: "Contradicts",
  },
  duplicates: {
    color: "text-fuchsia-500 border-fuchsia-500/30 bg-fuchsia-500/5",
    icon: Copy,
    label: "Duplicate",
  },
  unrelated: {
    color: "text-muted-foreground border-border bg-muted/5",
    icon: Ghost,
    label: "Unrelated",
  },
};

export function AnalysisPanel({ entry, edges }: AnalysisPanelProps) {
  const { mutate } = useSWRConfig();
  const { trigger: reanalyze, isMutating: reanalyzing } = useReanalyzeItem(
    entry.id,
  );
  const { trigger: updateItem } = useUpdateItem(entry.id);
  const { trigger: createEdge } = useCreateEdge();

  const analysis: StoreAnalysis | null = entry.analysis ?? null;

  const initialOverrides = {
    title:
      (entry.metadata.title as string | undefined) ?? analysis?.title ?? "",
    summary:
      (entry.metadata.summary as string | undefined) ?? analysis?.summary ?? "",
    doc_type:
      (entry.metadata.doc_type as string | undefined) ??
      analysis?.doc_type ??
      "",
    freshness:
      (entry.metadata.freshness as string | undefined) ??
      analysis?.freshness ??
      "",
    cluster_hint:
      (entry.metadata.cluster_hint as string | undefined) ??
      analysis?.cluster_hint ??
      "",
  };
  const initialTags = (() => {
    const meta = entry.metadata.tags;
    if (typeof meta === "string" && meta.length > 0) {
      return meta
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
    }
    return analysis?.tags ?? [];
  })();

  const [editing, setEditing] = useState(false);
  const [tags, setTags] = useState<string[]>(initialTags);
  const [tagDraft, setTagDraft] = useState("");
  const [overrides, setOverrides] = useState(initialOverrides);
  const [dismissedEdges, setDismissedEdges] = useState<Set<string>>(new Set());

  const handleReanalyze = async () => {
    try {
      const updated = await reanalyze();
      mutate(["item", entry.id], updated, { revalidate: false });
      toast.success("Re-analyzed");
    } catch (e) {
      toast.error(
        `Re-analyze failed: ${e instanceof Error ? e.message : "unknown"}`,
      );
    }
  };

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
      });
      mutate(["item", entry.id]);
      setEditing(false);
      toast.success("Refinements saved to metadata");
    } catch (e) {
      toast.error(`Save failed: ${e instanceof Error ? e.message : "unknown"}`);
    }
  };

  const handleApplyEdge = async (edge: StoreAnalysisSuggestedEdge) => {
    try {
      await createEdge({
        source_id: entry.id,
        target_id: edge.target_id,
        relationship: edge.rel || "related",
        weight: edge.weight,
        directed: true,
      });
      mutate(["edges", entry.id]);
      setDismissedEdges((s) => new Set(s).add(edge.target_id));
      toast.success(`Edge to ${edge.target_id.slice(0, 16)}… created`);
    } catch (e) {
      toast.error(`Edge failed: ${e instanceof Error ? e.message : "unknown"}`);
    }
  };

  const addTag = () => {
    const t = tagDraft.trim();
    if (!t || tags.includes(t)) return;
    setTags([...tags, t]);
    setTagDraft("");
  };

  const removeTag = (t: string) => setTags(tags.filter((x) => x !== t));

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
    );
  }

  const a = analysis ?? ({} as StoreAnalysis);
  const at = entry.analysis_at
    ? new Date(entry.analysis_at).toLocaleString()
    : "—";

  const mergedRelated = (() => {
    const map = new Map<
      string,
      {
        id: string;
        title?: string | null;
        thumbnail?: string | null;
        sourceType?: string | null;
        neighborRelation?: string | null;
        isNeighbor: boolean;
        verdict?: StoreAnalysisVerdict;
        suggestedEdge?: StoreAnalysisSuggestedEdge;
      }
    >();

    // 1. Populate from neighbors
    if (entry.neighbors) {
      for (const n of entry.neighbors) {
        map.set(n.id, {
          id: n.id,
          title: n.title,
          thumbnail: n.thumbnail,
          sourceType: n.source_type,
          neighborRelation: n.relationship,
          isNeighbor: true,
        });
      }
    }

    // 2. Populate from verdicts
    if (a.verdicts) {
      for (const v of a.verdicts) {
        const existing = map.get(v.target_id);
        if (existing) {
          existing.verdict = v;
        } else {
          map.set(v.target_id, {
            id: v.target_id,
            isNeighbor: false,
            verdict: v,
          });
        }
      }
    }

    // 3. Populate from suggested edges
    if (a.suggested_edges) {
      for (const se of a.suggested_edges) {
        const existing = map.get(se.target_id);
        if (existing) {
          existing.suggestedEdge = se;
        } else {
          map.set(se.target_id, {
            id: se.target_id,
            isNeighbor: false,
            suggestedEdge: se,
          });
        }
      }
    }

    // Convert to array
    const items = Array.from(map.values());

    // Filter out dismissed suggested edges ONLY if they don't have any other relationship/verdict
    const activeItems = items
      .map((item) => {
        if (dismissedEdges.has(item.id)) {
          return {
            ...item,
            suggestedEdge: undefined,
          };
        }
        return item;
      })
      .filter((item) => {
        const hasInfo = item.isNeighbor || item.verdict || item.suggestedEdge;
        return hasInfo;
      });

    // Sort:
    // - Unrelated items last (verdict.relation === "unrelated")
    // - Non-unrelated items first
    // - Within each group, sort by max score (verdict confidence, suggested edge weight, or neighbor similarity 0.5)
    return activeItems.sort((x, y) => {
      const isXUnrelated = x.verdict?.relation === "unrelated";
      const isYUnrelated = y.verdict?.relation === "unrelated";

      if (isXUnrelated && !isYUnrelated) return 1;
      if (!isXUnrelated && isYUnrelated) return -1;

      const valX = Math.max(
        x.verdict?.confidence ?? 0,
        x.suggestedEdge?.weight ?? 0,
        x.isNeighbor ? 0.5 : 0,
      );
      const valY = Math.max(
        y.verdict?.confidence ?? 0,
        y.suggestedEdge?.weight ?? 0,
        y.isNeighbor ? 0.5 : 0,
      );
      if (valX !== valY) return valY - valX;

      return x.id.localeCompare(y.id);
    });
  })();

  const getRelationConfig = (item: (typeof mergedRelated)[0]) => {
    const rel =
      item.verdict?.relation ||
      item.suggestedEdge?.rel ||
      item.neighborRelation;
    if (!rel) return RELATION_CONFIG.unrelated;
    const r = rel.toLowerCase();
    return (
      RELATION_CONFIG[r] || {
        color: "text-primary border-primary/20 bg-primary/5",
        icon: GitBranch,
        label: rel,
      }
    );
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div className="flex flex-col gap-1">
          <h2 className="font-mono text-xs font-black uppercase tracking-[3px] text-primary/80">
            Intelligence Report
          </h2>
          <div className="flex items-center gap-2 font-mono text-[9px] uppercase tracking-widest text-muted-foreground/60">
            <span className="text-primary/60">
              {entry.analysis_model || "unknown-model"}
            </span>
            <span className="size-1 rounded-full bg-border" />
            <span>{at}</span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant={editing ? "default" : "outline"}
            onClick={editing ? handleSaveRefinements : () => setEditing(true)}
            className="h-7 px-3 font-mono text-[10px] uppercase tracking-widest font-black"
          >
            {editing ? (
              <>
                <Save className="size-3 mr-1.5" /> Save
              </>
            ) : (
              "Refine"
            )}
          </Button>
          {editing && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setEditing(false);
                setTags(initialTags);
                setOverrides(initialOverrides);
              }}
              className="h-7 px-3 font-mono text-[10px] uppercase tracking-widest text-muted-foreground"
            >
              Cancel
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={handleReanalyze}
            disabled={reanalyzing}
            className="h-7 px-3 font-mono text-[10px] uppercase tracking-widest border-primary/30 text-primary hover:bg-primary/5"
          >
            <RefreshCw
              className={`size-3 mr-1.5 ${reanalyzing ? "animate-spin" : ""}`}
            />
            Re-run
          </Button>
        </div>
      </div>

      <div className="relative overflow-hidden rounded-xl border border-border bg-card/50 dark:bg-black/40 backdrop-blur-md p-6 shadow-sm dark:shadow-[0_0_30px_rgba(var(--primary-rgb),0.02)]">
        {editing ? (
          <div className="space-y-4">
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
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
              <LabeledInput
                label="Type"
                value={overrides.doc_type}
                onChange={(v) => setOverrides({ ...overrides, doc_type: v })}
              />
              <LabeledInput
                label="Freshness"
                value={overrides.freshness}
                onChange={(v) => setOverrides({ ...overrides, freshness: v })}
              />
              <LabeledInput
                label="Cluster"
                value={overrides.cluster_hint}
                onChange={(v) =>
                  setOverrides({ ...overrides, cluster_hint: v })
                }
              />
            </div>
          </div>
        ) : (
          <div className="space-y-6">
            <div className="space-y-2">
              <FieldLabel>Executive Summary</FieldLabel>
              <h3 className="text-lg font-bold text-foreground/90">
                {a.title || "No Title Extracted"}
              </h3>
              <p className="text-sm leading-relaxed text-muted-foreground italic font-serif">
                {a.summary || "No summary available."}
              </p>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 pt-4 border-t border-border/40">
              <div className="space-y-1">
                <FieldLabel>Doc Type</FieldLabel>
                <div className="text-xs font-mono font-bold text-foreground/80 uppercase">
                  {a.doc_type || "—"}
                </div>
              </div>
              <div className="space-y-1">
                <FieldLabel>Freshness</FieldLabel>
                <div className="text-xs font-mono font-bold text-foreground/80 uppercase">
                  {a.freshness || "—"}
                </div>
              </div>
              <div className="space-y-1">
                <FieldLabel>Cluster</FieldLabel>
                <div className="text-xs font-mono font-bold text-foreground/80 uppercase truncate">
                  {a.cluster_hint || "—"}
                </div>
              </div>
              <div className="space-y-1">
                <FieldLabel>Quality</FieldLabel>
                <div className="flex flex-col gap-1">
                  <div className="text-xs font-mono font-bold text-primary">
                    {a.quality ? `${(a.quality.score * 100).toFixed(0)}%` : "—"}
                  </div>
                </div>
              </div>
            </div>

            {a.quality?.issues && a.quality.issues.length > 0 && (
              <div className="mt-4 p-3 bg-red-500/5 border border-red-500/20 rounded-lg">
                <div className="flex items-center gap-2 mb-2">
                  <AlertTriangle className="size-3 text-red-500" />
                  <span className="font-mono text-[9px] font-black uppercase tracking-widest text-red-500">
                    Quality Alerts
                  </span>
                </div>
                <ul className="text-xs text-red-400 space-y-1 list-none">
                  {a.quality.issues.map((i, idx) => (
                    <li key={idx} className="flex gap-2">
                      <span className="opacity-50">•</span>
                      {i}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Tags */}
      <div className="space-y-3">
        <FieldLabel>Tags</FieldLabel>
        <div className="flex flex-wrap gap-2">
          <EntryTagList
            tags={tags}
            onRemoveTag={editing ? removeTag : undefined}
          />
          {editing && (
            <div className="flex gap-2 w-full sm:w-auto">
              <Input
                value={tagDraft}
                onChange={(e) => setTagDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    addTag();
                  }
                }}
                placeholder="add tag…"
                className="h-7 text-[10px] font-mono bg-muted/20 border-border/40 w-32"
              />
              <Button
                size="sm"
                variant="outline"
                onClick={addTag}
                className="h-7 w-7 p-0 border-primary/20 text-primary"
              >
                <Plus className="size-3" />
              </Button>
            </div>
          )}
        </div>
      </div>

      {/* Related & Similar Entries */}
      {mergedRelated.length > 0 && (
        <div className="space-y-4">
          <div className="flex items-center gap-2">
            <GitBranch className="size-3.5 text-primary" />
            <FieldLabel>Related & Similar Entries</FieldLabel>
          </div>
          <div className="grid grid-cols-1 gap-3">
            {mergedRelated.map((item) => {
              const isUnrelated = item.verdict?.relation === "unrelated";
              const config = getRelationConfig(item);
              const Icon = config.icon;

              const existingEdge = edges?.find(
                (edge) =>
                  (edge.source_id === entry.id && edge.target_id === item.id) ||
                  (edge.target_id === entry.id && edge.source_id === item.id),
              );
              const isConnected = !!existingEdge;

              return (
                <div
                  key={item.id}
                  className={cn(
                    "relative group flex flex-col md:flex-row gap-4 justify-between items-start md:items-center border p-4 transition-all duration-300 rounded-xl bg-card/40 hover:bg-card/75 dark:hover:bg-black/60 shadow-sm",
                    config.color,
                    isUnrelated &&
                      "opacity-60 hover:opacity-100 grayscale-[0.5] hover:grayscale-0 bg-muted/5 border-border text-muted-foreground",
                  )}
                >
                  <div className="flex gap-3 flex-1 min-w-0">
                    {item.thumbnail && (
                      <div className="size-12 border border-border/40 overflow-hidden bg-muted/50 rounded-lg shrink-0">
                        <img
                          src={item.thumbnail}
                          alt=""
                          className="size-full object-cover grayscale group-hover:grayscale-0 transition-all duration-500"
                        />
                      </div>
                    )}
                    <div className="space-y-1 min-w-0 flex-1">
                      <div className="flex items-center gap-2 flex-wrap">
                        <Link
                          href={`/entries/${encodeURIComponent(item.id)}`}
                          className="font-mono text-xs font-bold text-foreground hover:text-primary transition-colors truncate block max-w-[200px] sm:max-w-[300px]"
                          title={item.id}
                        >
                          {item.title || item.id}
                        </Link>
                        {item.title && (
                          <span className="font-mono text-[9px] text-muted-foreground/60 truncate max-w-[120px]">
                            ({item.id.slice(0, 12)}...)
                          </span>
                        )}
                        {item.isNeighbor && !item.neighborRelation && (
                          <Badge
                            variant="outline"
                            className="h-4 px-1.5 font-mono text-[8px] uppercase tracking-tighter border-muted-foreground/20 text-muted-foreground/80 leading-none"
                          >
                            Similar
                          </Badge>
                        )}
                        {isConnected && (
                          <Badge
                            variant="outline"
                            className="h-4 px-1.5 font-mono text-[8px] uppercase tracking-tighter border-emerald-500/30 text-emerald-500 bg-emerald-500/5 leading-none"
                          >
                            Connected: {existingEdge.relationship}
                          </Badge>
                        )}
                      </div>
                      {item.verdict?.reason && (
                        <p className="text-xs leading-relaxed text-muted-foreground/80 italic pr-4">
                          {item.verdict.reason}
                        </p>
                      )}
                    </div>
                  </div>

                  <div className="flex items-center gap-4 shrink-0 w-full md:w-auto justify-between md:justify-end border-t md:border-t-0 pt-3 md:pt-0 border-border/10">
                    <div className="flex flex-col gap-1 items-start md:items-end">
                      <div className="flex items-center gap-1.5">
                        <Icon className="size-3.5" />
                        <span className="font-mono text-[9px] font-black uppercase tracking-wider">
                          {config.label}
                        </span>
                      </div>
                      {item.verdict && (
                        <div className="flex items-center gap-2">
                          <div className="w-16 h-1 bg-current/10 rounded-full overflow-hidden hidden sm:block">
                            <div
                              className="h-full bg-current transition-all duration-1000"
                              style={{
                                width: `${item.verdict.confidence * 100}%`,
                              }}
                            />
                          </div>
                          <span className="font-mono text-[9px] tabular-nums opacity-60">
                            {(item.verdict.confidence * 100).toFixed(0)}% conf
                          </span>
                        </div>
                      )}
                      {item.suggestedEdge && !item.verdict && (
                        <span className="font-mono text-[9px] text-muted-foreground/60">
                          w={item.suggestedEdge.weight.toFixed(2)}
                        </span>
                      )}
                    </div>

                    {item.suggestedEdge && !isConnected && (
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handleApplyEdge(item.suggestedEdge!)}
                          className="h-7 px-3 font-mono text-[10px] uppercase tracking-widest border-current/20 text-foreground hover:bg-current/10 hover:text-foreground transition-all duration-200"
                        >
                          Apply
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() =>
                            setDismissedEdges((s) => new Set(s).add(item.id))
                          }
                          className="h-7 w-7 p-0 text-muted-foreground hover:text-red-500 transition-all duration-200"
                        >
                          <X className="size-3.5" />
                        </Button>
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Raw Intelligence Data */}
      {a.raw && (
        <details className="group space-y-3">
          <summary className="flex items-center gap-2 font-mono text-[10px] font-black uppercase tracking-[2px] text-muted-foreground hover:text-primary cursor-pointer transition-colors list-none">
            <Terminal className="size-3.5" />
            Raw Model Output
            <span className="ml-auto text-[8px] opacity-0 group-open:opacity-100">
              READONLY MONACO
            </span>
          </summary>
          <div className="rounded-lg border border-border bg-muted/20 dark:bg-black/40 overflow-hidden shadow-inner">
            <Editor
              height="300px"
              defaultLanguage="json"
              theme="vs-dark"
              value={a.raw}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 11,
                fontFamily: "var(--font-mono)",
                lineNumbers: "on",
                scrollBeyondLastLine: false,
                padding: { top: 12, bottom: 12 },
                //backgroundColor: "#00000000",
                domReadOnly: true,
              }}
            />
          </div>
        </details>
      )}
    </div>
  );
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
      {children}
    </span>
  );
}

function LabeledInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
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
  );
}

function LabeledTextarea({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
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
  );
}
