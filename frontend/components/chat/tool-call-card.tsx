"use client"

import { useEffect, useRef, useState, type ReactNode } from "react"
import {
  Search,
  FileText,
  ListTree,
  Tags,
  Library,
  Save,
  PenLine,
  Trash2,
  Split,
  Share2,
  Waypoints,
  RefreshCw,
  Link2,
  Unlink,
  Globe,
  FileCode,
  Wrench,
  CheckCircle2,
  AlertTriangle,
  Loader2,
  ChevronRight,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { ToolResultView } from "./tool-result-view"

export type ToolCallStatus = "running" | "done" | "error"

interface ToolCallCardProps {
  name: string
  /** Raw JSON string (hosted) or parsed object (local). */
  args: string | Record<string, unknown> | undefined
  result?: string
  error?: string
  status: ToolCallStatus
}

const TOOL_META: Record<string, { icon: typeof Search; label: string }> = {
  // on-device (WebGPU) tools
  search: { icon: Search, label: "Semantic search" },
  search_entries: { icon: Search, label: "Semantic search" },
  get_entry: { icon: FileText, label: "Read entry" },
  get_item: { icon: FileText, label: "Read entry" },
  list_paths: { icon: ListTree, label: "Browse paths" },
  // backend repository tools
  list_categories: { icon: Tags, label: "List categories" },
  list_items: { icon: Library, label: "List items" },
  store_entry: { icon: Save, label: "Store entry" },
  update_item: { icon: PenLine, label: "Update entry" },
  delete_item: { icon: Trash2, label: "Delete entry" },
  split_entry_to_wiki: { icon: Split, label: "Split to wiki" },
  // graph tools
  graph_status: { icon: Waypoints, label: "Graph status" },
  graph_neighborhood: { icon: Share2, label: "Graph neighborhood" },
  rebuild_graph: { icon: RefreshCw, label: "Rebuild graph" },
  create_graph_edge: { icon: Link2, label: "Create edge" },
  delete_graph_edge: { icon: Unlink, label: "Delete edge" },
  // ingestion tools
  ingest_web_content: { icon: Globe, label: "Ingest web page" },
  read_file_range: { icon: FileCode, label: "Read file" },
}

function toolMeta(name: string) {
  return TOOL_META[name] ?? { icon: Wrench, label: name.replace(/_/g, " ") }
}

function parseArgs(args: ToolCallCardProps["args"]): { data?: unknown; text: string } {
  if (args === undefined || args === null) return { text: "" }
  if (typeof args === "object") {
    const text = JSON.stringify(args, null, 2)
    return { data: args, text }
  }
  try {
    const data = JSON.parse(args)
    return { data, text: JSON.stringify(data, null, 2) }
  } catch {
    // Arguments may still be streaming in as partial JSON.
    return { text: String(args) }
  }
}

function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** Short human summary for the result section header. */
function summarizeResult(result: string | undefined): string {
  if (!result) return ""
  try {
    const data = JSON.parse(result)
    const hits = Array.isArray(data) ? data : data?.results
    if (Array.isArray(hits)) return `${hits.length} hit${hits.length === 1 ? "" : "s"}`
    if (Array.isArray(data?.items)) {
      const total = typeof data.total_count === "number" ? data.total_count : data.items.length
      return `${data.items.length} of ${total} items`
    }
    if (Array.isArray(data?.categories)) {
      return `${data.categories.length} categor${data.categories.length === 1 ? "y" : "ies"}`
    }
    if (typeof data?.deleted === "boolean") return data.deleted ? "deleted" : "not found"
    if (typeof data?.id === "string" && data.id) return `stored ${data.id.slice(0, 14)}…`
    if (Array.isArray(data?.chunk_ids)) return `${data.chunk_ids.length} chunks`
    return humanSize(result.length)
  } catch {
    return humanSize(result.length)
  }
}

const JSON_TOKEN =
  /("(?:\\.|[^"\\])*")(\s*:)|("(?:\\.|[^"\\])*")|\b(true|false|null)\b|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g

/** Tiny JSON colorizer — keys/strings/numbers/keywords on the muted base. */
function JsonView({ source }: { source: string }) {
  const nodes: ReactNode[] = []
  let last = 0
  let key = 0
  for (const m of source.matchAll(JSON_TOKEN)) {
    const start = m.index ?? 0
    if (start > last) {
      nodes.push(
        <span key={key++} className="text-muted-foreground/70">
          {source.slice(last, start)}
        </span>
      )
    }
    if (m[1] !== undefined) {
      nodes.push(
        <span key={key++}>
          <span className="text-primary">{m[1]}</span>
          <span className="text-muted-foreground/70">{m[2]}</span>
        </span>
      )
    } else if (m[3] !== undefined) {
      nodes.push(
        <span key={key++} className="text-chart-2">
          {m[3]}
        </span>
      )
    } else if (m[4] !== undefined) {
      nodes.push(
        <span key={key++} className="text-chart-4">
          {m[4]}
        </span>
      )
    } else if (m[5] !== undefined) {
      nodes.push(
        <span key={key++} className="text-chart-3">
          {m[5]}
        </span>
      )
    }
    last = start + m[0].length
  }
  if (last < source.length) {
    nodes.push(
      <span key={key++} className="text-muted-foreground/70">
        {source.slice(last)}
      </span>
    )
  }
  return (
    <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all p-3 font-mono text-[10px] leading-relaxed">
      {nodes}
    </pre>
  )
}

function Section({
  open,
  onToggle,
  label,
  summary,
  children,
}: {
  open: boolean
  onToggle: () => void
  label: string
  summary?: string
  children: ReactNode
}) {
  return (
    <div>
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-primary/5"
      >
        <ChevronRight
          className={cn(
            "size-3 shrink-0 text-muted-foreground transition-transform duration-200",
            open && "rotate-90"
          )}
        />
        <span className="font-mono text-[9px] font-bold uppercase tracking-[1.5px] text-muted-foreground">
          {label}
        </span>
        {summary && (
          <span className="ml-auto max-w-[60%] truncate font-mono text-[9px] tabular-nums text-primary/60" title={summary}>
            {summary}
          </span>
        )}
      </button>
      {open && <div className="border-t border-primary/10 bg-background/40">{children}</div>}
    </div>
  )
}

export function ToolCallCard({ name, args, result, error, status }: ToolCallCardProps) {
  const { data, text } = parseArgs(args)
  const hasArgs =
    text.trim().length > 0 &&
    text.trim() !== "{}" &&
    text.trim() !== "null"
  const [argsOpen, setArgsOpen] = useState(false)
  const [resultOpen, setResultOpen] = useState(false)
  const prevStatus = useRef(status)

  // Pop the result section open once, on the running → done transition.
  useEffect(() => {
    if (prevStatus.current === "running" && status === "done") setResultOpen(true)
    prevStatus.current = status
  }, [status])

  const meta = toolMeta(name)
  const Icon = meta.icon
  const running = status === "running"

  return (
    <div
      className={cn(
        "w-full overflow-hidden border text-xs shadow-sm transition-colors duration-300",
        status === "error"
          ? "border-destructive/30 bg-destructive/5"
          : "border-primary/25 bg-primary/[0.04] hover:border-primary/45"
      )}
    >
      {/* Header */}
      <div className="flex items-center gap-2.5 border-b border-primary/10 bg-primary/10 px-3 py-2">
        <div className="flex size-6 shrink-0 items-center justify-center border border-primary/30 bg-background">
          <Icon className="size-3.5 text-primary" />
        </div>
        <div className="flex min-w-0 flex-col">
          <span className="truncate font-mono text-[11px] font-black uppercase tracking-[1px] text-primary">
            {meta.label}
          </span>
          <span className="truncate font-mono text-[9px] text-muted-foreground">{name}</span>
        </div>
        <div
          className={cn(
            "ml-auto flex shrink-0 items-center gap-1.5 border px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[1px]",
            status === "error"
              ? "border-destructive/40 text-destructive"
              : running
                ? "border-primary/40 text-primary"
                : "border-green-500/30 text-green-500"
          )}
        >
          {status === "error" ? (
            <>
              <AlertTriangle className="size-2.5" /> error
            </>
          ) : running ? (
            <>
              <Loader2 className="size-2.5 animate-spin" /> running
            </>
          ) : (
            <>
              <CheckCircle2 className="size-2.5" /> done
            </>
          )}
        </div>
      </div>

      {/* Args preview (always visible when present, full JSON behind the toggle) */}
      {hasArgs && (
        <button
          type="button"
          onClick={() => setArgsOpen((v) => !v)}
          className="flex w-full items-center gap-1.5 px-3 py-1.5 text-left transition-colors hover:bg-primary/5"
        >
          <ChevronRight
            className={cn(
              "size-2.5 shrink-0 text-muted-foreground/60 transition-transform duration-200",
              argsOpen && "rotate-90"
            )}
          />
          <span className="truncate font-mono text-[10px] text-muted-foreground/80" title={text}>
            {data !== undefined ? JSON.stringify(data) : text}
          </span>
        </button>
      )}
      {argsOpen && text && (
        <div className="border-t border-primary/10 bg-background/40">
          {data !== undefined ? <JsonView source={text} /> : (
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all p-3 font-mono text-[10px] leading-relaxed text-muted-foreground/70">
              {text}
            </pre>
          )}
        </div>
      )}

      {/* Error detail */}
      {error && (
        <div className="flex items-start gap-1.5 border-t border-destructive/20 px-3 py-1.5">
          <AlertTriangle className="mt-0.5 size-2.5 shrink-0 text-destructive" />
          <span className="font-mono text-[10px] leading-relaxed text-destructive">{error}</span>
        </div>
      )}

      {/* Result */}
      {result && (
        <div className="border-t border-primary/10">
          <Section
            open={resultOpen}
            onToggle={() => setResultOpen((v) => !v)}
            label="Result"
            summary={summarizeResult(result)}
          >
            <div className="max-h-80 overflow-auto p-3">
              <ToolResultView name={name} result={result} />
            </div>
          </Section>
        </div>
      )}

      {/* Running shimmer */}
      {running && (
        <div className="h-0.5 overflow-hidden">
          <div className="h-full w-1/2 animate-pulse bg-gradient-to-r from-transparent via-primary/70 to-transparent" />
        </div>
      )}
    </div>
  )
}
