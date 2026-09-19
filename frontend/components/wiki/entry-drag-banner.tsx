"use client"

import { FileText } from "lucide-react"
import { useDraggingEntry } from "@/lib/drag-entry"

/** Fixed pill shown while an entry is being dragged, so the user always
 *  knows what is in flight and what to do with it. */
export function EntryDragBanner() {
  const drag = useDraggingEntry()
  if (!drag) return null

  return (
    <div className="pointer-events-none fixed left-1/2 top-14 z-50 -translate-x-1/2 animate-in fade-in slide-in-from-top-2">
      <div className="flex items-center gap-2 rounded-full border border-primary/40 bg-background/95 px-4 py-1.5 shadow-lg backdrop-blur">
        <FileText className="size-4 shrink-0 text-primary" />
        <span className="max-w-[24rem] truncate text-sm font-medium">
          Moving “{drag.title}”
        </span>
        <span className="hidden text-xs text-muted-foreground sm:inline">
          — drop on a folder to file it
        </span>
      </div>
    </div>
  )
}
