"use client"

import type { ReactNode } from "react"
import { useMemo } from "react"
import Link from "next/link"
import { Badge } from "@/components/ui/badge"
import { Calendar, CheckCircle2, Circle, Clock, Tag, AlertTriangle, Users, Flame, Utensils, ChefHat, Gavel, ArrowRight, History, ShieldCheck, XCircle, StickyNote, Link2, User, Activity, AlertCircle, ShieldAlert, Wrench, Server, Dumbbell, Timer, Footprints, TrendingUp, BookOpen, GitBranch, FileText, Hash, GitCommitHorizontal, FolderGit2, ListChecks, Target, ListOrdered, Bot, Radio, Gauge, Layers, Boxes, Scale, Zap, MinusCircle, PlayCircle, Braces, Network } from "lucide-react"
import { EntryTagList } from "../ui/entry-tag"
import { MarkdownView } from "./markdown-view"
import { useHarnessTree } from "@/lib/api/hooks"
import type { HarnessTreeNode } from "@/lib/api"

interface StructuredDataViewProps {
  type: string
  data: any
  /** Entry id this data belongs to — lets views (e.g. harness_repo) cross-reference the harness graph. */
  entryId?: string
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

        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-primary/10 flex items-center justify-center border border-primary/20">
              <Gavel className="size-3.5 text-primary" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary">Decision</span>
          </div>
          <div className="py-4 pr-4 pl-9 rounded-xl bg-primary/[0.03] border border-primary/10 relative overflow-hidden group/decision">
            <div className="absolute top-0 left-0 w-1 h-full bg-primary/40" />
            <div className="text-[16px] font-medium leading-relaxed text-foreground">
              <MarkdownView content={data.decision} />
            </div>
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
  const statusIcon = isDone ? <CheckCircle2 className="size-6 text-emerald-500" /> : <Circle className="size-6 text-primary/60" />

  // Use semantic priority colors that work in both modes
  const priorityStyles = data.priority === "high"
    ? "text-red-600 dark:text-red-400"
    : data.priority === "medium"
      ? "text-amber-600 dark:text-amber-400"
      : "text-blue-600 dark:text-blue-400"

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <div className="shrink-0 mt-0.5">{statusIcon}</div>
          <div className="space-y-1">
            <h3 className="text-lg font-bold tracking-tight text-foreground/90">{data.title || "Untitled Todo"}</h3>
            <div className="flex items-center gap-3 text-xs font-mono text-muted-foreground uppercase tracking-widest">
              <span className={isDone ? "text-emerald-600 dark:text-emerald-500" : ""}>{data.status || "open"}</span>
              <span className="w-px h-3 bg-border" />
              <span className={`flex items-center gap-1.5 ${priorityStyles}`}>
                <AlertTriangle className="size-3" />
                {data.priority || "normal"} priority
              </span>
            </div>
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
        <div className="pl-9">
          <div className="p-4 rounded-lg bg-muted/40 dark:bg-black/40 border border-border/60 text-sm leading-relaxed text-muted-foreground">
            <MarkdownView content={data.notes} />
          </div>
        </div>
      )}

      {data.tags && Array.isArray(data.tags) && data.tags.length > 0 && (
        <div className="pl-9 pt-2">
          <EntryTagList tags={data.tags} />
        </div>
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

      <div className="grid grid-cols-1 md:grid-cols-12 gap-12">
        {/* Ingredients Column */}
        <div className="md:col-span-4 space-y-6">
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-muted flex items-center justify-center border border-border/60">
              <ChefHat className="size-3.5 text-muted-foreground" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[4px] text-primary/80">Ingredients</span>
          </div>
          <div className="space-y-4 pl-9">
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
            <div className="size-6 rounded-md bg-muted flex items-center justify-center border border-border/60">
              <Utensils className="size-3.5 text-muted-foreground" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[4px] text-primary/80">Preparation</span>
          </div>
          <div className="space-y-8 pl-9">
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
 * Specialized view for 'fact' entries
 */
function FactView({ data }: { data: any }) {
  const confidencePercent = Math.round((data.confidence || 0) * 100)
  const confidenceColor = confidencePercent > 80 ? "text-emerald-500" : confidencePercent > 50 ? "text-amber-500" : "text-red-500"

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1.5">
          <div className="flex items-center gap-2 text-primary/60">
            <ShieldCheck className="size-4" />
            <span className="font-mono text-[10px] font-black uppercase tracking-[2px]">Fact</span>
          </div>
          <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.fact || "Untitled Fact"}</h3>
        </div>
        <div className="flex flex-col items-end gap-1">
          <div className={`text-xs font-mono font-bold ${confidenceColor}`}>
            {confidencePercent}% Confidence
          </div>
          {data.last_verified && (
            <div className="text-[10px] font-mono text-muted-foreground uppercase">
              Verified: {data.last_verified}
            </div>
          )}
        </div>
      </div>

      {data.source && (
        <div className="p-3 rounded bg-muted/30 border border-border/40 text-xs text-muted-foreground flex items-center gap-2">
          <Link2 className="size-3" />
          <span className="font-medium italic truncate max-w-md">Source: {data.source}</span>
        </div>
      )}

      {data.tags && Array.isArray(data.tags) && data.tags.length > 0 && (
        <div className="pt-2">
          <EntryTagList tags={data.tags} />
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'workout' entries
 */
function WorkoutView({ data }: { data: any }) {
  return (
    <div className="space-y-8">
      {/* Header section */}
      <div className="flex flex-wrap items-end justify-between gap-6 pb-6 border-b border-border/40">
        <div className="space-y-3">
          <div className="flex items-center gap-2 text-primary/60">
            <TrendingUp className="size-4" />
            <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">Performance Log</span>
          </div>
          <div className="flex items-center gap-4">
            <div className="size-12 rounded-2xl bg-primary/10 flex items-center justify-center border border-primary/20">
              <Dumbbell className="size-6 text-primary" />
            </div>
            <div>
              <h3 className="text-2xl font-bold tracking-tight text-foreground/90">Training Session</h3>
              <div className="flex items-center gap-2 text-sm text-muted-foreground mt-1">
                <Calendar className="size-3.5" />
                <span>{data.date}</span>
                {data.duration_minutes && (
                  <>
                    <span className="mx-1 opacity-30">|</span>
                    <Timer className="size-3.5" />
                    <span>{data.duration_minutes}m</span>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>

        {data.notes && (
          <div className="max-w-md p-4 rounded-xl bg-muted/30 border border-border/40 italic text-sm text-muted-foreground leading-relaxed">
            {data.notes}
          </div>
        )}
      </div>

      {/* Exercises List */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {data.exercises?.map((ex: any, i: number) => (
          <div key={i} className="group relative p-6 rounded-2xl border border-border/60 bg-card hover:border-primary/40 hover:bg-primary/[0.01] transition-all overflow-hidden">
            <div className="absolute top-0 right-0 p-3 opacity-10 group-hover:opacity-20 transition-opacity">
               {ex.sets?.[0]?.distance_m ? <Footprints className="size-12" /> : <Dumbbell className="size-12" />}
            </div>
            
            <div className="space-y-4">
              <h4 className="font-bold text-lg text-foreground/90 group-hover:text-primary transition-colors flex items-center gap-2">
                {ex.name}
              </h4>

              <div className="flex flex-wrap gap-2">
                {ex.sets?.map((set: any, si: number) => (
                  <div key={si} className="px-3 py-2 rounded-lg bg-muted/40 border border-border/40 flex flex-col items-center min-w-[60px] group/set hover:border-primary/30 transition-colors">
                    <span className="text-[9px] font-mono font-black uppercase tracking-widest text-muted-foreground/60 group-hover/set:text-primary/60 transition-colors">
                      Set {si + 1}
                    </span>
                    <div className="flex items-baseline gap-0.5">
                      {set.reps !== undefined && (
                        <>
                          <span className="text-sm font-bold">{set.reps}</span>
                          <span className="text-[10px] text-muted-foreground">x</span>
                        </>
                      )}
                      {set.weight_kg !== undefined && (
                        <>
                          <span className="text-sm font-bold">{set.weight_kg}</span>
                          <span className="text-[10px] text-muted-foreground">kg</span>
                        </>
                      )}
                      {set.distance_m !== undefined && (
                        <>
                          <span className="text-sm font-bold">{set.distance_m / 1000}</span>
                          <span className="text-[10px] text-muted-foreground">km</span>
                        </>
                      )}
                      {set.duration_seconds !== undefined && (
                        <>
                          <span className="text-sm font-bold">{set.duration_seconds}</span>
                          <span className="text-[10px] text-muted-foreground">s</span>
                        </>
                      )}
                    </div>
                  </div>
                ))}
              </div>

              {ex.notes && (
                <p className="text-xs text-muted-foreground border-l-2 border-primary/20 pl-3 py-0.5 mt-2 line-clamp-2 italic">
                  {ex.notes}
                </p>
              )}
            </div>
          </div>
        ))}
      </div>

      {data.tags && data.tags.length > 0 && (
        <div className="pt-4 flex flex-wrap gap-2">
          <EntryTagList tags={data.tags} />
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'incident' entries
 */
function IncidentView({ data }: { data: any }) {
  const statusConfig = {
    active: { icon: <Activity className="size-4 text-red-500 animate-pulse" />, styles: "bg-red-50 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800" },
    monitoring: { icon: <Clock className="size-4 text-amber-500" />, styles: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
    resolved: { icon: <CheckCircle2 className="size-4 text-emerald-500" />, styles: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
    post_mortem: { icon: <ShieldCheck className="size-4 text-blue-500" />, styles: "bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800" },
  }

  const severityConfig = {
    sev1: { label: "Critical", styles: "bg-red-600 text-white border-red-700" },
    sev2: { label: "High", styles: "bg-orange-500 text-white border-orange-600" },
    sev3: { label: "Medium", styles: "bg-amber-500 text-white border-amber-600" },
    sev4: { label: "Low", styles: "bg-blue-500 text-white border-blue-600" },
  }

  const currentStatus = (data.status as keyof typeof statusConfig) || "active"
  const sConfig = statusConfig[currentStatus]
  const currentSeverity = (data.severity as keyof typeof severityConfig) || "sev3"
  const sevConfig = severityConfig[currentSeverity]

  return (
    <div className="space-y-8">
      {/* Header section */}
      <div className="flex flex-wrap items-start justify-between gap-6 pb-6 border-b border-border/40">
        <div className="space-y-4 max-w-2xl">
          <div className="flex items-center gap-2">
             <div className={`px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border ${sevConfig.styles}`}>
              {sevConfig.label}
            </div>
            <h3 className="text-2xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Incident"}</h3>
          </div>
          <div className="flex flex-wrap items-center gap-4">
            <div className={`flex items-center gap-2 px-3 py-1 rounded-full border text-xs font-semibold ${sConfig.styles}`}>
              {sConfig.icon}
              <span className="capitalize">{currentStatus.replace('_', ' ')}</span>
            </div>
            <div className="flex items-center gap-2 text-sm text-muted-foreground font-medium">
              <Calendar className="size-4" />
              <span>{data.started_at}</span>
              {data.resolved_at && (
                <>
                  <ArrowRight className="size-3" />
                  <span>{data.resolved_at}</span>
                </>
              )}
            </div>
          </div>
        </div>
        
        {data.responders && data.responders.length > 0 && (
          <div className="space-y-2">
            <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80 flex items-center gap-1.5">
              <Users className="size-3" />
              Responders
            </span>
            <div className="flex flex-wrap gap-1.5">
              {data.responders.map((responder: string, i: number) => (
                <Badge key={i} variant="outline" className="rounded-sm font-medium px-2 py-0.5 bg-muted/30">
                  {responder}
                </Badge>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Main Content Sections */}
      <div className="grid grid-cols-1 gap-8">
        {/* Summary */}
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-muted flex items-center justify-center border border-border/60">
              <Activity className="size-3.5 text-muted-foreground" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary/80">Summary</span>
          </div>
          <div className="text-[15px] leading-relaxed text-foreground/80 pl-9">
            <MarkdownView content={data.summary} />
          </div>
        </div>

        {/* Affected Services */}
        {data.affected_services && data.affected_services.length > 0 && (
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <div className="size-6 rounded-md bg-muted flex items-center justify-center border border-border/60">
                <Server className="size-3.5 text-muted-foreground" />
              </div>
              <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary/80">Affected Services</span>
            </div>
            <div className="flex flex-wrap gap-2 pl-9">
              {data.affected_services.map((service: string, i: number) => (
                <Badge key={i} variant="secondary" className="bg-primary/5 text-primary border-primary/10">
                  {service}
                </Badge>
              ))}
            </div>
          </div>
        )}

        {/* Root Cause */}
        {data.root_cause && (
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <div className="size-6 rounded-md bg-amber-500/10 flex items-center justify-center border border-amber-500/20">
                <ShieldAlert className="size-3.5 text-amber-600" />
              </div>
              <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-amber-600">Root Cause</span>
            </div>
            <div className="py-4 pr-4 pl-9 rounded-xl bg-amber-500/[0.03] border border-amber-500/10 relative overflow-hidden">
              <div className="absolute top-0 left-0 w-1 h-full bg-amber-500/40" />
              <div className="text-[15px] leading-relaxed text-foreground/80">
                <MarkdownView content={data.root_cause} />
              </div>
            </div>
          </div>
        )}

        {/* Resolution */}
        {data.resolution && (
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <div className="size-6 rounded-md bg-emerald-500/10 flex items-center justify-center border border-emerald-500/20">
                <Wrench className="size-3.5 text-emerald-600" />
              </div>
              <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-emerald-600">Resolution</span>
            </div>
            <div className="py-4 pr-4 pl-9 rounded-xl bg-emerald-500/[0.03] border border-emerald-500/10 relative overflow-hidden">
              <div className="absolute top-0 left-0 w-1 h-full bg-emerald-500/40" />
              <div className="text-[15px] leading-relaxed text-foreground/80">
                <MarkdownView content={data.resolution} />
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

/**
 * Specialized view for 'harness_doc' entries (ADR, SPEC, INVARIANT, OVERVIEW, ARCHITECTURE, GUIDE)
 */
function HarnessDocView({ data }: { data: any }) {
  const docTypeConfig: Record<string, { styles: string }> = {
    ADR: { styles: "bg-violet-50 text-violet-700 border-violet-200 dark:bg-violet-900/30 dark:text-violet-400 dark:border-violet-800" },
    SPEC: { styles: "bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800" },
    INVARIANT: { styles: "bg-red-50 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800" },
    OVERVIEW: { styles: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
    ARCHITECTURE: { styles: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
    GUIDE: { styles: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
  }
  const docType = (data.doc_type as string) || "DOC"
  const dConfig = docTypeConfig[docType] || { styles: "bg-muted text-muted-foreground border-border" }
  const statusConfig: Record<string, { icon: ReactNode; styles: string }> = {
    ACTIVE: { icon: <ShieldCheck className="size-3.5" />, styles: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
    DRAFT: { icon: <Circle className="size-3.5" />, styles: "bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800" },
    SUPERSEDED: { icon: <History className="size-3.5" />, styles: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
    DEPRECATED: { icon: <XCircle className="size-3.5" />, styles: "bg-red-50 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800" },
  }
  const status = ((data.status as string) || "ACTIVE").toUpperCase()
  const sConfig = statusConfig[status] || statusConfig.ACTIVE

  const sections = Array.isArray(data.sections) ? data.sections : []
  const sourceFiles = Array.isArray(data.source_files) ? data.source_files : []
  const invariants = Array.isArray(data.invariants) ? data.invariants : []

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex flex-wrap items-start justify-between gap-6 pb-6 border-b border-border/40">
        <div className="space-y-3 max-w-2xl">
          <div className="flex flex-wrap items-center gap-2">
            <div className={`flex items-center gap-1.5 px-2 py-0.5 rounded text-[10px] font-black uppercase tracking-wider border ${dConfig.styles}`}>
              <BookOpen className="size-3" />
              {docType}
            </div>
            {data.version && (
              <span className="flex items-center gap-1 font-mono text-[10px] text-muted-foreground uppercase">
                <Hash className="size-3" />v{data.version}
              </span>
            )}
          </div>
          <h3 className="text-2xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Document"}</h3>
          <div className="flex flex-wrap items-center gap-3">
            <div className={`flex items-center gap-1.5 px-2.5 py-0.5 rounded-full border text-xs font-semibold ${sConfig.styles}`}>
              {sConfig.icon}
              <span className="capitalize">{status.toLowerCase()}</span>
            </div>
            {data.repo && (
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground font-mono">
                <FolderGit2 className="size-3.5" />
                {data.repo}
                {data.repo_path && <span className="text-muted-foreground/60">/{String(data.repo_path).replace(/^\//, "")}</span>}
              </div>
            )}
          </div>
        </div>
        {data.git_sha && (
          <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-muted/40 border border-border/40 font-mono text-[11px] text-muted-foreground">
            <GitCommitHorizontal className="size-3.5" />
            {String(data.git_sha).slice(0, 10)}
          </div>
        )}
      </div>

      {/* Summary */}
      {data.summary && (
        <div className="text-[15px] leading-relaxed text-foreground/80 italic">
          <MarkdownView content={data.summary} />
        </div>
      )}

      {/* ADR-style Context / Decision / Consequences */}
      {(data.context || data.decision || data.consequences) && (
        <div className="grid grid-cols-1 gap-8">
          {data.context && (
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
          )}
          {data.decision && (
            <div className="space-y-3">
              <div className="flex items-center gap-3">
                <div className="size-6 rounded-md bg-primary/10 flex items-center justify-center border border-primary/20">
                  <Gavel className="size-3.5 text-primary" />
                </div>
                <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary">Decision</span>
              </div>
              <div className="py-4 pr-4 pl-9 rounded-xl bg-primary/[0.03] border border-primary/10 relative overflow-hidden">
                <div className="absolute top-0 left-0 w-1 h-full bg-primary/40" />
                <div className="text-[16px] font-medium leading-relaxed text-foreground">
                  <MarkdownView content={data.decision} />
                </div>
              </div>
            </div>
          )}
          {data.consequences && (
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
          )}
        </div>
      )}

      {/* Table of contents from extracted sections */}
      {sections.length > 0 && (
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-muted flex items-center justify-center border border-border/60">
              <ListChecks className="size-3.5 text-muted-foreground" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary/80">Sections</span>
          </div>
          <div className="pl-9 space-y-2">
            {sections.map((s: any, i: number) => (
              <a
                key={i}
                href={s.anchor ? `#${s.anchor}` : undefined}
                className="group flex items-start gap-2 text-sm hover:text-primary transition-colors"
              >
                <span className="font-mono text-[10px] text-muted-foreground/60 mt-0.5">{(i + 1).toString().padStart(2, "0")}</span>
                <div>
                  <div className="font-medium text-foreground/90 group-hover:text-primary">{s.title}</div>
                  {s.summary && <div className="text-xs text-muted-foreground line-clamp-1">{s.summary}</div>}
                </div>
              </a>
            ))}
          </div>
        </div>
      )}

      {/* Invariants */}
      {invariants.length > 0 && (
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="size-6 rounded-md bg-red-500/10 flex items-center justify-center border border-red-500/20">
              <ShieldAlert className="size-3.5 text-red-600" />
            </div>
            <span className="font-mono text-[11px] font-black uppercase tracking-[3px] text-red-600">Invariants</span>
          </div>
          <ul className="pl-9 space-y-1.5 list-disc marker:text-red-500/50">
            {invariants.map((inv: any, i: number) => (
              <li key={i} className="text-sm text-foreground/80 leading-relaxed">
                {typeof inv === "string" ? inv : JSON.stringify(inv)}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Source files */}
      {sourceFiles.length > 0 && (
        <div className="space-y-3 pt-4 border-t border-border/20">
          <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80 flex items-center gap-1.5">
            <FileText className="size-3" />
            Source Files
          </span>
          <div className="flex flex-wrap gap-2">
            {sourceFiles.map((f: string, i: number) => (
              <Badge key={i} variant="secondary" className="rounded-sm font-mono text-[10px] px-2 py-0.5 bg-muted/40">
                {f}
              </Badge>
            ))}
          </div>
        </div>
      )}

      {/* Lineage */}
      {(data.parent_doc_id || data.supersedes) && (
        <div className="flex flex-wrap gap-8 pt-4 border-t border-border/20">
          {data.parent_doc_id && (
            <div className="space-y-2">
              <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80">Parent Doc</span>
              <div className="flex items-center gap-2 text-sm font-medium text-primary hover:underline cursor-pointer">
                <GitBranch className="size-3.5" />
                {data.parent_doc_id}
              </div>
            </div>
          )}
          {data.supersedes && (
            <div className="space-y-2">
              <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80">Supersedes</span>
              <div className="flex items-center gap-2 text-sm font-medium text-amber-600 dark:text-amber-500 hover:underline cursor-pointer">
                <History className="size-3.5" />
                {data.supersedes}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

/** Small uppercase mono label used as a section header across harness views. */
function HarnessSectionLabel({ icon, children, tone = "muted" }: { icon: ReactNode; children: ReactNode; tone?: "muted" | "primary" | "red" | "amber" | "emerald" }) {
  const toneStyles: Record<string, string> = {
    muted: "bg-muted border-border/60 text-muted-foreground",
    primary: "bg-primary/10 border-primary/20 text-primary",
    red: "bg-red-500/10 border-red-500/20 text-red-600",
    amber: "bg-amber-500/10 border-amber-500/20 text-amber-600",
    emerald: "bg-emerald-500/10 border-emerald-500/20 text-emerald-600",
  }
  return (
    <div className="flex items-center gap-3">
      <div className={`size-6 rounded-md flex items-center justify-center border ${toneStyles[tone]}`}>
        {icon}
      </div>
      <span className={`font-mono text-[11px] font-black uppercase tracking-[3px] ${tone === "muted" ? "text-primary/80" : toneStyles[tone].split(" ").pop()}`}>
        {children}
      </span>
    </div>
  )
}

/** Reference chip for pointing at another harness node id (plan_id, sprint_id, ...). */
function RefChip({ label, value }: { label: string; value?: string }) {
  if (!value) return null
  return (
    <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-muted/40 border border-border/40 font-mono text-[11px] text-muted-foreground">
      <Link2 className="size-3" />
      <span className="uppercase tracking-wider text-muted-foreground/70">{label}</span>
      <span className="text-foreground/80">{value}</span>
    </div>
  )
}

/** Standard state pill shared by plan/sprint/todo/validation harness nodes. */
function StatePill({ state, styles }: { state?: string; styles: Record<string, { icon: ReactNode; classes: string }> }) {
  if (!state) return null
  const config = styles[state] ?? { icon: <Circle className="size-3.5" />, classes: "bg-muted text-muted-foreground border-border" }
  return (
    <div className={`flex items-center gap-1.5 px-2.5 py-0.5 rounded-full border text-xs font-semibold ${config.classes}`}>
      {config.icon}
      <span className="capitalize">{state.toLowerCase().replace(/_/g, " ")}</span>
    </div>
  )
}

const PLAN_STATE_STYLES: Record<string, { icon: ReactNode; classes: string }> = {
  PROPOSED: { icon: <Circle className="size-3.5" />, classes: "bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800" },
  GRILLED: { icon: <Flame className="size-3.5" />, classes: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
  APPROVED: { icon: <ShieldCheck className="size-3.5" />, classes: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
}

/**
 * Specialized view for 'harness_plan' entries
 */
function HarnessPlanView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4 pb-4 border-b border-border/40">
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-primary/60">
            <Target className="size-4" />
            <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">Plan</span>
          </div>
          <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Plan"}</h3>
        </div>
        <StatePill state={data.state} styles={PLAN_STATE_STYLES} />
      </div>
      {data.repo && <RefChip label="Repo" value={data.repo} />}
    </div>
  )
}

const SPRINT_STATE_STYLES: Record<string, { icon: ReactNode; classes: string }> = {
  PLANNED: { icon: <Circle className="size-3.5" />, classes: "bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800" },
  ACTIVE: { icon: <PlayCircle className="size-3.5" />, classes: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
  COMPLETED: { icon: <CheckCircle2 className="size-3.5" />, classes: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
}

/**
 * Specialized view for 'harness_sprint' entries
 */
function HarnessSprintView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4 pb-4 border-b border-border/40">
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-primary/60">
            <Layers className="size-4" />
            <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">Sprint · {data.cadence}</span>
          </div>
          <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.goal || "Untitled Sprint"}</h3>
        </div>
        <StatePill state={data.state} styles={SPRINT_STATE_STYLES} />
      </div>
      <div className="flex flex-wrap gap-2">
        <RefChip label="Plan" value={data.plan_id} />
        <RefChip label="Repo" value={data.repo} />
      </div>
    </div>
  )
}

const TODO_STATE_STYLES: Record<string, { icon: ReactNode; classes: string }> = {
  PENDING: { icon: <Circle className="size-3.5" />, classes: "bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800" },
  IN_PROGRESS: { icon: <PlayCircle className="size-3.5" />, classes: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
  BLOCKED: { icon: <XCircle className="size-3.5" />, classes: "bg-red-50 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800" },
  DONE: { icon: <CheckCircle2 className="size-3.5" />, classes: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
}

/**
 * Specialized view for 'harness_todo' entries
 */
function HarnessTodoView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4 pb-4 border-b border-border/40">
        <div className="flex items-start gap-3">
          <div className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted border border-border/60 font-mono text-xs font-bold text-muted-foreground">
            {data.sequence_order ?? "–"}
          </div>
          <div className="space-y-2">
            <h3 className="text-lg font-bold tracking-tight text-foreground/90">{data.title || "Untitled Todo"}</h3>
            <StatePill state={data.state} styles={TODO_STATE_STYLES} />
          </div>
        </div>
        <RefChip label="Sprint" value={data.sprint_id} />
      </div>
      {data.action_spec && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<Braces className="size-3.5 text-muted-foreground" />}>Action Spec</HarnessSectionLabel>
          <pre className="pl-9 text-[11px] font-mono text-muted-foreground bg-muted/50 p-3 rounded-lg overflow-x-auto">
            {JSON.stringify(data.action_spec, null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_agent' entries
 */
function HarnessAgentView({ data }: { data: any }) {
  const capabilities = Array.isArray(data.capabilities) ? data.capabilities : []
  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4 pb-4 border-b border-border/40">
        <div className="size-12 rounded-2xl bg-primary/10 flex items-center justify-center border border-primary/20">
          <Bot className="size-6 text-primary" />
        </div>
        <div>
          <span className="font-mono text-[10px] font-black uppercase tracking-[3px] text-primary/60">Agent Role</span>
          <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.role || "Untitled Agent"}</h3>
        </div>
      </div>
      {capabilities.length > 0 && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<ListChecks className="size-3.5 text-muted-foreground" />}>Capabilities</HarnessSectionLabel>
          <div className="flex flex-wrap gap-1.5 pl-9">
            {capabilities.map((c: string, i: number) => (
              <Badge key={i} variant="secondary" className="rounded-sm font-mono text-[10px] px-2 py-0.5 bg-muted/40">
                {c}
              </Badge>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_stream' entries
 */
function HarnessStreamView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4 pb-4 border-b border-border/40">
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-primary/60">
            <Radio className="size-4" />
            <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">Event Stream</span>
          </div>
          <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.stream_name || "Untitled Stream"}</h3>
        </div>
        {data.aggregate_type && (
          <Badge variant="outline" className="font-mono text-[10px] uppercase">{data.aggregate_type}</Badge>
        )}
      </div>
      {data.schema_definition && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<Braces className="size-3.5 text-muted-foreground" />}>Payload Schema</HarnessSectionLabel>
          <pre className="pl-9 text-[11px] font-mono text-muted-foreground bg-muted/50 p-3 rounded-lg overflow-x-auto">
            {JSON.stringify(data.schema_definition, null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_audit' entries (grill verdicts)
 */
function HarnessAuditView({ data }: { data: any }) {
  const violations = Array.isArray(data.violations) ? data.violations : []
  const unanchored = Array.isArray(data.unanchored_assumptions) ? data.unanchored_assumptions : []
  const suggestedEdits = Array.isArray(data.suggested_edits) ? data.suggested_edits : []
  const passed = !!data.passed
  const scorePercent = Math.round((data.score || 0) * 100)

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-center justify-between gap-6 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className={`flex items-center gap-2 px-3 py-1.5 rounded-full border text-sm font-semibold ${passed ? "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" : "bg-red-50 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800"}`}>
            {passed ? <ShieldCheck className="size-4" /> : <ShieldAlert className="size-4" />}
            {passed ? "Passed" : "Failed"}
          </div>
          <div className="flex items-center gap-2">
            <Gauge className="size-4 text-muted-foreground" />
            <span className={`font-mono text-sm font-bold ${scorePercent >= 80 ? "text-emerald-600" : scorePercent >= 50 ? "text-amber-600" : "text-red-600"}`}>
              {scorePercent}%
            </span>
          </div>
        </div>
        <div className="flex flex-col items-end gap-1 text-right">
          {data.auditor && (
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground font-medium">
              <User className="size-3" />
              {data.auditor}
            </div>
          )}
          <RefChip label="Target" value={data.target_id} />
        </div>
      </div>

      {violations.length > 0 && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<AlertTriangle className="size-3.5 text-red-600" />} tone="red">
            Violations ({violations.length})
          </HarnessSectionLabel>
          <div className="pl-9 space-y-2">
            {violations.map((v: any, i: number) => (
              <div key={i} className="flex items-start gap-3 p-3 rounded-lg bg-red-500/[0.03] border border-red-500/10">
                <Badge
                  variant="outline"
                  className={`shrink-0 text-[10px] uppercase font-black ${v.severity === "BLOCKING" ? "bg-red-600 text-white border-red-700" : "bg-amber-500 text-white border-amber-600"}`}
                >
                  {v.severity}
                </Badge>
                <div className="space-y-0.5">
                  <div className="text-sm font-medium text-foreground/90">{v.title}</div>
                  <div className="text-xs text-muted-foreground">{v.violation_reason}</div>
                  {v.doc_id && <div className="text-[10px] font-mono text-muted-foreground/60">{v.doc_id}</div>}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {unanchored.length > 0 && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<AlertCircle className="size-3.5 text-amber-600" />} tone="amber">
            Unanchored Assumptions ({unanchored.length})
          </HarnessSectionLabel>
          <ul className="pl-9 space-y-1.5 list-disc marker:text-amber-500/50">
            {unanchored.map((a: string, i: number) => (
              <li key={i} className="text-sm text-foreground/80 leading-relaxed">{a}</li>
            ))}
          </ul>
        </div>
      )}

      {suggestedEdits.length > 0 && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<Wrench className="size-3.5 text-muted-foreground" />}>
            Suggested Edits ({suggestedEdits.length})
          </HarnessSectionLabel>
          <div className="pl-9 space-y-2">
            {suggestedEdits.map((e: any, i: number) => (
              <div key={i} className="text-sm text-foreground/80 border-l-2 border-primary/20 pl-3 py-0.5">
                <span className="font-mono text-[10px] text-muted-foreground/60 mr-2">{e.doc_id}</span>
                {e.suggestion}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_poc' entries (proof-of-concept sessions)
 */
function HarnessPocView({ data }: { data: any }) {
  const timestamp = typeof data.timestamp === "number" ? new Date(data.timestamp).toLocaleString() : null
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4 pb-4 border-b border-border/40">
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-primary/60">
            <FolderGit2 className="size-4" />
            <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">POC Session</span>
          </div>
          <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.repo || "Untitled Repo"}</h3>
        </div>
        {timestamp && (
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground font-mono">
            <Calendar className="size-3.5" />
            {timestamp}
          </div>
        )}
      </div>
      {data.rollout_recommendation && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<ArrowRight className="size-3.5 text-muted-foreground" />}>Rollout Recommendation</HarnessSectionLabel>
          <div className="text-sm leading-relaxed text-foreground/80 pl-9">{data.rollout_recommendation}</div>
        </div>
      )}
      {data.session_id && <RefChip label="Session" value={data.session_id} />}
    </div>
  )
}

const RISK_SEVERITY_STYLES: Record<string, string> = {
  low: "bg-blue-500 text-white border-blue-600",
  medium: "bg-amber-500 text-white border-amber-600",
  high: "bg-orange-500 text-white border-orange-600",
  critical: "bg-red-600 text-white border-red-700",
}

/**
 * Specialized view for 'harness_risk' entries
 */
function HarnessRiskView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3 pb-4 border-b border-border/40">
        <div className={`px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border ${RISK_SEVERITY_STYLES[data.severity] ?? RISK_SEVERITY_STYLES.medium}`}>
          {data.severity || "medium"}
        </div>
        <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Risk"}</h3>
      </div>
      {data.mitigation && (
        <div className="space-y-3">
          <HarnessSectionLabel icon={<Wrench className="size-3.5 text-emerald-600" />} tone="emerald">Mitigation</HarnessSectionLabel>
          <div className="text-sm leading-relaxed text-foreground/80 pl-9">{data.mitigation}</div>
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_compliance' entries
 */
function HarnessComplianceView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3 pb-4 border-b border-border/40">
        <div className="flex items-center gap-1.5 px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border bg-violet-50 text-violet-700 border-violet-200 dark:bg-violet-900/30 dark:text-violet-400 dark:border-violet-800">
          <Scale className="size-3" />
          {data.framework || "Compliance"}
        </div>
        <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Concern"}</h3>
      </div>
      {data.requirement && (
        <div className="text-sm leading-relaxed text-foreground/80">{data.requirement}</div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_resource' entries
 */
function HarnessResourceView({ data }: { data: any }) {
  const provisioned = !!data.provisioned
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4 pb-4 border-b border-border/40">
        <div className="flex items-center gap-3">
          <div className="size-10 rounded-xl bg-muted flex items-center justify-center border border-border/60">
            <Boxes className="size-5 text-muted-foreground" />
          </div>
          <div>
            <span className="font-mono text-[10px] font-black uppercase tracking-[2px] text-muted-foreground">{data.kind || "Resource"}</span>
            <h3 className="text-lg font-bold tracking-tight text-foreground/90">{data.title || "Untitled Resource"}</h3>
          </div>
        </div>
        <div className={`flex items-center gap-1.5 px-2.5 py-0.5 rounded-full border text-xs font-semibold ${provisioned ? "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" : "bg-muted text-muted-foreground border-border"}`}>
          {provisioned ? <CheckCircle2 className="size-3.5" /> : <MinusCircle className="size-3.5" />}
          {provisioned ? "Provisioned" : "Pending"}
        </div>
      </div>
    </div>
  )
}

/**
 * Specialized view for 'harness_scaling' entries
 */
function HarnessScalingView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex items-center gap-2 text-primary/60 pb-2">
        <Zap className="size-4" />
        <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">Scaling Concern</span>
      </div>
      <h3 className="text-xl font-bold tracking-tight text-foreground/90 pb-2">{data.title || "Untitled Concern"}</h3>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        {data.bottleneck && (
          <div className="space-y-1 border-l-2 border-amber-500/30 pl-4 py-0.5">
            <div className="font-mono text-[10px] font-black uppercase tracking-[2px] text-amber-600">Bottleneck</div>
            <div className="text-sm text-foreground/80">{data.bottleneck}</div>
          </div>
        )}
        {data.measured_metric && (
          <div className="space-y-1 border-l-2 border-primary/20 pl-4 py-0.5">
            <div className="font-mono text-[10px] font-black uppercase tracking-[2px] text-primary/70">Measured Metric</div>
            <div className="text-sm text-foreground/80">{data.measured_metric}</div>
          </div>
        )}
      </div>
    </div>
  )
}

const VALIDATION_STATE_STYLES: Record<string, { icon: ReactNode; classes: string }> = {
  needed: { icon: <Circle className="size-3.5" />, classes: "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800" },
  done: { icon: <CheckCircle2 className="size-3.5" />, classes: "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800" },
}

/**
 * Specialized view for 'harness_validation' entries
 */
function HarnessValidationView({ data }: { data: any }) {
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4 pb-4 border-b border-border/40">
        <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Validation"}</h3>
        <StatePill state={data.status} styles={VALIDATION_STATE_STYLES} />
      </div>
      {data.how && (
        <div className="space-y-1.5">
          <div className="font-mono text-[10px] font-black uppercase tracking-[2px] text-muted-foreground">How</div>
          <div className="text-sm text-foreground/80 leading-relaxed">{data.how}</div>
        </div>
      )}
      {data.result && (
        <div className="space-y-1.5">
          <div className="font-mono text-[10px] font-black uppercase tracking-[2px] text-emerald-600">Result</div>
          <div className="text-sm text-foreground/80 leading-relaxed">{data.result}</div>
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_rollout' entries
 */
function HarnessRolloutView({ data }: { data: any }) {
  const phases = Array.isArray(data.phases) ? data.phases : []
  const prerequisites = Array.isArray(data.prerequisites) ? data.prerequisites : []
  return (
    <div className="space-y-8">
      <div className="flex items-center gap-2 text-primary/60 pb-2">
        <Layers className="size-4" />
        <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">Rollout Plan</span>
      </div>
      <h3 className="text-xl font-bold tracking-tight text-foreground/90">{data.title || "Untitled Rollout"}</h3>

      {phases.length > 0 && (
        <div className="pl-1 space-y-6">
          {phases.map((p: any, i: number) => (
            <div key={i} className="flex gap-4 group">
              <div className="shrink-0 flex flex-col items-center gap-2">
                <div className="size-7 rounded flex items-center justify-center bg-muted/40 border border-border/60 text-muted-foreground font-mono text-xs group-hover:border-primary/50 group-hover:text-primary transition-all">
                  {i + 1}
                </div>
                {i < phases.length - 1 && <div className="w-px flex-1 bg-gradient-to-b from-border/60 to-transparent" />}
              </div>
              <div className="pb-6 space-y-1.5">
                <div className="font-semibold text-sm text-foreground/90">{p.name}</div>
                {p.description && <div className="text-sm text-muted-foreground leading-relaxed">{p.description}</div>}
                {p.gate && (
                  <div className="flex items-center gap-1.5 text-xs text-primary/80 font-mono">
                    <ShieldCheck className="size-3" />
                    Gate: {p.gate}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {prerequisites.length > 0 && (
        <div className="space-y-3 pt-4 border-t border-border/20">
          <span className="text-[10px] font-mono font-black uppercase tracking-wider text-muted-foreground/80">Prerequisites</span>
          <div className="flex flex-wrap gap-2">
            {prerequisites.map((p: string, i: number) => (
              <Badge key={i} variant="secondary" className="rounded-sm font-mono text-[10px] px-2 py-0.5 bg-muted/40">
                {p}
              </Badge>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/**
 * Specialized view for 'harness_evidence' entries (free-form session evidence)
 */
function HarnessEvidenceView({ data }: { data: any }) {
  const statement = typeof data.statement === "string" ? data.statement : null
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 text-primary/60">
        <StickyNote className="size-4" />
        <span className="font-mono text-[10px] font-black uppercase tracking-[2px]">Session Evidence</span>
      </div>
      {statement ? (
        <div className="relative pl-4 border-l-2 border-muted text-[15px] leading-relaxed text-foreground/80 italic">
          {statement}
        </div>
      ) : (
        <GenericDataView data={data} />
      )}
    </div>
  )
}

function harnessTypeLabel(typeName: string): string {
  return typeName.replace(/^harness_/, "").replace(/_/g, " ").toUpperCase()
}

/** One row in the repo's "related nodes" list — links off to the node's own entry page. */
function RelatedNodeRow({ node }: { node: HarnessTreeNode }) {
  return (
    <Link
      href={`/entries/${encodeURIComponent(node.id)}`}
      className="group flex items-center gap-2 rounded-md px-2 py-1.5 -mx-2 hover:bg-primary/5 transition-colors"
    >
      <span className="shrink-0 font-mono text-[9px] font-black uppercase tracking-widest text-muted-foreground/60 group-hover:text-primary/70 w-20">
        {harnessTypeLabel(node.type_name)}
      </span>
      <span className="truncate text-sm text-foreground/85 group-hover:text-primary">
        {node.title}
      </span>
      {node.state && (
        <span className="ml-auto shrink-0 font-mono text-[9px] uppercase tracking-wider text-muted-foreground/70">
          {node.state}
        </span>
      )}
    </Link>
  )
}

/**
 * Specialized view for 'harness_repo' entries (governing repository root node).
 *
 * Lists every other harness node associated with this repo — via a direct
 * graph edge (HAD_POC, etc.) or via a matching `data.repo` field (plan/sprint
 * reference the repo by id or name without necessarily being edge-linked) —
 * so the repo page works as a navigation hub even for unlinked nodes.
 */
function HarnessRepoView({ data, entryId }: { data: any; entryId?: string }) {
  const { data: tree, isLoading } = useHarnessTree()

  const related = useMemo(() => {
    if (!tree || !entryId) return []
    const repoKeys = new Set(
      [entryId, typeof data.name === "string" ? data.name : undefined].filter(
        (v): v is string => !!v
      )
    )
    const edgeConnected = new Set<string>()
    for (const e of tree.edges) {
      if (e.from_item_id === entryId) edgeConnected.add(e.to_item_id)
      if (e.to_item_id === entryId) edgeConnected.add(e.from_item_id)
    }
    return tree.nodes.filter((n) => {
      if (n.id === entryId) return false
      if (edgeConnected.has(n.id)) return true
      const repoField = (n.data as any)?.repo
      return typeof repoField === "string" && repoKeys.has(repoField)
    })
  }, [tree, entryId, data.name])

  const grouped = useMemo(() => {
    const groups = new Map<string, HarnessTreeNode[]>()
    for (const n of related) {
      const list = groups.get(n.type_name) ?? []
      list.push(n)
      groups.set(n.type_name, list)
    }
    return [...groups.entries()].sort(([a], [b]) => a.localeCompare(b))
  }, [related])

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-center justify-between gap-4 pb-4 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="size-12 rounded-2xl bg-primary/10 flex items-center justify-center border border-primary/20">
            <FolderGit2 className="size-6 text-primary" />
          </div>
          <div>
            <div className="flex items-center gap-2 text-primary/60">
              <span className="font-mono text-[10px] font-black uppercase tracking-[3px]">Repository</span>
            </div>
            <h3 className="text-2xl font-bold tracking-tight text-foreground/90">{data.name || "Untitled Repo"}</h3>
          </div>
        </div>
        {data.default_branch && (
          <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-muted/40 border border-border/40 font-mono text-xs text-muted-foreground">
            <GitBranch className="size-3.5" />
            {data.default_branch}
          </div>
        )}
      </div>

      {/* Any other scalar/array fields, rendered generically */}
      {(() => {
        const rest = Object.entries(data).filter(([k]) => k !== "name" && k !== "default_branch")
        if (rest.length === 0) return null
        return (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-x-8 gap-y-4">
            {rest.map(([key, val]) => (
              <div key={key} className="space-y-1 border-l-2 border-primary/20 pl-4 py-0.5">
                <div className="font-mono text-[10px] font-black uppercase tracking-[2px] text-primary/70">
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
                  ) : typeof val === "object" && val !== null ? (
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
      })()}

      {/* Every other harness node tied to this repo, edge-connected or not */}
      <div className="space-y-3 pt-2">
        <HarnessSectionLabel icon={<Network className="size-3.5 text-muted-foreground" />}>
          Related Nodes {related.length > 0 ? `(${related.length})` : ""}
        </HarnessSectionLabel>
        <div className="pl-9">
          {isLoading ? (
            <div className="text-xs text-muted-foreground">loading graph…</div>
          ) : !entryId ? (
            <div className="text-xs text-muted-foreground">Save this entry to see linked nodes.</div>
          ) : grouped.length === 0 ? (
            <div className="text-xs text-muted-foreground">No other harness nodes reference this repo yet.</div>
          ) : (
            <div className="space-y-5">
              {grouped.map(([typeName, nodes]) => (
                <div key={typeName} className="space-y-1">
                  <div className="font-mono text-[9px] font-black uppercase tracking-[2px] text-muted-foreground/50">
                    {harnessTypeLabel(typeName)} ({nodes.length})
                  </div>
                  <div>
                    {nodes.map((n) => (
                      <RelatedNodeRow key={n.id} node={n} />
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
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

function CmsTemplateView({ type, data }: { type: string; data: any }) {
  const fields = Object.entries(data ?? {})
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/30 pb-4">
        <div>
          <p className="font-mono text-[10px] font-black uppercase tracking-[3px] text-primary/70">
            CMS Template
          </p>
          <h3 className="text-xl font-bold tracking-tight">{type}</h3>
        </div>
        <Badge variant="outline" className="font-mono text-[10px] uppercase">
          Rendered via /cms/&lt;id&gt;
        </Badge>
      </div>
      {fields.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-2">
          {fields.map(([key, value]) => (
            <div key={key} className="rounded-xl border border-border/50 bg-background/60 p-4">
              <div className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                {key}
              </div>
              <div className="mt-2 text-sm text-foreground/85 break-words">
                {typeof value === "string" ? value : JSON.stringify(value)}
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="rounded-xl border border-dashed border-border/60 p-4 text-sm text-muted-foreground">
          This component is driven by its sorted child edges.
        </div>
      )}
    </div>
  )
}

export function StructuredDataView({ type, data, entryId }: StructuredDataViewProps) {
  if (!data) return null

  return (
    <div className="relative group overflow-hidden rounded-xl border border-border bg-card/50 dark:bg-black/40 backdrop-blur-md p-6 shadow-sm dark:shadow-[0_0_30px_rgba(var(--primary-rgb),0.02)]">
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
        ) : type === "incident" ? (
          <IncidentView data={data} />
        ) : type === "fact" ? (
          <FactView data={data} />
        ) : type === "workout" ? (
          <WorkoutView data={data} />
        ) : type === "harness_doc" ? (
          <HarnessDocView data={data} />
        ) : type === "harness_repo" ? (
          <HarnessRepoView data={data} entryId={entryId} />
        ) : type === "harness_plan" ? (
          <HarnessPlanView data={data} />
        ) : type === "harness_sprint" ? (
          <HarnessSprintView data={data} />
        ) : type === "harness_todo" ? (
          <HarnessTodoView data={data} />
        ) : type === "harness_agent" ? (
          <HarnessAgentView data={data} />
        ) : type === "harness_stream" ? (
          <HarnessStreamView data={data} />
        ) : type === "harness_audit" ? (
          <HarnessAuditView data={data} />
        ) : type === "harness_poc" ? (
          <HarnessPocView data={data} />
        ) : type === "harness_risk" ? (
          <HarnessRiskView data={data} />
        ) : type === "harness_compliance" ? (
          <HarnessComplianceView data={data} />
        ) : type === "harness_resource" ? (
          <HarnessResourceView data={data} />
        ) : type === "harness_scaling" ? (
          <HarnessScalingView data={data} />
        ) : type === "harness_validation" ? (
          <HarnessValidationView data={data} />
        ) : type === "harness_rollout" ? (
          <HarnessRolloutView data={data} />
        ) : type === "harness_evidence" ? (
          <HarnessEvidenceView data={data} />
        ) : type.startsWith("cms_") ? (
          <CmsTemplateView type={type} data={data} />
        ) : (
          <GenericDataView data={data} />
        )}
      </div>
    </div>
  )
}
