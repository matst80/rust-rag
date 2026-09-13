"use client"

import { useMemo, useState } from "react"
import Link from "next/link"
import {
  AlertTriangle,
  CheckCircle2,
  FileWarning,
  Loader2,
  Scale,
  ShieldCheck,
  XCircle,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { api, useItem } from "@/lib/api"
import type {
  EntryMetadata,
  HarnessAuditVerdict,
  HarnessTreeNode,
  HarnessTreeResponse,
} from "@/lib/api"
import { GrillGraph } from "@/components/grill/grill-graph"
import { SprintBrief } from "@/components/grill/sprint-brief"
import { MarkdownView } from "@/components/entries/markdown-view"

function SeverityBadge({ severity }: { severity: string }) {
  return severity === "BLOCKING" ? (
    <Badge variant="destructive" className="font-mono text-[10px]">
      BLOCKING
    </Badge>
  ) : (
    <Badge className="border-amber-500/30 bg-amber-500/10 font-mono text-[10px] text-amber-500">
      WARNING
    </Badge>
  )
}

function VerdictHeader({ verdict }: { verdict: HarnessAuditVerdict }) {
  const blocking = verdict.violations.some((v) => v.severity === "BLOCKING")
  return (
    <div className="flex items-center gap-3 border-b pb-2">
      {blocking ? (
        <XCircle className="h-4 w-4 text-red-500" />
      ) : verdict.passed ? (
        <CheckCircle2 className="h-4 w-4 text-emerald-500" />
      ) : (
        <AlertTriangle className="h-4 w-4 text-amber-500" />
      )}
      <span className="text-sm font-semibold uppercase tracking-wider">
        {blocking ? "Grill failed" : verdict.passed ? "Grill passed" : "Grill warnings"}
      </span>
      <span className="ml-auto font-mono text-xs text-muted-foreground">
        score {(verdict.score * 100).toFixed(0)}% · audit {verdict.audit_id}
      </span>
    </div>
  )
}

function ViolationCard({
  violation,
  onApplyEdit,
  onExempt,
  busy,
}: {
  violation: HarnessAuditVerdict["violations"][number]
  onApplyEdit: (docId: string, suggestion: string) => void
  onExempt: (docId: string) => void
  busy: boolean
}) {
  return (
    <div className="rounded-sm border border-red-500/30 bg-red-500/5 p-3 text-xs">
      <div className="mb-2 flex items-center gap-2">
        <FileWarning className="h-3.5 w-3.5 text-red-500" />
        <span className="font-mono font-semibold">{violation.title}</span>
        <SeverityBadge severity={violation.severity} />
        <Link
          href={`/entries/${violation.doc_id}`}
          className="ml-auto font-mono text-[10px] text-muted-foreground underline hover:text-foreground"
        >
          {violation.doc_id}
        </Link>
      </div>
      <p className="text-foreground/90">{violation.violation_reason}</p>
      <div className="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => onApplyEdit(violation.doc_id, violation.violation_reason)}
          className="rounded-sm border border-primary/40 bg-primary/10 px-2 py-1 font-mono text-[10px] uppercase tracking-wider text-primary hover:bg-primary/20 disabled:opacity-50"
        >
          {busy ? <Loader2 className="mr-1 inline h-3 w-3 animate-spin" /> : null}
          Apply Suggested Edit
        </button>
        <Link
          href={`/entries/${violation.doc_id}/edit`}
          className="rounded-sm border px-2 py-1 font-mono text-[10px] uppercase tracking-wider text-muted-foreground hover:text-foreground"
        >
          Amend ADR
        </Link>
        <button
          type="button"
          disabled={busy}
          onClick={() => onExempt(violation.doc_id)}
          className="rounded-sm border px-2 py-1 font-mono text-[10px] uppercase tracking-wider text-muted-foreground hover:text-foreground disabled:opacity-50"
        >
          Record Manual Exemption
        </button>
      </div>
    </div>
  )
}

function DataPayload({ node }: { node: HarnessTreeNode }) {
  if (!node.data || Object.keys(node.data).length === 0) return null
  return (
    <div className="rounded-sm border p-3 text-xs">
      <p className="mb-2 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
        Structured payload
      </p>
      <dl className="grid grid-cols-[max-content_1fr] items-baseline gap-x-4 gap-y-1">
        {Object.entries(node.data)
          .filter(([key]) => key !== "session_id")
          .map(([key, value]) => (
            <div key={key} className="col-span-2 grid grid-cols-subgrid">
              <dt className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                {key.replace(/_/g, " ")}
              </dt>
              <dd className="break-words font-mono text-foreground/90">
                {typeof value === "object" && value !== null
                  ? JSON.stringify(value)
                  : String(value)}
              </dd>
            </div>
          ))}
      </dl>
    </div>
  )
}

function ContentPreview({ node }: { node: HarnessTreeNode }) {
  const { data: full, isLoading } = useItem(node.id)
  const text = full?.text?.trim()

  return (
    <div className="flex flex-col gap-3">
      {isLoading && !text && (
        <div className="flex items-center gap-2 py-4 text-xs text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin" /> loading content…
        </div>
      )}
      {text ? (
        <div className="rounded-sm border p-3">
          <p className="mb-2 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
            Content
          </p>
          <MarkdownView content={text} className="text-xs" />
        </div>
      ) : null}
      <DataPayload node={node} />
      {node.source_id && (
        <p className="font-mono text-[10px] text-muted-foreground/70">
          source: {node.source_id}
        </p>
      )}
    </div>
  )
}

export function AuditPanel({
  tree,
  selectedId,
  onSelect,
  onChanged,
}: {
  tree: HarnessTreeResponse | null
  selectedId: string | null
  onSelect: (id: string) => void
  onChanged: () => void
}) {
  const node = useMemo(
    () => tree?.nodes.find((n) => n.id === selectedId) ?? null,
    [tree, selectedId]
  )
  const { data: full } = useItem(selectedId)
  const [busy, setBusy] = useState(false)

  // Audit nodes carry their own verdict in `data`; regular nodes resolve the
  // latest AUDITED verdict server-side.
  const verdict: HarnessAuditVerdict | null = useMemo(() => {
    if (!node) return null
    if (node.type_name === "harness_audit") {
      const data = (full?.data ?? {}) as Record<string, unknown>
      return {
        audit_id: node.id,
        passed: Boolean(data.passed),
        score: Number(data.score ?? 0),
        violations: (data.violations as HarnessAuditVerdict["violations"]) ?? [],
        unanchored_assumptions: (data.unanchored_assumptions as string[]) ?? [],
        audited_at: Number(data.audited_at ?? node.updated_at),
      }
    }
    return node.verdict ?? null
  }, [node, full])

  const updateMetadata = async (
    itemId: string,
    transform: (metadata: Record<string, unknown>) => Record<string, unknown>
  ) => {
    setBusy(true)
    try {
      const item = await api.items.get(itemId)
      await api.items.update(itemId, {
        text: item.text,
        metadata: transform(item.metadata) as EntryMetadata,
        source_id: item.source_id,
        path: item.path ?? null,
        type: item.type ?? null,
        data: (item.data as Record<string, unknown> | null) ?? null,
      })
      onChanged()
    } finally {
      setBusy(false)
    }
  }

  const applyEdit = (docId: string, suggestion: string) =>
    updateMetadata(docId, (metadata) => ({
      ...metadata,
      harness_suggested_edit: suggestion,
    }))

  const recordExemption = (docId: string) =>
    updateMetadata(docId, (metadata) => {
      const exemptions = Array.isArray(metadata.harness_exemptions)
        ? (metadata.harness_exemptions as unknown[])
        : []
      return {
        ...metadata,
        harness_exemptions: [
          ...exemptions,
          { doc_id: docId, target: node?.id, at: Date.now() },
        ],
      }
    })

  return (
    <div className="flex h-full flex-col">
      <div className="border-b px-4 py-2">
        <h2 className="text-xs font-semibold uppercase tracking-[3px]">
          Node Inspector
        </h2>
        <p className="truncate font-mono text-[10px] text-muted-foreground">
          {node
            ? `${node.type_name} · ${node.id}${node.state ? ` · ${node.state}` : ""}`
            : "no node selected"}
        </p>
      </div>
      <Tabs defaultValue="overview" className="flex flex-1 flex-col gap-0">
        <TabsList className="mx-4 mt-2 h-7 w-fit">
          <TabsTrigger value="overview" className="h-5 font-mono text-[10px]">
            Overview
          </TabsTrigger>
          <TabsTrigger value="verdict" className="h-5 font-mono text-[10px]">
            Compiler Errors
          </TabsTrigger>
          <TabsTrigger value="graph" className="h-5 font-mono text-[10px]">
            Graph
          </TabsTrigger>
        </TabsList>
        <TabsContent value="overview" className="flex-1 overflow-hidden">
          {!node && (
            <div className="flex h-full items-center justify-center px-6 text-center text-xs text-muted-foreground">
              Select a node in the tree to preview its content — sprints get an
              auto-generated brief with todos, risks and session evidence.
            </div>
          )}
          {node && node.type_name === "harness_sprint" && tree && (
            <SprintBrief tree={tree} sprint={node} />
          )}
          {node && node.type_name !== "harness_sprint" && (
            <div className="h-full overflow-y-auto px-4 py-3">
              <ContentPreview node={node} />
            </div>
          )}
        </TabsContent>
        <TabsContent value="verdict" className="flex-1 overflow-y-auto px-4 py-3">
          {!node && (
            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
              Select a node in the tree to inspect its grill verdict.
            </div>
          )}
          {node && !verdict && (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-xs text-muted-foreground">
              <ShieldCheck className="h-5 w-5" />
              No grill audit recorded for this node yet.
              {node.badge === "yellow" && (
                <span className="text-amber-500">
                  Unanchored: no GOVERNED_BY / ENFORCES_DOC edge found.
                </span>
              )}
            </div>
          )}
          {node && verdict && (
            <div className="flex flex-col gap-3">
              <VerdictHeader verdict={verdict} />
              {verdict.violations.length === 0 && (
                <div className="rounded-sm border border-emerald-500/30 bg-emerald-500/5 p-3 text-xs text-emerald-500">
                  <Scale className="mr-2 inline h-3.5 w-3.5" />
                  No invariant collisions detected against governing docs.
                </div>
              )}
              {verdict.violations.map((violation, index) => (
                <ViolationCard
                  key={`${violation.doc_id}-${index}`}
                  violation={violation}
                  onApplyEdit={applyEdit}
                  onExempt={recordExemption}
                  busy={busy}
                />
              ))}
              {verdict.unanchored_assumptions.length > 0 && (
                <div className="rounded-sm border p-3 text-xs">
                  <p className="mb-2 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                    Unanchored assumptions
                  </p>
                  <ul className="list-inside list-disc space-y-1 text-foreground/90">
                    {verdict.unanchored_assumptions.map((assumption, index) => (
                      <li key={index}>{assumption}</li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </TabsContent>
        <TabsContent value="graph" className="flex-1 overflow-hidden">
          {selectedId ? (
            <GrillGraph centerId={selectedId} onNodeClick={onSelect} tree={tree} />
          ) : (
            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
              Select a node to expand its 2-hop neighborhood.
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  )
}
