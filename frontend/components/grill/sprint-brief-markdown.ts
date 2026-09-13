import type { HarnessTreeNode, HarnessTreeResponse } from "@/lib/api"

const SEV_LABEL: Record<string, string> = {
  critical: "CRITICAL",
  high: "HIGH",
  medium: "MEDIUM",
  low: "LOW",
}

export const rel = (edge: { relation: string | null }) => (edge.relation ?? "").toUpperCase()

/** Structural view of a sprint assembled from the harness graph edges. */
export function collectSprintContext(tree: HarnessTreeResponse, sprint: HarnessTreeNode) {
  const byId = new Map(tree.nodes.map((n) => [n.id, n]))
  const seen = new Set<string>()

  // Parent plan: explicit plan_id, or a plan that BREAKS_INTO / IMPLEMENTED_BY this sprint.
  const planId =
    (sprint.data?.plan_id as string | undefined) ??
    tree.edges.find(
      (e) =>
        e.to_item_id === sprint.id &&
        byId.get(e.from_item_id)?.type_name === "harness_plan" &&
        ["BREAKS_INTO", "IMPLEMENTED_BY"].includes(rel(e))
    )?.from_item_id
  const plan = planId ? byId.get(planId) ?? null : null

  const childrenOf = (id: string, relations: string[]) => {
    const out: HarnessTreeNode[] = []
    for (const e of tree.edges) {
      if (e.from_item_id !== id || !relations.includes(rel(e))) continue
      const key = `${e.from_item_id}|${e.to_item_id}|${rel(e)}`
      if (seen.has(key)) continue
      seen.add(key)
      const n = byId.get(e.to_item_id)
      if (n) out.push(n)
    }
    return out
  }

  const todos = childrenOf(sprint.id, ["CONTAINS_TODO", "CONTAINS"])
    .filter((n) => n.type_name === "harness_todo")
    .sort((a, b) => Number(a.data?.sequence_order ?? 0) - Number(b.data?.sequence_order ?? 0))

  const governingDocs: HarnessTreeNode[] = []
  const risks: HarnessTreeNode[] = []
  for (const anchor of [plan?.id, sprint.id].filter(Boolean) as string[]) {
    governingDocs.push(...childrenOf(anchor, ["GOVERNED_BY"]))
    risks.push(...childrenOf(anchor, ["RAISED"]).filter((n) => n.type_name === "harness_risk"))
  }
  for (const todo of todos) governingDocs.push(...childrenOf(todo.id, ["ENFORCES_DOC"]))

  const validations: HarnessTreeNode[] = []
  for (const risk of risks) {
    validations.push(
      ...childrenOf(risk.id, ["ADDRESSED_BY"]).filter((n) => n.type_name === "harness_validation")
    )
  }
  const openValidations = validations.filter((v) => String(v.data?.status ?? v.state ?? "").toLowerCase() !== "done")

  const usedIds = new Set<string>([
    sprint.id,
    ...(plan ? [plan.id] : []),
    ...todos.map((t) => t.id),
    ...governingDocs.map((d) => d.id),
    ...risks.map((r) => r.id),
    ...validations.map((v) => v.id),
  ])

  return {
    plan,
    todos,
    governingDocs: [...new Map(governingDocs.map((d) => [d.id, d])).values()],
    risks: [...new Map(risks.map((r) => [r.id, r])).values()],
    openValidations: [...new Map(openValidations.map((v) => [v.id, v])).values()],
    usedIds,
  }
}

type SprintContext = ReturnType<typeof collectSprintContext>

function filesOf(node: HarnessTreeNode): string[] {
  const files = (node.data?.action_spec as { files?: unknown } | undefined)?.files
  return Array.isArray(files) ? files.map(String) : []
}

export function buildBriefMarkdown(
  tree: HarnessTreeResponse,
  sprint: HarnessTreeNode,
  ctx: SprintContext
): string {
  const lines: string[] = []
  const goal = (sprint.data?.goal as string | undefined) ?? ""
  const cadence = (sprint.data?.cadence as string | undefined) ?? null
  const state = (sprint.data?.state as string | undefined) ?? sprint.state ?? "UNKNOWN"

  lines.push(`# ${sprint.title}`)
  const meta = [state, cadence ? `cadence ${cadence}` : null, ctx.plan ? `plan: ${ctx.plan.title}` : null]
    .filter(Boolean)
    .join(" · ")
  lines.push(meta)
  lines.push("")

  if (goal) {
    lines.push("## Goal", "", goal, "")
  }

  if (ctx.todos.length > 0) {
    const done = ctx.todos.filter((t) => String(t.data?.state ?? t.state ?? "").toUpperCase() === "DONE").length
    lines.push(`## Todos (${done}/${ctx.todos.length} done)`, "")
    for (const todo of ctx.todos) {
      const todoState = String(todo.data?.state ?? todo.state ?? "").toUpperCase()
      const box = todoState === "DONE" ? "x" : " "
      const files = filesOf(todo)
      const verdict = todo.verdict
      const flag = verdict
        ? verdict.violations.some((v) => v.severity === "BLOCKING")
          ? " ⛔"
          : verdict.passed
            ? " ✅"
            : " ⚠️"
        : ""
      const fileNote = files.length ? ` — \`${files.slice(0, 3).join("\` \`")}\`` : ""
      lines.push(`- [${box}] **${todo.title}**${flag}${fileNote}`)
    }
    lines.push("")
  }

  if (ctx.governingDocs.length > 0) {
    lines.push("## Governing documents", "")
    for (const doc of ctx.governingDocs) {
      const kind = (doc.data?.doc_type as string | undefined) ?? "DOC"
      const status = (doc.data?.status as string | undefined) ?? doc.state ?? ""
      lines.push(`- **${doc.title}** (${kind}${status ? ` ${status}` : ""}) — \`${doc.id}\``)
    }
    lines.push("")
  }

  if (ctx.risks.length > 0) {
    lines.push("## Risks raised", "")
    for (const risk of ctx.risks) {
      const severity = String(risk.data?.severity ?? "medium").toLowerCase()
      lines.push(`- **${SEV_LABEL[severity] ?? severity}** — ${risk.title}`)
    }
    lines.push("")
  }

  if (ctx.openValidations.length > 0) {
    lines.push("## Open validations", "")
    for (const validation of ctx.openValidations) {
      lines.push(`- [ ] ${validation.title}`)
    }
    lines.push("")
  }

  return lines.join("\n")
}

/** Footer appended after all evidence sections. */
export function briefFooter(tree: HarnessTreeResponse): string {
  const nodeCount = tree.nodes.filter((n) => n.type_name !== "harness_audit").length
  return `---\n*Generated from ${nodeCount} harness nodes · ${new Date().toISOString().slice(0, 10)}*`
}

export function evidenceMarkdown(
  results: Array<{ id: string; text: string; metadata?: Record<string, unknown> }>,
  tree: HarnessTreeResponse,
  excludeIds: Set<string>
): string {
  const inTree = new Set(tree.nodes.map((n) => n.id))
  const lines: string[] = []
  const evidence = results.filter((r) => !inTree.has(r.id))
  // Related = in-tree hits the structural sections above didn't already use.
  const related = results.filter((r) => inTree.has(r.id) && !excludeIds.has(r.id))

  if (related.length > 0) {
    lines.push("## Related graph nodes", "")
    for (const r of related) {
      lines.push(`- **${r.text.replace(/\s+/g, " ").slice(0, 140)}** — \`${r.id}\``)
    }
    lines.push("")
  }

  if (evidence.length > 0) {
    lines.push("## Session evidence", "")
    lines.push("*Semantic matches from the repo knowledge base:*", "")
    for (const r of evidence.slice(0, 8)) {
      const kind = (r.metadata?.kind as string | undefined) ?? "note"
      const importance = r.metadata?.importance
      const tags = Array.isArray(r.metadata?.tags) ? (r.metadata?.tags as string[]) : []
      const tagNote = tags.length ? ` \`${tags.slice(0, 3).join("` `")}\`` : ""
      lines.push(
        `- **${kind}**${importance ? ` (importance ${String(importance)})` : ""}: ${r.text
          .replace(/\s+/g, " ")
          .slice(0, 220)}${tagNote}`
      )
    }
    lines.push("")
  }

  return lines.join("\n")
}

