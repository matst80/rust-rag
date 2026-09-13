"use client"

import { useMemo } from "react"
import type { HarnessTreeEdge, HarnessTreeMemory, HarnessTreeNode, HarnessTreeResponse } from "@/lib/api"

/** Reverse-edge lookup: which node + relation points at this node. */
export interface ParentLink {
  from: string
  relation: string
}

export interface HarnessIndex {
  byId: Map<string, HarnessTreeNode>
  childrenOf: Map<string, HarnessTreeEdge[]>
  parentsOf: Map<string, ParentLink[]>
  repos: HarnessTreeNode[]
  pocs: HarnessTreeNode[]
  /** POC id → session memories joined in by the tree endpoint. */
  memoriesByPoc: Map<string, HarnessTreeMemory[]>
}

/** Walk RAISED / HAD_POC parent edges upward; returns nearest-first chain. */
export function ancestorChain(
  index: HarnessIndex,
  nodeId: string
): Array<{ node: HarnessTreeNode; relation: string | null }> {
  const chain: Array<{ node: HarnessTreeNode; relation: string | null }> = []
  const seen = new Set<string>([nodeId])
  let current = nodeId
  for (;;) {
    const parents = index.parentsOf.get(current) ?? []
    const next = parents.find(
      (p) => p.relation === "RAISED" || p.relation === "HAD_POC" || p.relation === "BREAKS_INTO" || p.relation === "CONTAINS_TODO"
    )
    if (!next || seen.has(next.from)) break
    const node = index.byId.get(next.from)
    if (!node) break
    seen.add(next.from)
    chain.push({ node, relation: next.relation })
    current = next.from
  }
  return chain
}

/** Repo node a node ultimately hangs off, if any. */
export function repoOf(index: HarnessIndex, nodeId: string): HarnessTreeNode | null {
  for (const { node } of ancestorChain(index, nodeId)) {
    if (node.type_name === "harness_repo") return node
  }
  return null
}

/**
 * POC nodes a sprint-domain node inherits evidence from: explicit
 * DERIVES_FROM / EVIDENCED_BY edges upward, so a sprint or todo resolves to
 * the sessions that motivated it.
 */
export function evidencePocsOf(index: HarnessIndex, nodeId: string): HarnessTreeNode[] {
  const pocs: HarnessTreeNode[] = []
  const seen = new Set<string>([nodeId])
  let current = nodeId
  for (;;) {
    const next = (index.parentsOf.get(current) ?? []).find(
      (p) => p.relation === "DERIVES_FROM" || p.relation === "EVIDENCED_BY"
    )
    if (!next || seen.has(next.from)) break
    const node = index.byId.get(next.from)
    if (!node) break
    seen.add(next.from)
    if (node.type_name === "harness_poc") pocs.push(node)
    current = next.from
  }
  return pocs
}

/** Session memories for a node: its own POC evidence, or [] when it has none. */
export function memoriesFor(
  index: HarnessIndex,
  nodeId: string
): Array<HarnessTreeMemory & { node: HarnessTreeNode }> {
  const out: Array<HarnessTreeMemory & { node: HarnessTreeNode }> = []
  for (const poc of evidencePocsOf(index, nodeId)) {
    for (const memory of index.memoriesByPoc.get(poc.id) ?? []) {
      out.push({ ...memory, node: poc })
    }
  }
  return out
}

export function useHarnessIndex(tree: HarnessTreeResponse | null): HarnessIndex {
  return useMemo(() => {
    const byId = new Map<string, HarnessTreeNode>()
    const childrenOf = new Map<string, HarnessTreeEdge[]>()
    const parentsOf = new Map<string, ParentLink[]>()
    const repos: HarnessTreeNode[] = []
    const pocs: HarnessTreeNode[] = []
    const memoriesByPoc = new Map<string, HarnessTreeMemory[]>()
    if (!tree) return { byId, childrenOf, parentsOf, repos, pocs, memoriesByPoc }

    for (const node of tree.nodes) {
      byId.set(node.id, node)
      if (node.type_name === "harness_repo") repos.push(node)
      if (node.type_name === "harness_poc") pocs.push(node)
    }
    for (const memory of tree.memories ?? []) {
      const list = memoriesByPoc.get(memory.poc_id) ?? []
      list.push(memory)
      memoriesByPoc.set(memory.poc_id, list)
    }
    for (const edge of tree.edges) {
      const out = childrenOf.get(edge.from_item_id) ?? []
      out.push(edge)
      childrenOf.set(edge.from_item_id, out)
      const inc = parentsOf.get(edge.to_item_id) ?? []
      inc.push({ from: edge.from_item_id, relation: edge.relation ?? "" })
      parentsOf.set(edge.to_item_id, inc)
    }
    return { byId, childrenOf, parentsOf, repos, pocs, memoriesByPoc }
  }, [tree])
}

export const SEVERITY_RANK: Record<string, number> = {
  critical: 0,
  high: 1,
  medium: 2,
  low: 3,
}

export const SEVERITY_COLOR: Record<string, string> = {
  critical: "text-red-500 border-red-500/40 bg-red-500/10",
  high: "text-red-400 border-red-400/30 bg-red-400/5",
  medium: "text-amber-500 border-amber-500/40 bg-amber-500/10",
  low: "text-sky-400 border-sky-400/30 bg-sky-400/5",
}
