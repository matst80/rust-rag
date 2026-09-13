"use client"

import { useMemo, useState } from "react"
import useSWR from "swr"
import { Check, Copy, FileText } from "lucide-react"
import { api } from "@/lib/api"
import type { HarnessTreeNode, HarnessTreeResponse } from "@/lib/api"
import { MarkdownView } from "@/components/entries/markdown-view"
import {
  briefFooter,
  buildBriefMarkdown,
  collectSprintContext,
  evidenceMarkdown,
} from "@/components/grill/sprint-brief-markdown"

export function SprintBrief({ tree, sprint }: { tree: HarnessTreeResponse; sprint: HarnessTreeNode }) {
  const [copied, setCopied] = useState(false)

  const ctx = useMemo(() => collectSprintContext(tree, sprint), [tree, sprint])
  const structural = useMemo(() => buildBriefMarkdown(tree, sprint, ctx), [tree, sprint, ctx])

  const goal = (sprint.data?.goal as string | undefined) ?? ""
  const query = [sprint.title, goal].filter(Boolean).join(" ").slice(0, 400)
  const { data, isLoading } = useSWR(
    ["sprint-evidence", sprint.id, sprint.source_id],
    () =>
      api.search({
        query,
        source_id: sprint.source_id || undefined,
        top_k: 12,
        hybrid: true,
      }),
    { revalidateOnFocus: false }
  )

  const markdown = useMemo(() => {
    if (!data) return structural
    const evidence = evidenceMarkdown(data.results, tree, ctx.usedIds)
    if (!evidence) return `${structural}\n${briefFooter(tree)}`
    return `${structural}\n${evidence}\n${briefFooter(tree)}`
  }, [structural, data, tree, ctx])

  const copy = async () => {
    await navigator.clipboard.writeText(markdown)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b px-4 py-2">
        <FileText className="h-3.5 w-3.5 text-primary" />
        <span className="text-[10px] uppercase tracking-widest text-muted-foreground">
          Sprint brief{isLoading ? " · searching evidence…" : ""}
        </span>
        <button
          type="button"
          onClick={copy}
          className="ml-auto flex items-center gap-1 rounded-sm border px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-muted-foreground hover:text-foreground"
        >
          {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
          {copied ? "copied" : "copy md"}
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3">
        <MarkdownView content={markdown} className="text-xs" />
      </div>
    </div>
  )
}
