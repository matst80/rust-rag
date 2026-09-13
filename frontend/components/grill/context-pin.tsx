"use client"

import { useMemo } from "react"
import useSWR from "swr"
import { Bot, Database, FileText, Loader2, Pin } from "lucide-react"
import { api } from "@/lib/api"
import { useGraphNeighborhood } from "@/lib/api"
import type {
  Entry,
  HarnessTreeEdge,
  HarnessTreeNode,
  HarnessTreeResponse,
} from "@/lib/api"

function jsonBlock(value: unknown): string {
  return "```json\n" + JSON.stringify(value, null, 2) + "\n```"
}

export function ContextPin({
  tree,
  selectedId,
}: {
  tree: HarnessTreeResponse | null
  selectedId: string | null
}) {
  const node: HarnessTreeNode | undefined = useMemo(
    () => tree?.nodes.find((n) => n.id === selectedId),
    [tree, selectedId]
  )
  const { data: neighborhood } = useGraphNeighborhood(selectedId, 2, 100)

  const { todoItem, docs, agent, streams } = useMemo(() => {
    const todoItem = neighborhood?.nodes.find((n) => n.id === selectedId) ?? null
    const nodesById = new Map<string, Entry>()
    for (const n of neighborhood?.nodes ?? []) nodesById.set(n.id, n)

    const docs: Entry[] = []
    const streams: Entry[] = []
    let agent: Entry | null = null
    for (const edge of neighborhood?.edges ?? []) {
      if (edge.edge_type !== "manual") continue
      const relation = edge.relationship?.toUpperCase()
      const from = nodesById.get(edge.source_id)
      const to = nodesById.get(edge.target_id)
      if (!from || !to) continue
      if (relation === "ENFORCES_DOC" || relation === "GOVERNED_BY") {
        const doc = to.type === "harness_doc" ? to : from
        if (!docs.some((d) => d.id === doc.id)) docs.push(doc)
      } else if (relation === "DELEGATES_TO" && to.type === "harness_agent") {
        agent = to
      } else if (relation === "MUTATES_STREAM" && to.type === "harness_stream") {
        streams.push(to)
      }
    }
    return { todoItem, docs, agent, streams }
  }, [neighborhood, selectedId])

  const markdown = useMemo(() => {
    if (!node || !todoItem || node.type_name !== "harness_todo") return ""
    const data = todoItem.data ?? {}
    const lines: string[] = []
    lines.push(`# Context Slice — ${data.title ?? node.title}`)
    lines.push(`> todo: ${node.id} · state: ${node.state ?? "unknown"}`)
    lines.push("")
    lines.push("## Task spec")
    lines.push(todoItem.text)
    if (data.action_spec) {
      lines.push(jsonBlock(data.action_spec))
    }
    if (docs.length > 0) {
      lines.push("## Governing documents")
      for (const doc of docs) {
        lines.push(`### ${doc.id}`)
        lines.push(doc.text)
      }
    }
    if (streams.length > 0) {
      lines.push("## Allowed event streams")
      for (const stream of streams) {
        lines.push(`### ${stream.id} (write)`)
        lines.push(jsonBlock((stream.data ?? {}).schema_definition ?? {}))
      }
    }
    if (agent) {
      lines.push("## Delegation bounds")
      lines.push(jsonBlock({
        role: (agent.data ?? {}).role,
        capabilities: (agent.data ?? {}).capabilities ?? [],
      }))
    }
    return lines.join("\n")
  }, [node, todoItem, docs, agent, streams])

  const { data: tokens, isLoading: counting } = useSWR(
    markdown ? ["token-count", markdown.length, markdown.slice(0, 64)] : null,
    () => api.tokens.count(markdown)
  )

  const capabilities: string[] = Array.isArray(agent?.data?.capabilities)
    ? (agent?.data?.capabilities as string[])
    : []

  return (
    <div className="flex h-full flex-col">
      <div className="border-b px-4 py-2">
        <h2 className="text-xs font-semibold uppercase tracking-[3px]">
          Sub-Agent Pinning
        </h2>
        <p className="font-mono text-[10px] text-muted-foreground">
          {tokens ? `${tokens.token_count} tokens` : counting ? "counting…" : "context slice"}
        </p>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3 text-xs">
        {node?.type_name !== "harness_todo" ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
            <Pin className="h-5 w-5" />
            Select a TODO to inspect its delegation boundaries and the exact
            context slice injected into the sub-agent prompt.
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <div>
              <p className="mb-2 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                <Bot className="h-3 w-3" /> Agent
              </p>
              {agent ? (
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="rounded-sm border border-primary/40 bg-primary/10 px-1.5 py-0.5 font-mono text-[10px] text-primary">
                    {String((agent.data ?? {}).role ?? agent.id)}
                  </span>
                  {capabilities.map((capability) => (
                    <span
                      key={capability}
                      className="rounded-sm border px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                    >
                      {capability}
                    </span>
                  ))}
                </div>
              ) : (
                <p className="text-amber-500">
                  No DELEGATES_TO edge — task is not delegated yet.
                </p>
              )}
            </div>
            <div>
              <p className="mb-2 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                <Database className="h-3 w-3" /> Allowed event streams
              </p>
              {streams.length > 0 ? (
                <ul className="space-y-1">
                  {streams.map((stream) => (
                    <li key={stream.id} className="flex items-center gap-2">
                      <code className="font-mono text-[10px]">{stream.id}</code>
                      <span className="rounded-sm bg-emerald-500/10 px-1 py-0.5 font-mono text-[9px] uppercase text-emerald-500">
                        write
                      </span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-amber-500">
                  No MUTATES_STREAM edge — aggregate mutations untracked.
                </p>
              )}
            </div>
            <div>
              <p className="mb-2 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                <FileText className="h-3 w-3" /> Injected context slice
              </p>
              {markdown ? (
                <pre className="max-h-72 overflow-auto rounded-sm border bg-black/40 p-2 font-mono text-[10px] leading-relaxed whitespace-pre-wrap">
                  {markdown}
                </pre>
              ) : (
                <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
