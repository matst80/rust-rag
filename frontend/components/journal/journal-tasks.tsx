"use client"

import { useState } from "react"
import Link from "next/link"
import { CheckCircle2, Circle, ListTodo } from "lucide-react"
import { cn } from "@/lib/utils"
import type { JournalTask } from "@/lib/journal"

interface JournalTasksProps {
  openTasks: JournalTask[]
  doneTasks: JournalTask[]
}

export function JournalTasks({ openTasks, doneTasks }: JournalTasksProps) {
  const [showDone, setShowDone] = useState(false)
  const visibleDone = showDone ? doneTasks.slice(0, 20) : doneTasks.slice(0, 5)

  if (openTasks.length === 0 && doneTasks.length === 0) {
    return (
      <div className="rounded-2xl border border-dashed border-border p-6 text-center">
        <ListTodo className="mx-auto mb-2 size-5 text-muted-foreground/40" />
        <p className="text-sm text-muted-foreground">No checklist tasks in daily notes yet</p>
        <p className="mt-1 text-xs text-muted-foreground/70">
          Add <code className="rounded bg-muted px-1 font-mono">- [ ] buy milk</code> to
          any daily note and it will aggregate here.
        </p>
      </div>
    )
  }

  return (
    <div className="rounded-2xl border border-border bg-card p-4">
      <div className="mb-3 flex items-baseline justify-between">
        <h2 className="font-mono text-xs font-black uppercase tracking-[2px]">
          Tasks · {openTasks.length} open
        </h2>
        {doneTasks.length > 0 && (
          <button
            type="button"
            onClick={() => setShowDone((v) => !v)}
            className="font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground transition-colors hover:text-primary"
          >
            {showDone ? "Hide done" : `Show done (${doneTasks.length})`}
          </button>
        )}
      </div>

      {openTasks.length > 0 ? (
        <ul className="flex flex-col gap-1.5">
          {openTasks.slice(0, 30).map((t) => (
            <li key={t.id}>
              <Link
                href={`/entries/${encodeURIComponent(t.entryId)}`}
                prefetch={false}
                className="group flex items-start gap-2.5 rounded-lg border border-border/70 px-3 py-2 transition-colors hover:border-amber-500/40 hover:bg-amber-500/5"
              >
                <Circle className="mt-0.5 size-4 shrink-0 text-amber-500/70 transition-colors group-hover:text-amber-500" />
                <span className="min-w-0 flex-1 text-sm leading-snug">{t.text}</span>
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">
                  {t.entryTitle}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      ) : (
        <p className="rounded-lg bg-emerald-500/10 px-3 py-2 text-sm text-emerald-600 dark:text-emerald-400">
          All caught up — no open checklist tasks.
        </p>
      )}

      {doneTasks.length > 0 && (
        <div className={cn("mt-3", !showDone && openTasks.length > 0 && "opacity-80")}>
          <p className="mb-1.5 font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground/70">
            Recently done
          </p>
          <ul className="flex flex-col gap-1">
            {visibleDone.map((t) => (
              <li key={t.id}>
                <Link
                  href={`/entries/${encodeURIComponent(t.entryId)}`}
                  prefetch={false}
                  className="flex items-start gap-2.5 rounded-lg px-3 py-1.5 text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                >
                  <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-emerald-500/60" />
                  <span className="min-w-0 flex-1 text-[13px] leading-snug line-through decoration-muted-foreground/40">
                    {t.text}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}
