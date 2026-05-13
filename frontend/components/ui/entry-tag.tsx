import React from "react"
import { Tag as TagIcon, X } from "lucide-react"
import { cn } from "@/lib/utils"

interface EntryTagProps {
  label: string
  onRemove?: () => void
  className?: string
  icon?: boolean
}

export function EntryTag({ label, onRemove, className, icon = true }: EntryTagProps) {
  return (
    <div
      className={cn(
        "flex items-center gap-1.5 px-2 py-0.5 rounded bg-background border border-border/80 shadow-sm text-[10px] font-mono uppercase tracking-tighter text-muted-foreground group/tag transition-all hover:border-primary/30",
        className
      )}
    >
      {icon && <TagIcon className="size-3 opacity-50 group-hover/tag:text-primary transition-colors" />}
      <span className="group-hover/tag:text-foreground transition-colors">{label}</span>
      {onRemove && (
        <button
          onClick={(e) => {
            e.preventDefault()
            e.stopPropagation()
            onRemove()
          }}
          className="ml-1 -mr-1 p-0.5 rounded-full hover:bg-red-500/10 hover:text-red-500 transition-colors"
        >
          <X className="size-2.5" />
        </button>
      )}
    </div>
  )
}

interface EntryTagListProps {
  tags: string[]
  onRemoveTag?: (tag: string) => void
  className?: string
  emptyText?: string
}

export function EntryTagList({ tags, onRemoveTag, className, emptyText = "none" }: EntryTagListProps) {
  if (tags.length === 0) {
    return <span className="text-[10px] text-muted-foreground font-mono uppercase tracking-widest opacity-50">{emptyText}</span>
  }

  return (
    <div className={cn("flex flex-wrap gap-1.5", className)}>
      {tags.map((tag) => (
        <EntryTag key={tag} label={tag} onRemove={onRemoveTag ? () => onRemoveTag(tag) : undefined} />
      ))}
    </div>
  )
}
