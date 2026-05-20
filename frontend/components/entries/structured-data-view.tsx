"use client"

import { Badge } from "@/components/ui/badge"
import { Calendar, CheckCircle2, Circle, Clock, Tag, AlertTriangle, Users, Flame, Utensils, ChefHat, Gavel, ArrowRight, History, ShieldCheck, XCircle, StickyNote, Link2, User } from "lucide-react"
import { EntryTagList } from "../ui/entry-tag"
import { MarkdownView } from "./markdown-view"

interface StructuredDataViewProps {
  type: string
  data: any
}

/**
 * Specialized view for 'note' entries
 */
function NoteView({ data }: { data: any }) {
  return (
    <div className="space-y-8">
      {/* Header section */}
      <div className="flex flex-wrap items-start justify-between gap-4 pb-4 border-b border-border/20">
        <div className="space-y-1.5">
          <div className="flex items-center gap-2 text-primary/60">
            <StickyNote className="size-4" />
            <span className="font-mono text-[10px] font-black uppercase tracking-[2px]">Memo</span>
          </div>
          <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Note"}</h3>
        </div>
        {(data.author || data.decided_at) && (
          <div className="flex flex-col items-end gap-1 text-right">
            {data.author && (
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground font-medium">
                <User className="size-3" />
                {data.author}
              </div>
            )}
            {data.decided_at && (
              <div className="flex items-center gap-1.5 text-[11px] font-mono text-muted-foreground/70 uppercase">
                <Calendar className="size-3" />
                {data.decided_at}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Body section */}
      {data.body && (
        <div className="relative group/note">
          {/* Subtle "paper" background or left border */}
          <div className="absolute -left-4 top-0 bottom-0 w-0.5 bg-muted transition-colors group-hover/note:bg-primary/30" />
          <div className="text-[15px] leading-relaxed text-foreground/80">
            <MarkdownView content={data.body} />
          </div>
        </div>
      )}

      {/* Tags and Links */}
      <div className="pt-6 space-y-4">
        {data.tags && Array.isArray(data.tags) && data.tags.length > 0 && (
          <div className="flex items-center gap-3">
            <Tag className="size-3.5 text-muted-foreground/60" />
            <EntryTagList tags={data.tags} />
          </div>
        )}
        
        {data.links && Array.isArray(data.links) && data.links.length > 0 && (
          <div className="space-y-2">
            <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80 flex items-center gap-1.5">
              <Link2 className="size-3" />
              References
            </span>
            <div className="flex flex-wrap gap-2">
              {data.links.map((link: string, i: number) => (
                <Badge 
                  key={i} 
                  variant="secondary" 
                  className="rounded-sm font-mono text-[10px] px-2 py-0.5 bg-muted/40 hover:bg-primary/5 hover:text-primary transition-colors cursor-pointer"
                >
                  {link}
                </Badge>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

/**
 * Specialized view for 'decision' entries
 */
function DecisionView({ data }: { data: any }) {
  const statusConfig = {
    proposed: { icon: <Circle className="size-4 text-blue-500" />, styles: "bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800" },
    accepted: { icon: <ShieldCheck className="size-4 text-emerald-500" />, styles: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
    superseded: { icon: <History className="size-4 text-amber-500" />, styles: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
    rejected: { icon: <XCircle className="size-4 text-red-500" />, styles: "bg-red-50 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800" },
  }

  const currentStatus = (data.status as keyof typeof statusConfig) || "proposed"
  const config = statusConfig[currentStatus]

  return (
    <div className="space-y-10">
      {/* Header section */}
      <div className="flex flex-wrap items-start justify-between gap-6 pb-6 border-b border-border/40">
        <div className="space-y-4 max-w-2xl">
          <h3 className="text-2xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Decision"}</h3>
          <div className="flex flex-wrap items-center gap-4">
            <div className={`flex items-center gap-2 px-3 py-1 rounded-full border text-xs font-semibold ${config.styles}`}>
              {config.icon}
              <span className="capitalize">{currentStatus}</span>
            </div>
            {data.decided_at && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground font-medium">
                <Calendar className="size-4" />
                {data.decided_at}
              </div>
            )}
          </div>
        </div>
        {data.deciders && data.deciders.length > 0 && (
          <div className="space-y-2">
            <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80 flex items-center gap-1.5">
              <Users className="size-3" />
              Deciders
            </span>
            <div className="flex flex-wrap gap-1.5">
              {data.deciders.map((decider: string, i: number) => (
                <Badge key={i} variant="outline" className="rounded-sm font-medium px-2 py-0.5 bg-muted/30">
                  {decider}
                </Badge>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Main Content Sections */}
      <div className="grid grid-cols-1 gap-10">
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-muted flex items-center justify-center border border-border/60">
              <Clock className="size-3.5 text-muted-foreground" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary/80">Context</span>
          </div>
          <div className="text-[15px] leading-relaxed text-foreground/80 pl-9">
            <MarkdownView content={data.context} />
          </div>
        </div>

        <div className="space-y-3 p-6 rounded-xl bg-primary/[0.03] border border-primary/10 relative overflow-hidden group/decision">
          <div className="absolute top-0 left-0 w-1 h-full bg-primary/40" />
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-primary/10 flex items-center justify-center border border-primary/20">
              <Gavel className="size-3.5 text-primary" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary">Decision</span>
          </div>
          <div className="text-[16px] font-medium leading-relaxed text-foreground pl-9">
            <MarkdownView content={data.decision} />
          </div>
        </div>

        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-muted flex items-center justify-center border border-border/60">
              <ArrowRight className="size-3.5 text-muted-foreground" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary/80">Consequences</span>
          </div>
          <div className="text-[15px] leading-relaxed text-foreground/80 pl-9">
            <MarkdownView content={data.consequences} />
          </div>
        </div>
      </div>

      {/* Linked Decisions */}
      {(data.supersedes || data.superseded_by) && (
        <div className="flex flex-wrap gap-8 pt-6 border-t border-border/40">
          {data.supersedes && (
            <div className="space-y-2">
              <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80">Supersedes</span>
              <div className="flex items-center gap-2 text-sm font-medium text-amber-600 dark:text-amber-500 hover:underline cursor-pointer">
                <History className="size-3.5" />
                {data.supersedes}
              </div>
            </div>
          )}
          {data.superseded_by && (
            <div className="space-y-2">
              <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80">Superseded By</span>
              <div className="flex items-center gap-2 text-sm font-medium text-blue-600 dark:text-blue-500 hover:underline cursor-pointer">
                <ShieldCheck className="size-3.5" />
                {data.superseded_by}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
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
        <div className="p-4 rounded-lg bg-muted/40 dark:bg-black/40 border border-border/60 text-sm leading-relaxed text-muted-foreground">
          <MarkdownView content={data.notes} />
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
        {type === "decision" ? (
          <DecisionView data={data} />
        ) : type === "note" ? (
          <NoteView data={data} />
        ) : type === "todo" ? (
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
