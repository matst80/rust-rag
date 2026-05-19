"use client"

import { Sparkles, Maximize2, Clock, History } from "lucide-react"
import { MarkdownView } from "./markdown-view"
import { cn, formatRelativeTime } from "@/lib/utils"
import type { Entry } from "@/lib/api"
import { EntryTagList } from "@/components/ui/entry-tag"

interface EntryPeekProps {
  entry: Entry
  className?: string
}

export function EntryPeek({ entry, className }: EntryPeekProps) {
  const analysis = entry.analysis

  return (
    <div className={cn("flex flex-col overflow-hidden", className)}>
      <div className="px-4 py-2 border-b border-border/40 bg-muted/5 flex items-center gap-4">
        <div className="flex items-center gap-1.5 font-mono text-[9px] text-muted-foreground/60 uppercase" title={`Created: ${new Date(entry.created_at).toLocaleString()}`}>
          <Clock className="size-2.5 opacity-50" />
          {formatRelativeTime(entry.created_at)}
        </div>
        {entry.updated_at > entry.created_at + 1000 && (
          <div className="flex items-center gap-1.5 font-mono text-[9px] text-muted-foreground/60 uppercase" title={`Modified: ${new Date(entry.updated_at).toLocaleString()}`}>
            <History className="size-2.5 opacity-50" />
            {formatRelativeTime(entry.updated_at)}
          </div>
        )}
      </div>
      <div className="flex-1 overflow-y-auto p-4 md:p-6 space-y-6">
        {analysis?.summary && (
          <div className="p-4 bg-primary/[0.03] border border-primary/10 rounded-lg animate-in fade-in slide-in-from-top-2 duration-500">
            <div className="flex items-center gap-2 mb-2.5">
              <Sparkles className="size-3 text-primary animate-pulse" />
              <span className="font-mono text-[9px] font-black uppercase tracking-[2px] text-primary/80">Intelligence Summary</span>
            </div>
            <p className="text-xs text-foreground/70 leading-relaxed italic selection:bg-primary/20">
              {analysis.summary}
            </p>
          </div>
        )}
        
        <div className="prose-sm">
          <MarkdownView content={entry.text} />
        </div>
      </div>

      {analysis?.tags && analysis.tags.length > 0 && (
        <div className="px-4 py-3 border-t border-border/50 bg-muted/5">
          <EntryTagList tags={analysis.tags} />
        </div>
      )}

      {Object.keys(entry.metadata).length > 0 && (
        <div className="p-4 border-t border-border bg-muted/20">
           <div className="grid grid-cols-2 md:grid-cols-3 gap-x-4 gap-y-3">
             {Object.entries(entry.metadata).map(([key, value]) => (
               <div key={key} className="flex flex-col gap-0.5 min-w-0">
                 <span className="font-mono text-[8px] uppercase tracking-widest text-muted-foreground/40">{key}</span>
                 <span className="font-mono text-[10px] text-foreground/70 truncate" title={String(value)}>{String(value)}</span>
               </div>
             ))}
           </div>
        </div>
      )}
    </div>
  )
}
