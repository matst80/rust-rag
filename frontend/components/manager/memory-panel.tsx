"use client"

import { useEffect, useRef, useState } from "react"
import useSWR from "swr"
import { Brain, Trash2, X } from "lucide-react"
import { api } from "@/lib/api"
import { cn } from "@/lib/utils"

interface ManagerMemoryPanelProps {
  open: boolean
  onClose: () => void
}

export function ManagerMemoryPanel({ open, onClose }: ManagerMemoryPanelProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null)
  const [kindFilter, setKindFilter] = useState<string>("")
  const [search, setSearch] = useState<string>("")

  const { data, isLoading, mutate } = useSWR(
    open ? ["manager.memory", kindFilter, search] : null,
    () =>
      api.manager.memory({
        kind: kindFilter || undefined,
        search: search || undefined,
        limit: 100,
      })
  )

  useEffect(() => {
    const dialog = dialogRef.current
    if (!dialog) return
    if (open && !dialog.open) {
      dialog.showModal()
    } else if (!open && dialog.open) {
      dialog.close()
    }
  }, [open])

  // Close on backdrop click (target is the dialog itself when clicking outside the inner panel).
  const handleBackdropClick = (e: React.MouseEvent<HTMLDialogElement>) => {
    if (e.target === e.currentTarget) onClose()
  }

  const handleDelete = async (id: string) => {
    if (!confirm("Delete this memory entry?")) return
    await api.manager.deleteMemory(id)
    void mutate()
  }

  const handleClearAll = async () => {
    const scope = kindFilter ? `kind "${kindFilter}"` : "ALL manager memory"
    if (!confirm(`Clear ${scope}? Cannot be undone.`)) return
    await api.manager.clearMemory(kindFilter || undefined)
    void mutate()
  }

  return (
    <dialog
      ref={dialogRef}
      onClose={onClose}
      onClick={handleBackdropClick}
      className="manager-memory-dialog"
    >
      <div className="manager-memory-panel flex h-full flex-col bg-background text-foreground">
        <header className="flex items-center justify-between gap-2 border-b border-border px-4 py-3">
          <h2 className="flex items-center gap-2 text-sm font-semibold">
            <Brain className="size-4 text-amber-500" />
            Manager Memory
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            aria-label="Close"
          >
            <X className="size-4" />
          </button>
        </header>
        <div className="flex flex-col gap-2 border-b border-border px-4 py-2">
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="search content…"
            className="rounded border border-border bg-background px-2 py-1 text-sm"
          />
          <div className="flex items-center gap-2">
            <select
              value={kindFilter}
              onChange={(e) => setKindFilter(e.target.value)}
              className="flex-1 rounded border border-border bg-background px-2 py-1 text-sm"
            >
              <option value="">all kinds</option>
              <option value="summary">summary</option>
              <option value="note">note</option>
              <option value="task">task</option>
              <option value="observation">observation</option>
            </select>
            <button
              type="button"
              onClick={() => void handleClearAll()}
              className="rounded border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs font-semibold text-destructive hover:bg-destructive/20"
              title={
                kindFilter
                  ? `Clear all "${kindFilter}" entries`
                  : "Clear ALL manager memory"
              }
            >
              Clear {kindFilter || "all"}
            </button>
          </div>
        </div>
        <ul className="flex-1 overflow-y-auto divide-y divide-border">
          {isLoading ? (
            <li className="px-4 py-3 text-sm text-muted-foreground">loading…</li>
          ) : data && data.length > 0 ? (
            data.map((m) => (
              <li key={m.id} className="group/row relative px-4 py-3">
                <div className="flex items-start justify-between gap-2">
                  <span
                    className={cn(
                      "rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase",
                      m.kind === "task"
                        ? "bg-blue-500/20 text-blue-600 dark:text-blue-400"
                        : m.kind === "summary"
                          ? "bg-purple-500/20 text-purple-600 dark:text-purple-400"
                          : m.kind === "observation"
                            ? "bg-emerald-500/20 text-emerald-600 dark:text-emerald-400"
                            : "bg-muted text-muted-foreground"
                    )}
                  >
                    {m.kind}
                  </span>
                  <button
                    type="button"
                    onClick={() => void handleDelete(m.id)}
                    className="opacity-0 transition-opacity group-hover/row:opacity-100 text-muted-foreground hover:text-destructive"
                    aria-label="Delete"
                  >
                    <Trash2 className="size-3.5" />
                  </button>
                </div>
                <p className="mt-1 whitespace-pre-wrap break-words text-sm">
                  {m.content}
                </p>
                <p className="mt-1 text-[10px] text-muted-foreground">
                  {new Date(m.created_at).toLocaleString()}
                </p>
              </li>
            ))
          ) : (
            <li className="px-4 py-3 text-sm text-muted-foreground">
              no memory entries
            </li>
          )}
        </ul>
      </div>
    </dialog>
  )
}
