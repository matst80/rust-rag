"use client"

import { useMemo, useState } from "react"
import { ChevronDown, ChevronRight, ShieldAlert, Loader2 } from "lucide-react"
import { cn } from "@/lib/utils"
import type {
  HarnessTreeEdge,
  HarnessTreeNode,
  HarnessTreeResponse,
} from "@/lib/api"

const TYPE_LABELS: Record<string, string> = {
  harness_plan: "PLAN",
  harness_sprint: "SPRINT",
  harness_todo: "TODO",
  harness_doc: "DOC",
  harness_agent: "AGENT",
  harness_stream: "STREAM",
  harness_audit: "AUDIT",
  harness_repo: "REPO",
  harness_poc: "POC",
  // harness_decision was folded into the generic `decision` type.
  decision: "DECISION",
  harness_risk: "RISK",
  harness_compliance: "COMPLIANCE",
  harness_resource: "RESOURCE",
  harness_scaling: "SCALING",
  harness_validation: "VALIDATION",
  harness_rollout: "ROLLOUT",
  // harness_fact renamed harness_evidence (collided with the generic `fact` type).
  harness_evidence: "EVIDENCE",
}

function Badge({ node }: { node: HarnessTreeNode }) {
  if (!node.badge) return null
  if (node.badge === "red") {
    return (
      <span
        title="Invariant collision detected"
        className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-sm bg-red-500/15 text-red-500"
      >
        <ShieldAlert className="h-3 w-3" />
      </span>
    )
  }
  if (node.badge === "yellow") {
    return (
      <span
        title="Missing document anchor or untracked mutation"
        className="inline-flex h-2 w-2 shrink-0 items-center justify-center rounded-full bg-amber-500"
      />
    )
  }
  return (
    <span
      title="Verified against active ADRs"
      className="inline-flex h-2 w-2 shrink-0 items-center justify-center rounded-full bg-emerald-500"
    />
  )
}

function NodeRow({
  node,
  selectedId,
  onSelect,
  depth,
}: {
  node: HarnessTreeNode
  selectedId: string | null
  onSelect: (id: string) => void
  depth: number
}) {
  const selected = node.id === selectedId
  return (
    <button
      type="button"
      onClick={() => onSelect(node.id)}
      className={cn(
        "flex w-full items-center gap-2 py-1 pr-2 text-left text-xs hover:bg-accent/50",
        selected && "bg-accent border-l-2 border-primary",
        !selected && "border-l-2 border-transparent"
      )}
      style={{ paddingLeft: `${depth * 14 + 8}px` }}
    >
      <Badge node={node} />
      <span className="text-[10px] text-muted-foreground tracking-wider">
        {TYPE_LABELS[node.type_name] ?? node.type_name.toUpperCase()}
      </span>
      <span className="truncate text-foreground">{node.title}</span>
      {node.state && (
        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground uppercase">
          {node.state}
        </span>
      )}
    </button>
  )
}

function Section({
  label,
  children,
  defaultOpen = true,
}: {
  label: string
  children: React.ReactNode
  defaultOpen?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-1 px-2 py-1 text-[10px] uppercase tracking-widest text-muted-foreground hover:text-foreground"
      >
        {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        {label}
      </button>
      {open && <div>{children}</div>}
    </div>
  )
}

interface Branch {
  node: HarnessTreeNode
  /** Relation that led to this node (e.g. RAISED, SUPERSEDES, ADDRESSED_BY). */
  relation: string | null
  children: Branch[]
}

function BranchRows({
  branch,
  selectedId,
  onSelect,
  depth,
}: {
  branch: Branch
  selectedId: string | null
  onSelect: (id: string) => void
  depth: number
}) {
  return (
    <div>
      <NodeRow node={branch.node} selectedId={selectedId} onSelect={onSelect} depth={depth} />
      {branch.relation && depth > 0 && (
        <div
          className="text-[9px] uppercase tracking-widest text-muted-foreground/70"
          style={{ paddingLeft: `${depth * 14 + 24}px` }}
        >
          ↳ {branch.relation.toLowerCase()}
        </div>
      )}
      {branch.children.map((child) => (
        <BranchRows
          key={child.node.id}
          branch={child}
          selectedId={selectedId}
          onSelect={onSelect}
          depth={depth + 1}
        />
      ))}
    </div>
  )
}

/**
 * Relations that nest a child underneath its parent in the tree. Matched
 * case-insensitively: agents emit both the canonical SCREAMING_CASE relations
 * and lowercase ones (`contains`, `implemented_by`, `depends_on`, …).
 */
const CHILD_RELATIONS = new Set([
  "BREAKS_INTO",
  // CONTAINS_TODO/GOVERNED_BY/REQUIRES/upper-case SUPERSEDES were folded
  // into the canonical contains/ENFORCES_DOC/depends_on/supersedes
  // predicates — kept here (plus lower-case forms) so old and new edges
  // both render regardless of which call site upper-cases the relation.
  "CONTAINS_TODO",
  "CONTAINS",
  "contains",
  "IMPLEMENTED_BY",
  "GOVERNED_BY",
  "ENFORCES_DOC",
  "DELEGATES_TO",
  "MUTATES_STREAM",
  "HAD_POC",
  "RAISED",
  "ADDRESSED_BY",
  "REQUIRES",
  "DEPENDS_ON",
  "depends_on",
  "SUPERSEDES",
  "supersedes",
])

function buildBranches(
  rootIds: string[],
  byId: Map<string, HarnessTreeNode>,
  childrenByParent: Map<string, HarnessTreeEdge[]>,
  rendered: Set<string>
): Branch[] {
  const build = (nodeId: string, relation: string | null): Branch | null => {
    if (rendered.has(nodeId)) return null
    const node = byId.get(nodeId)
    if (!node) return null
    rendered.add(nodeId)
    const children: Branch[] = []
    for (const edge of childrenByParent.get(nodeId) ?? []) {
      if (!CHILD_RELATIONS.has(edge.relation ?? "")) continue
      const child = build(edge.to_item_id, edge.relation)
      if (child) children.push(child)
    }
    return { node, relation, children }
  }
  const branches: Branch[] = []
  for (const id of rootIds) {
    const branch = build(id, null)
    if (branch) branches.push(branch)
  }
  return branches
}

export function PlanTree({
  tree,
  isLoading,
  error,
  selectedId,
  onSelect,
}: {
  tree: HarnessTreeResponse | null
  isLoading: boolean
  error: unknown
  selectedId: string | null
  onSelect: (id: string) => void
}) {
  const { planBranches, docBranches, pocBranches, evidence, unlinked } = useMemo(() => {
    const byId = new Map<string, HarnessTreeNode>()
    const childrenByParent = new Map<string, HarnessTreeEdge[]>()
    const planIds: string[] = []
    const repoIds: string[] = []
    const docIds: string[] = []
    if (!tree) return { planBranches: [], docBranches: [], pocBranches: [], evidence: [], unlinked: [] }

    for (const node of tree.nodes) byId.set(node.id, node)
    for (const edge of tree.edges) {
      if (!CHILD_RELATIONS.has((edge.relation ?? "").toUpperCase())) continue
      // Agents re-emit the same edge; dedupe on from+to+relation.
      const key = `${edge.from_item_id}|${edge.to_item_id}|${edge.relation ?? ""}`
      if (seenEdges.has(key)) continue
      seenEdges.add(key)
      const list = childrenByParent.get(edge.from_item_id) ?? []
      list.push(edge)
      childrenByParent.set(edge.from_item_id, list)
    }
    for (const node of tree.nodes) {
      if (node.type_name === "harness_plan") planIds.push(node.id)
      if (node.type_name === "harness_repo") repoIds.push(node.id)
      if (node.type_name === "harness_doc") {
        const isChildDoc = tree.edges.some(
          (e) =>
            e.to_item_id === node.id &&
            e.relation?.toLowerCase() === "contains" &&
            byId.get(e.from_item_id)?.type_name === "harness_doc"
        )
        if (!isChildDoc) docIds.push(node.id)
      }
    }
    // Render order matters: plans first, then governing docs, then repos/POCs.
    const rendered = new Set<string>()
    const planBranches = buildBranches(planIds, byId, childrenByParent, rendered)
    const docBranches = buildBranches(docIds, byId, childrenByParent, rendered)
    const pocBranches = buildBranches(repoIds, byId, childrenByParent, rendered)
    const evidence = tree.nodes.filter((n) => n.type_name === "harness_evidence" && !rendered.has(n.id))
    const unlinked = tree.nodes.filter(
      (n) =>
        !rendered.has(n.id) &&
        n.type_name !== "harness_audit" &&
        n.type_name !== "harness_evidence"
    )
    return { planBranches, docBranches, pocBranches, evidence, unlinked }
  }, [tree])

  const empty = planBranches.length === 0 && docBranches.length === 0 && pocBranches.length === 0

  return (
    <div className="flex h-full flex-col">
      <div className="border-b px-3 py-2">
        <h2 className="text-xs font-semibold uppercase tracking-[3px]">
          Plan &amp; Invariants
        </h2>
        <p className="text-[10px] text-muted-foreground">
          {tree?.nodes.length ?? 0} nodes · {tree?.edges.length ?? 0} edges
        </p>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {isLoading && (
          <div className="flex items-center gap-2 px-3 py-4 text-xs text-muted-foreground">
            <Loader2 className="h-3 w-3 animate-spin" /> loading graph…
          </div>
        )}
        {error ? (
          <div className="px-3 py-4 text-xs text-red-500">
            failed to load harness tree
          </div>
        ) : null}
        {!isLoading && !error && empty && (
          <div className="px-3 py-4 text-xs text-muted-foreground">
            No harness plans or POC sessions yet. The harness projects nodes
            via <code className="font-mono text-[10px]">POST /api/store</code>{" "}
            with the <code className="font-mono text-[10px]">harness_*</code>{" "}
            types.
          </div>
        )}
        {planBranches.map((branch) => (
          <BranchRows
            key={branch.node.id}
            branch={branch}
            selectedId={selectedId}
            onSelect={onSelect}
            depth={0}
          />
        ))}
        {docBranches.length > 0 && (
          <Section label={`Governing Documents (${docBranches.length})`}>
            {docBranches.map((branch) => (
              <BranchRows
                key={branch.node.id}
                branch={branch}
                selectedId={selectedId}
                onSelect={onSelect}
                depth={0}
              />
            ))}
          </Section>
        )}
        {pocBranches.length > 0 && (
          <Section label={`POC sessions (${pocBranches.length})`}>
            {pocBranches.map((branch) => (
              <BranchRows
                key={branch.node.id}
                branch={branch}
                selectedId={selectedId}
                onSelect={onSelect}
                depth={1}
              />
            ))}
          </Section>
        )}
        {evidence.length > 0 && (
          <Section label={`Session evidence (${evidence.length})`}>
            {evidence.map((node) => (
              <NodeRow
                key={node.id}
                node={node}
                selectedId={selectedId}
                onSelect={onSelect}
                depth={1}
              />
            ))}
          </Section>
        )}
        {unlinked.length > 0 && (
          <Section label={`Unlinked (${unlinked.length})`} defaultOpen={unlinked.length <= 10}>
            {unlinked.map((node) => (
              <NodeRow
                key={node.id}
                node={node}
                selectedId={selectedId}
                onSelect={onSelect}
                depth={1}
              />
            ))}
          </Section>
        )}
      </div>
    </div>
  )
}
