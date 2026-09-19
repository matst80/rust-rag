import type { PathRow } from "@/lib/api"
import type { TreeNodeData } from "@/components/wiki/wiki-tree-node"

export interface SourceTree {
  sourceId: string
  totalCount: number
  roots: TreeNodeData[]
}

/** Build per-source nested trees from a flat list of (source, path, count). */
export function buildSourceTrees(rows: PathRow[]): SourceTree[] {
  const bySource = new Map<string, PathRow[]>()
  for (const r of rows) {
    if (!bySource.has(r.source_id)) bySource.set(r.source_id, [])
    bySource.get(r.source_id)!.push(r)
  }
  const out: SourceTree[] = []
  for (const [sourceId, sourceRows] of bySource) {
    const byPath = new Map<string, TreeNodeData>()
    const ensure = (path: string): TreeNodeData => {
      const existing = byPath.get(path)
      if (existing) return existing
      const segment = path.includes("/") ? path.slice(path.lastIndexOf("/") + 1) : path
      const node: TreeNodeData = {
        segment,
        path,
        count: 0,
        subtreeCount: 0,
        children: [],
      }
      byPath.set(path, node)
      return node
    }
    for (const r of sourceRows) {
      const segs = r.path.split("/")
      for (let i = 1; i <= segs.length; i++) {
        ensure(segs.slice(0, i).join("/"))
      }
      ensure(r.path).count = r.count
    }
    for (const node of byPath.values()) {
      const idx = node.path.lastIndexOf("/")
      if (idx === -1) continue
      const parentPath = node.path.slice(0, idx)
      const parent = byPath.get(parentPath)
      if (parent) parent.children.push(node)
    }
    for (const node of byPath.values()) {
      node.children.sort((a, b) => a.segment.localeCompare(b.segment))
    }
    const roots = Array.from(byPath.values()).filter((n) => !n.path.includes("/"))
    roots.sort((a, b) => a.segment.localeCompare(b.segment))
    const fillSubtree = (n: TreeNodeData): number => {
      n.subtreeCount = n.count + n.children.reduce((s, c) => s + fillSubtree(c), 0)
      return n.subtreeCount
    }
    let total = 0
    for (const r of roots) total += fillSubtree(r)
    out.push({ sourceId, totalCount: total, roots })
  }
  out.sort((a, b) => a.sourceId.localeCompare(b.sourceId))
  return out
}

/** Active path → set of every ancestor path along the chain (inclusive). */
export function ancestorChain(prefix?: string): Set<string> {
  const set = new Set<string>()
  if (!prefix) return set
  const segs = prefix.split("/")
  for (let i = 1; i <= segs.length; i++) {
    set.add(segs.slice(0, i).join("/"))
  }
  return set
}

export function wikiHref(sourceId: string, path?: string, entryId?: string) {
  const params = new URLSearchParams({ source_id: sourceId })
  if (path) params.set("path", path)
  if (entryId) params.set("entry", entryId)
  return `/wiki?${params.toString()}`
}
