"use client"

import { useMemo, useState } from "react"
import useSWR from "swr"
import { ExternalLink, Loader2, Search } from "lucide-react"
import { api, useItem } from "@/lib/api"
import type { HarnessTreeResponse } from "@/lib/api"
import {
  SEVERITY_COLOR,
  ancestorChain,
  type HarnessIndex,
} from "@/components/insights/use-harness-index"
import { cn } from "@/lib/utils"

const EXPLORE_TYPES = [
  { id: "harness_compliance", label: "Compliance" },
  { id: "harness_risk", label: "Risks" },
  { id: "decision", label: "Decisions" },
  { id: "harness_scaling", label: "Scaling" },
  { id: "harness_validation", label: "Validations" },
  { id: "harness_poc", label: "POC sessions" },
] as const

function prettyData(data: Record<string, unknown> | null | undefined): string {
  if (!data || Object.keys(data).length === 0) return ""
  return JSON.stringify(data, null, 2)
}

export function GraphExplore({
  index,
  repoId,
  onSelect,
}: {
  index: HarnessIndex
  repoId: string | null
  onSelect: (id: string) => void
}) {
  const [query, setQuery] = useState("")
  const [submitted, setSubmitted] = useState<{ query: string; repoId: string | null } | null>(null)
  const [types, setTypes] = useState<Set<string>>(
    new Set(["harness_compliance", "harness_risk", "decision"])
  )
  const [selectedHit, setSelectedHit] = useState<string | null>(null)
  const { data: full } = useItem(selectedHit)

  const { data, isLoading } = useSWR(
    submitted ? ["explore", submitted.query, submitted.repoId, [...types].sort()] : null,
    async ([, q, rid, typeList]) =>
      api.search({
        query: q as string,
        top_k: 12,
        type_names: typeList as string[],
      })
  )

  const hits = useMemo(() => {
    const results = data?.results ?? []
    return results
      .map((hit) => {
        const chain = ancestorChain(index, hit.id)
        const hitRepo = chain.find((c) => c.node.type_name === "harness_repo")?.node ?? null
        if (submitted?.repoId && hitRepo?.id !== submitted.repoId) return null
        const poc = chain.find((c) => c.node.type_name === "harness_poc")?.node ?? null
        return { hit, repo: hitRepo, poc }
      })
      .filter((x): x is NonNullable<typeof x> => x !== null)
  }, [data, index, submitted])

  const toggleType = (id: string) => {
    setTypes((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <div className="flex flex-col gap-4">
      <form
        onSubmit={(e) => {
          e.preventDefault()
          if (query.trim()) setSubmitted({ query: query.trim(), repoId })
        }}
        className="flex flex-col gap-2"
      >
        <div className="flex items-center gap-2 rounded-sm border border-primary/30 bg-black/40 px-3 py-2 focus-within:border-primary/60">
          <Search className="h-4 w-4 text-muted-foreground" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder='e.g. "what compliance concerns have come up near payments code"'
            className="flex-1 bg-transparent font-mono text-xs outline-none placeholder:text-muted-foreground/60"
          />
          <button
            type="submit"
            className="rounded-sm border border-primary/40 bg-primary/10 px-2 py-1 font-mono text-[10px] uppercase tracking-wider text-primary hover:bg-primary/20"
          >
            Search graph
          </button>
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          {EXPLORE_TYPES.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => toggleType(t.id)}
              className={cn(
                "rounded-sm border px-1.5 py-0.5 font-mono text-[10px] uppercase",
                types.has(t.id)
                  ? "border-primary/50 bg-primary/10 text-primary"
                  : "border-border text-muted-foreground hover:text-foreground"
              )}
            >
              {t.label}
            </button>
          ))}
        </div>
      </form>

      {!submitted && (
        <p className="py-8 text-center font-mono text-xs text-muted-foreground">
          Semantic search across promoted memories — scoped to the node types
          you pick{repoId ? " and the selected repo" : ""}.
        </p>
      )}

      {submitted && isLoading && (
        <div className="flex items-center gap-2 py-6 text-xs text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin" /> embedding query, searching…
        </div>
      )}

      {submitted && !isLoading && hits.length === 0 && (
        <p className="py-8 text-center font-mono text-xs text-muted-foreground">
          No matches. Try broader wording or enable more node types.
        </p>
      )}

      {submitted && !isLoading && hits.length > 0 && (
        <div className="grid gap-2 lg:grid-cols-[1fr_20rem]">
          <div className="flex flex-col gap-2">
            {hits.map(({ hit, repo, poc }) => {
              const meta = index.byId.get(hit.id)
              const severity = meta?.data?.severity
              const framework = meta?.data?.framework
              return (
                <button
                  key={hit.id}
                  type="button"
                  onClick={() => {
                    setSelectedHit(hit.id)
                    onSelect(hit.id)
                  }}
                  className={cn(
                    "rounded-sm border px-3 py-2 text-left text-xs hover:bg-accent/40",
                    selectedHit === hit.id ? "border-primary/60" : "border-border"
                  )}
                >
                  <div className="flex items-center gap-2">
                    <span className="rounded-sm border px-1.5 py-0.5 font-mono text-[9px] uppercase text-muted-foreground">
                      {(hit.type_name ?? "entry").replace("harness_", "")}
                    </span>
                    {typeof severity === "string" && (
                      <span
                        className={cn(
                          "rounded-sm border px-1.5 py-0.5 font-mono text-[9px] uppercase",
                          SEVERITY_COLOR[severity] ?? "text-muted-foreground"
                        )}
                      >
                        {severity}
                      </span>
                    )}
                    {typeof framework === "string" && (
                      <span className="rounded-sm border px-1.5 py-0.5 font-mono text-[9px] uppercase text-amber-500">
                        {framework}
                      </span>
                    )}
                    <span className="ml-auto font-mono text-[10px] text-muted-foreground">
                      {Math.round(hit.score * 100)}%
                    </span>
                  </div>
                  <p className="mt-1 line-clamp-2">{hit.text.split("\n")[0]}</p>
                  <p className="mt-1 font-mono text-[10px] text-muted-foreground">
                    {[repo?.title, poc?.title].filter(Boolean).join(" › ") || hit.source_id}
                  </p>
                </button>
              )
            })}
          </div>
          {selectedHit && (
            <div className="h-fit rounded-sm border p-3 text-xs">
              <div className="mb-2 flex items-center justify-between">
                <p className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                  Node detail
                </p>
                <a
                  href={`/entries/${selectedHit}`}
                  className="inline-flex items-center gap-1 font-mono text-[10px] text-muted-foreground underline hover:text-foreground"
                >
                  open <ExternalLink className="h-3 w-3" />
                </a>
              </div>
              <pre className="max-h-80 overflow-auto whitespace-pre-wrap font-mono text-[10px] leading-relaxed">
                {full?.text}
                {prettyData(full?.data) && `\n\n${prettyData(full?.data)}`}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
