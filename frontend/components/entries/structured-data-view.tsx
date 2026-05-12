"use client"

import { Badge } from "@/components/ui/badge"
import { Calendar, CheckCircle2, Circle, Clock, Tag, AlertTriangle, Users, Flame, Utensils, ChefHat } from "lucide-react"
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
 * Specialized view for 'recipe' entries
 */
function RecipeView({ data }: { data: any }) {
  return (
    <div className="space-y-10">
      {/* Header Area */}
      <div className="relative">
        <div className="absolute -left-6 top-0 bottom-0 w-1 bg-primary/40 shadow-[0_0_10px_rgba(var(--primary-rgb),0.3)]" />
        <div className="pl-2 space-y-3">
          {/* <h3 className="text-2xl font-black tracking-tight text-foreground/95 uppercase font-mono">
            {data.title || "Untitled Recipe"}
          </h3> */}
          <div className="flex flex-wrap gap-3">
            {data.prep_minutes && (
              <div className="flex flex-col px-3 py-1 bg-muted/20 border border-border/40 rounded-sm">
                <span className="text-[9px] font-mono text-muted-foreground uppercase tracking-wider">Prep</span>
                <span className="text-sm font-mono font-bold text-foreground">{data.prep_minutes}m</span>
              </div>
            )}
            {data.cook_minutes && (
              <div className="flex flex-col px-3 py-1 bg-primary/5 border border-primary/20 rounded-sm">
                <span className="text-[9px] font-mono text-primary/70 uppercase tracking-wider">Cook</span>
                <span className="text-sm font-mono font-bold text-primary">{data.cook_minutes}m</span>
              </div>
            )}
            {data.servings && (
              <div className="flex flex-col px-3 py-1 bg-muted/20 border border-border/40 rounded-sm">
                <span className="text-[9px] font-mono text-muted-foreground uppercase tracking-wider">Servings</span>
                <span className="text-sm font-mono font-bold text-foreground">{data.servings}</span>
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-12 gap-12">
        {/* Ingredients Column */}
        <div className="md:col-span-4 space-y-6">
          <div className="flex items-center gap-3">
            <ChefHat className="size-4 text-primary/60" />
            <span className="font-mono text-[11px] font-black uppercase tracking-[4px] text-primary">Ingredients</span>
          </div>
          <div className="space-y-4">
            {data.ingredients?.map((ing: any, i: number) => (
              <div key={i} className="group border-l border-border/40 hover:border-primary/50 pl-4 py-0.5 transition-colors">
                <div className="flex flex-wrap items-baseline gap-2">
                  {ing.amount && <span className="font-mono text-xs font-bold text-primary/80">{ing.amount}</span>}
                  <span className="text-sm font-medium text-foreground/90 group-hover:text-foreground">{ing.item}</span>
                </div>
                {ing.note && <p className="text-[11px] text-muted-foreground italic mt-0.5 leading-tight">{ing.note}</p>}
              </div>
            ))}
          </div>
        </div>

        {/* Steps Column */}
        <div className="md:col-span-8 space-y-6">
          <div className="flex items-center gap-3">
            <Utensils className="size-4 text-primary/60" />
            <span className="font-mono text-[11px] font-black uppercase tracking-[4px] text-primary">Preparation</span>
          </div>
          <div className="space-y-8">
            {data.steps?.map((step: string, i: number) => (
              <div key={i} className="flex gap-6 group">
                <div className="shrink-0 flex flex-col items-center gap-2">
                  <div className="size-8 rounded flex items-center justify-center bg-muted/40 border border-border/60 text-muted-foreground font-mono text-xs group-hover:border-primary/50 group-hover:text-primary transition-all">
                    {(i + 1).toString().padStart(2, '0')}
                  </div>
                  <div className="w-px flex-1 bg-gradient-to-b from-border/60 to-transparent group-last:hidden" />
                </div>
                <div className="pb-8 group-last:pb-0">
                  <p className="text-[15px] leading-relaxed text-foreground/80 group-hover:text-foreground transition-colors">
                    {step}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Footer */}
      {data.tags && data.tags.length > 0 && (
        <div className="pt-8 border-t border-border/20 flex items-center gap-4">
          <Tag className="size-3.5 text-muted-foreground" />
          <EntryTagList tags={data.tags} />
        </div>
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
                    {typeof v === 'object' ? (
                      Object.entries(v).map(([vk, vv]) => `${vk}: ${vv}`).join(' | ')
                    ) : String(v)}
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
        ) : type === "recipe" ? (
          <RecipeView data={data} />
        ) : (
          <GenericDataView data={data} />
        )}
      </div>
    </div>
  )
}
