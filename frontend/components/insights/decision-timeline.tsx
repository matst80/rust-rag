"use client"

import { useMemo } from "react"
import { Gavel } from "lucide-react"
import { cn } from "@/lib/utils"
import type { HarnessTreeNode } from "@/lib/api"
import type { HarnessIndex } from "@/components/insights/use-harness-index"

interface TimelineEntry {
  poc: HarnessTreeNode
  decisions: Array<{
    node: HarnessTreeNode
    supersededBy: HarnessTreeNode | null
  }>
}

function pocTimestamp(poc: HarnessTreeNode): number {
  const ts = poc.data?.timestamp
  return typeof ts === "number" ? ts : poc.updated_at
}

export function DecisionTimeline({
  index,
  repoId,
  onSelect,
}: {
  index: HarnessIndex
  repoId: string | null
  onSelect: (id: string) => void
}) {
  const { entries, supersededBy } = useMemo(() => {
    // decision id → the decision that replaced it (SUPERSEDES edges point
    // newer → older, so the *source* is the replacement).
    const supersededBy = new Map<string, HarnessTreeNode>()
    for (const node of index.byId.values()) {
      if (node.type_name !== "harness_decision") continue
      for (const edge of index.childrenOf.get(node.id) ?? []) {
        if (edge.relation === "SUPERSEDES") {
          const old = index.byId.get(edge.to_item_id)
          if (old) supersededBy.set(old.id, node)
        }
      }
    }

    const pocs = index.pocs
      .filter((poc) => !repoId || repoOfPoc(index, poc.id) === repoId)
      .sort((a, b) => pocTimestamp(a) - pocTimestamp(b))

    const entries: TimelineEntry[] = pocs.map((poc) => {
      const decisions = (index.childrenOf.get(poc.id) ?? [])
        .filter(
          (e) =>
            e.relation === "RAISED" &&
            index.byId.get(e.to_item_id)?.type_name === "harness_decision"
        )
        .map((e) => index.byId.get(e.to_item_id)!)
        .filter(Boolean)
        .map((node) => ({ node, supersededBy: supersededBy.get(node.id) ?? null }))
      return { poc, decisions }
    })
    return { entries, supersededBy }
  }, [index, repoId])

  const totalDecisions = entries.reduce((sum, e) => sum + e.decisions.length, 0)
  const supersededCount = [...supersededBy.keys()].length

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-4 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
        <span>{entries.length} POC sessions</span>
        <span>{totalDecisions} decisions</span>
        <span className="text-red-400">{supersededCount} superseded</span>
      </div>

      {entries.length === 0 ? (
        <p className="py-8 text-center font-mono text-xs text-muted-foreground">
          No POC sessions {repoId ? "for this repo" : "in the graph yet"}.
        </p>
      ) : (
        <ol className="relative ml-3 border-l border-border pl-6">
          {entries.map(({ poc, decisions }) => (
            <li key={poc.id} className="mb-6">
              <span className="absolute -left-[7px] mt-1.5 flex h-3.5 w-3.5 items-center justify-center rounded-full border border-primary/60 bg-primary/20" />
              <div className="mb-2">
                <button
                  type="button"
                  onClick={() => onSelect(poc.id)}
                  className="font-mono text-xs text-primary underline-offset-2 hover:underline"
                >
                  {poc.title}
                </button>
                <span className="ml-2 font-mono text-[10px] text-muted-foreground">
                  {new Date(pocTimestamp(poc)).toLocaleDateString()}
                </span>
              </div>
              {decisions.length === 0 ? (
                <p className="font-mono text-[10px] text-muted-foreground">no decisions raised</p>
              ) : (
                <div className="flex flex-col gap-2">
                  {decisions.map(({ node, supersededBy: replacedBy }) => (
                    <button
                      key={node.id}
                      type="button"
                      onClick={() => onSelect(node.id)}
                      className={cn(
                        "rounded-sm border px-3 py-2 text-left text-xs hover:bg-accent/40",
                        replacedBy
                          ? "border-border/60 bg-transparent text-muted-foreground"
                          : "border-primary/30 bg-primary/5"
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <Gavel className={cn("h-3 w-3", replacedBy ? "text-muted-foreground" : "text-primary")} />
                        <span className={cn("font-mono", replacedBy && "line-through")}>
                          {node.title}
                        </span>
                        {replacedBy && (
                          <span className="ml-auto font-mono text-[10px] text-red-400">
                            superseded by {replacedBy.title}
                          </span>
                        )}
                      </div>
                      {typeof node.data?.status === "string" && (
                        <span className="mt-1 inline-block rounded-sm border px-1 py-0.5 font-mono text-[9px] uppercase text-muted-foreground">
                          {node.data.status}
                        </span>
                      )}
                    </button>
                  ))}
                </div>
              )}
            </li>
          ))}
        </ol>
      )}
    </div>
  )
}

function repoOfPoc(index: HarnessIndex, pocId: string): string | null {
  const link = (index.parentsOf.get(pocId) ?? []).find((p) => p.relation === "HAD_POC")
  return link?.from ?? null
}
