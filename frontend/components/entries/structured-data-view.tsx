"use client"

import { Badge } from "@/components/ui/badge"
import { Calendar, CheckCircle2, Circle, Clock, Tag, AlertTriangle } from "lucide-react"
import { EntryTagList } from "../ui/entry-tag"

interface StructuredDataViewProps {
  type: string
  data: any
}

/**
 * Specialized view for 'todo' entries
 */
function TodoView({ data }: { data: any }) {
  const isDone = data.status === "done"
  const statusIcon = isDone ? <CheckCircle2 className="size-4 text-emerald-500" /> : <Circle className="size-4 text-primary/60" />
  
  // Use semantic priority colors that work in both modes
  const priorityStyles = data.priority === "high" 
    ? "text-red-600 dark:text-red-400" 
    : data.priority === "medium" 
      ? "text-amber-600 dark:text-amber-400" 
      : "text-blue-600 dark:text-blue-400"

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <h3 className="text-lg font-bold tracking-tight text-foreground/90">{data.title || "Untitled Todo"}</h3>
          <div className="flex items-center gap-3 text-xs font-mono text-muted-foreground uppercase tracking-widest">
            <span className="flex items-center gap-1.5">
              {statusIcon}
              <span className={isDone ? "text-emerald-600 dark:text-emerald-500" : ""}>{data.status || "open"}</span>
            </span>
            <span className="w-px h-3 bg-border" />
            <span className={`flex items-center gap-1.5 ${priorityStyles}`}>
              <AlertTriangle className="size-3" />
              {data.priority || "normal"} priority
            </span>
          </div>
        </div>
        {data.due && (
          <div className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-primary/5 border border-primary/20 text-primary font-mono text-[11px] uppercase tracking-wider">
            <Calendar className="size-3.5" />
            Due: {data.due}
          </div>
        )}
      </div>

      {data.notes && (
        <div className="p-4 rounded-lg bg-muted/40 dark:bg-black/40 border border-border/60 text-sm leading-relaxed text-muted-foreground italic font-serif">
          {data.notes}
        </div>
      )}

      {data.tags && Array.isArray(data.tags) && data.tags.length > 0 && (
        <EntryTagList tags={data.tags} />
      )}
    </div>
  )
}

/**
 * Generic Property Grid for any data type
 */
function GenericDataView({ data }: { data: any }) {
  if (typeof data !== "object" || data === null) {
    return <pre className="text-xs font-mono p-4 bg-muted/30 rounded">{JSON.stringify(data)}</pre>
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-x-8 gap-y-6">
      {Object.entries(data).map(([key, val]) => (
        <div key={key} className="space-y-1.5 border-l-2 border-primary/20 pl-4 py-1 hover:bg-primary/[0.03] transition-colors group">
          <div className="font-mono text-[10px] font-black uppercase tracking-[2px] text-primary/70 group-hover:text-primary transition-colors">
            {key.replace(/_/g, " ")}
          </div>
          <div className="text-sm font-medium text-foreground/80">
            {Array.isArray(val) ? (
              <div className="flex flex-wrap gap-1.5 mt-1">
                {val.map((v, i) => (
                  <Badge key={i} variant="secondary" className="text-[10px] h-5 rounded-sm px-1.5 font-mono bg-muted/50 border-none">
                    {String(v)}
                  </Badge>
                ))}
              </div>
            ) : typeof val === "object" ? (
              <pre className="text-[11px] font-mono text-muted-foreground bg-muted/50 p-2 rounded mt-1 overflow-x-auto">
                {JSON.stringify(val, null, 2)}
              </pre>
            ) : (
              String(val)
            )}
          </div>
        </div>
      ))}
    </div>
  )
}

export function StructuredDataView({ type, data }: StructuredDataViewProps) {
  if (!data) return null

  return (
    <div className="relative group overflow-hidden rounded-xl border border-border bg-card/50 dark:bg-black/40 backdrop-blur-md p-6 md:p-8 shadow-sm dark:shadow-[0_0_40px_rgba(var(--primary-rgb),0.03)]">
      {/* Background decoration */}
      <div className="absolute -left-20 -bottom-20 size-60 bg-primary/5 blur-[80px] pointer-events-none opacity-40 group-hover:opacity-70 transition-opacity duration-700" />
      
      <div className="relative z-10">
        {type === "todo" ? (
          <TodoView data={data} />
        ) : (
          <GenericDataView data={data} />
        )}
      </div>
    </div>
  )
}
