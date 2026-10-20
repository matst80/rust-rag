"use client"

import type { HarnessTreeNode } from "@/lib/api"

/**
 * Single source of truth for harness graph vocabulary on the frontend.
 *
 * The server normalizes legacy concern types to `harness_concern` at tree
 * assembly, but edges stored under pre-fold relation names still exist —
 * match them here instead of hand-maintaining alias lists per component.
 */

/** Consolidated concern type (replaces the six legacy per-kind types). */
export const HARNESS_CONCERN = "harness_concern"

/** `kind` values for `harness_concern` — one per legacy concern type. */
export const CONCERN_KINDS = [
  "risk",
  "compliance",
  "resource",
  "scaling",
  "validation",
  "rollout",
] as const

export type ConcernKind = (typeof CONCERN_KINDS)[number]

const LEGACY_CONCERN_KIND: Record<string, ConcernKind> = {
  harness_risk: "risk",
  harness_compliance: "compliance",
  harness_resource: "resource",
  harness_scaling: "scaling",
  harness_validation: "validation",
  harness_rollout: "rollout",
}

/** Legacy relation names folded into a canonical equivalent (or `contains`
 *  for the deprecated hierarchy relations BREAKS_INTO / HAD_POC). */
const RELATION_FOLDS: Record<string, string> = {
  contains_todo: "contains",
  has_plan: "contains",
  breaks_into: "contains",
  had_poc: "contains",
  conflicts_with: "contradicts",
  requires: "depends_on",
  derives_from: "depends_on",
  governed_by: "enforces_doc",
  supersedes: "supersedes",
}

/** Lowercase + fold legacy aliases, so `HAD_POC`, `had_poc`, `contains`
 *  etc. all compare equal. Returns "" for missing relations. */
export function normalizeRelation(relation: string | null | undefined): string {
  const lower = (relation ?? "").trim().toLowerCase()
  if (!lower) return ""
  return RELATION_FOLDS[lower] ?? lower
}

/** True when the edge relation matches any of the given names (each run
 *  through {@link normalizeRelation}, so legacy aliases just work). */
export function relIs(relation: string | null | undefined, ...names: string[]): boolean {
  const n = normalizeRelation(relation)
  return names.some((name) => normalizeRelation(name) === n)
}

/** Concern kind of a tree node: `data.kind` for `harness_concern`, the
 *  legacy mapping for pre-consolidation types, else null. */
export function concernKindOf(node: Pick<HarnessTreeNode, "type_name" | "data">): ConcernKind | null {
  if (node.type_name === HARNESS_CONCERN) {
    const kind = node.data?.kind
    if (typeof kind === "string" && (CONCERN_KINDS as readonly string[]).includes(kind)) {
      return kind as ConcernKind
    }
    return null
  }
  return LEGACY_CONCERN_KIND[node.type_name] ?? null
}

/** True for consolidated concerns and legacy per-kind concern types. */
export function isConcernNode(node: Pick<HarnessTreeNode, "type_name" | "data">): boolean {
  return node.type_name === HARNESS_CONCERN || node.type_name in LEGACY_CONCERN_KIND
}

/** Short uppercase label for the plan tree / inspector header. Concerns
 *  render as one type — the kind shows as a separate chip. */
export function harnessTypeLabel(node: Pick<HarnessTreeNode, "type_name" | "data">): string {
  if (isConcernNode(node)) return "CONCERN"
  const labels: Record<string, string> = {
    harness_plan: "PLAN",
    harness_sprint: "SPRINT",
    harness_todo: "TODO",
    harness_doc: "DOC",
    harness_agent: "AGENT",
    harness_stream: "STREAM",
    harness_audit: "AUDIT",
    harness_repo: "REPO",
    harness_poc: "POC",
    decision: "DECISION",
    harness_evidence: "EVIDENCE",
  }
  return labels[node.type_name] ?? node.type_name.toUpperCase()
}

/** Node fill color for the reagraph views. One color per family —
 *  concerns share a single color instead of six. */
export function harnessTypeColor(typeName: string): string {
  if (typeName === HARNESS_CONCERN || typeName in LEGACY_CONCERN_KIND) return "#ef4444"
  const colors: Record<string, string> = {
    harness_plan: "#00f2ff",
    harness_sprint: "#22d3ee",
    harness_todo: "#67e8f9",
    harness_doc: "#f59e0b",
    harness_agent: "#a78bfa",
    harness_stream: "#34d399",
    harness_audit: "#94a3b8",
    harness_repo: "#e879f9",
    harness_poc: "#f472b6",
    decision: "#60a5fa",
  }
  return colors[typeName] ?? "#64748b"
}
