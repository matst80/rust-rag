"use client"

import { useMemo } from "react"
import { cn } from "@/lib/utils"
import type { HarnessTreeResponse } from "@/lib/api"

function formatTime(epochMs: number): string {
  return new Date(epochMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  })
}

export function TimelineBar({
  tree,
  selectedId,
  onSelect,
}: {
  tree: HarnessTreeResponse | null
  selectedId: string | null
  onSelect: (id: string) => void
}) {
  const audits = useMemo(() => {
    return (tree?.nodes ?? [])
      .filter((n) => n.type_name === "harness_audit")
      .slice()
      .sort((a, b) => a.updated_at - b.updated_at)
  }, [tree])

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b px-4 py-1.5">
        <h2 className="text-[10px] font-semibold uppercase tracking-[3px]">
          Grill Timeline
        </h2>
        <span className="font-mono text-[10px] text-muted-foreground">
          {audits.length} audits · oldest → latest
        </span>
      </div>
      <div className="flex-1 overflow-x-auto overflow-y-hidden px-4 py-2">
        {audits.length === 0 ? (
          <p className="py-2 font-mono text-[10px] text-muted-foreground">
            No audit verdicts posted yet — the harness writes them as
            harness_audit nodes linked with AUDITED edges.
          </p>
        ) : (
          <ol className="flex h-full items-stretch gap-0">
            {audits.map((audit, index) => {
              const selected = audit.id === selectedId
              const blocking = audit.verdict?.violations.some(
                (v) => v.severity === "BLOCKING"
              )
              return (
                <li key={audit.id} className="flex items-stretch">
                  {index > 0 && (
                    <span
                      aria-hidden
                      className="mx-1 self-center h-px w-4 bg-muted-foreground/40"
                    />
                  )}
                  <button
                    type="button"
                    onClick={() => onSelect(audit.id)}
                    className={cn(
                      "flex min-w-36 flex-col justify-center gap-0.5 rounded-sm border px-2 py-1 text-left hover:bg-accent/50",
                      selected ? "border-primary" : "border-border"
                    )}
                  >
                    <span className="flex items-center gap-1.5">
                      <span
                        className={cn(
                          "inline-block h-2 w-2 rounded-full",
                          blocking
                            ? "bg-red-500"
                            : audit.verdict?.passed
                              ? "bg-emerald-500"
                              : "bg-amber-500"
                        )}
                      />
                      <span className="font-mono text-[10px] text-muted-foreground">
                        {formatTime(audit.updated_at)}
                      </span>
                    </span>
                    <span className="truncate font-mono text-[10px]">
                      {audit.verdict
                        ? `score ${(audit.verdict.score * 100).toFixed(0)}%`
                        : audit.title}
                    </span>
                  </button>
                </li>
              )
            })}
          </ol>
        )}
      </div>
    </div>
  )
}
