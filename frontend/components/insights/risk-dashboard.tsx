"use client"

import { useMemo } from "react"
import { AlertTriangle, ShieldCheck, ShieldX } from "lucide-react"
import { cn } from "@/lib/utils"
import type { HarnessTreeNode, HarnessTreeResponse } from "@/lib/api"
import {
  SEVERITY_COLOR,
  SEVERITY_RANK,
  repoOf,
  type HarnessIndex,
} from "@/components/insights/use-harness-index"

type ValidationStatus = "done" | "needed" | "none"

interface RiskRow {
  node: HarnessTreeNode
  poc: HarnessTreeNode | null
  validation: { node: HarnessTreeNode; status: ValidationStatus } | null
}

function severityOf(risk: HarnessTreeNode): string {
  const severity = risk.data?.severity
  return (typeof severity === "string" ? severity : "medium").toLowerCase()
}

function StatCard({
  label,
  value,
  tone,
  icon,
}: {
  label: string
  value: number | string
  tone?: string
  icon: React.ReactNode
}) {
  return (
    <div className="flex items-center gap-3 rounded-sm border px-3 py-2">
      <span className={cn("flex h-7 w-7 items-center justify-center rounded-sm border", tone ?? "text-muted-foreground")}>
        {icon}
      </span>
      <div>
        <p className="font-mono text-lg leading-none">{value}</p>
        <p className="text-[10px] uppercase tracking-widest text-muted-foreground">{label}</p>
      </div>
    </div>
  )
}

export function RiskDashboard({
  index,
  repoId,
  onSelect,
}: {
  index: HarnessIndex
  repoId: string | null
  onSelect: (id: string) => void
}) {
  const rows = useMemo<RiskRow[]>(() => {
    const rows: RiskRow[] = []
    for (const node of index.byId.values()) {
      if (node.type_name !== "harness_risk") continue
      if (repoId && repoOf(index, node.id)?.id !== repoId) continue
      const pocLink = (index.parentsOf.get(node.id) ?? []).find((p) => p.relation === "RAISED")
      const poc = pocLink ? index.byId.get(pocLink.from) ?? null : null
      const valEdges = (index.childrenOf.get(node.id) ?? []).filter(
        (e) => e.relation === "ADDRESSED_BY"
      )
      let validation: RiskRow["validation"] = null
      for (const edge of valEdges) {
        const val = index.byId.get(edge.to_item_id)
        if (!val) continue
        const raw = val.data?.status
        const status = (raw === "done" ? "done" : "needed") as ValidationStatus
        if (!validation || (validation.status !== "done" && status === "done")) {
          validation = { node: val, status }
        }
      }
      rows.push({ node, poc, validation })
    }
    rows.sort((a, b) => {
      const sev =
        (SEVERITY_RANK[severityOf(a.node)] ?? 9) - (SEVERITY_RANK[severityOf(b.node)] ?? 9)
      return sev || b.node.updated_at - a.node.updated_at
    })
    return rows
  }, [index, repoId])

  const stats = useMemo(() => {
    const blocking = rows.filter((r) => ["critical", "high"].includes(severityOf(r.node))).length
    const unaddressed = rows.filter((r) => r.validation?.status !== "done").length
    const addressed = rows.filter((r) => r.validation?.status === "done").length
    return { total: rows.length, blocking, unaddressed, addressed }
  }, [rows])

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatCard label="Risks tracked" value={stats.total} icon={<AlertTriangle className="h-4 w-4" />} />
        <StatCard label="Critical / high" value={stats.blocking} tone="text-red-500 border-red-500/40 bg-red-500/10" icon={<AlertTriangle className="h-4 w-4" />} />
        <StatCard label="Unaddressed" value={stats.unaddressed} tone="text-amber-500 border-amber-500/40 bg-amber-500/10" icon={<ShieldX className="h-4 w-4" />} />
        <StatCard label="Addressed" value={stats.addressed} tone="text-emerald-500 border-emerald-500/40 bg-emerald-500/10" icon={<ShieldCheck className="h-4 w-4" />} />
      </div>

      {rows.length === 0 ? (
        <p className="py-8 text-center font-mono text-xs text-muted-foreground">
          No harness_risk nodes {repoId ? "for this repo" : "in the graph yet"}.
        </p>
      ) : (
        <div className="overflow-hidden rounded-sm border">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b bg-accent/30 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                <th className="px-3 py-2">Severity</th>
                <th className="px-3 py-2">Risk</th>
                <th className="px-3 py-2">POC session</th>
                <th className="px-3 py-2">Validation</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const severity = severityOf(row.node)
                return (
                  <tr
                    key={row.node.id}
                    onClick={() => onSelect(row.node.id)}
                    className="cursor-pointer border-b border-border/50 last:border-0 hover:bg-accent/40"
                  >
                    <td className="px-3 py-2">
                      <span
                        className={cn(
                          "rounded-sm border px-1.5 py-0.5 font-mono text-[10px] uppercase",
                          SEVERITY_COLOR[severity] ?? "text-muted-foreground"
                        )}
                      >
                        {severity}
                      </span>
                    </td>
                    <td className="max-w-72 px-3 py-2">
                      <p className="truncate">{row.node.title}</p>
                      <p className="font-mono text-[10px] text-muted-foreground">{row.node.id}</p>
                    </td>
                    <td className="px-3 py-2 font-mono text-[10px] text-muted-foreground">
                      {row.poc ? (
                        <button
                          type="button"
                          className="underline hover:text-foreground"
                          onClick={(e) => {
                            e.stopPropagation()
                            onSelect(row.poc!.id)
                          }}
                        >
                          {row.poc.title}
                        </button>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td className="px-3 py-2">
                      {row.validation ? (
                        <span
                          className={cn(
                            "rounded-sm border px-1.5 py-0.5 font-mono text-[10px] uppercase",
                            row.validation.status === "done"
                              ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-500"
                              : "border-amber-500/40 bg-amber-500/10 text-amber-500"
                          )}
                        >
                          {row.validation.status}
                        </span>
                      ) : (
                        <span className="font-mono text-[10px] uppercase text-red-400">
                          unaddressed
                        </span>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
