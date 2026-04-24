"use client"

import { useState, useEffect } from "react"
import { Wand2, ChevronDown } from "lucide-react"
import { Textarea } from "@/components/ui/textarea"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useCategories } from "@/lib/api"
import { cn } from "@/lib/utils"

interface SearchInputProps {
  query: string
  onQueryChange: (query: string) => void
  categoryFilter: string | null
  onCategoryFilterChange: (category: string | null) => void
  onSubmit: () => void
  isLoading?: boolean
}

const SUGGESTIONS = [
  "Summarize my latest entries",
  "Find connections between rust and rag",
  "Recent research notes",
]

export function SearchInput({
  query,
  onQueryChange,
  categoryFilter,
  onCategoryFilterChange,
  onSubmit,
  isLoading,
}: SearchInputProps) {
  const [mounted, setMounted] = useState(false)
  useEffect(() => setMounted(true), [])

  const { data: categories } = useCategories()
  const validCategories = categories?.filter((c) => c.id.trim().length > 0)

  const handleSubmit = (e?: React.FormEvent) => {
    e?.preventDefault()
    if (query.trim()) onSubmit()
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      handleSubmit()
    }
  }

  return (
    <div className="w-full max-w-3xl mx-auto">
      <div
        className={cn(
          "relative flex flex-col w-full border border-border bg-card transition-all duration-200",
          "focus-within:border-primary focus-within:[box-shadow:0_0_0_1px_oklch(0.9_0.148_196.3/0.15),inset_0_0_30px_oklch(0.9_0.148_196.3/0.03)]"
        )}
      >
        {/* Textarea */}
        <div className="flex-1 px-4 pt-4 pb-2">
          <Textarea
            placeholder="Search your knowledge base..."
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            onKeyDown={onKeyDown}
            className="w-full min-h-13 max-h-65 border-none bg-transparent! p-0 text-base font-medium focus-visible:ring-0 resize-none placeholder:text-muted-foreground/60 shadow-none ring-0!"
            rows={1}
          />
        </div>

        {/* Controls */}
        <div className="flex items-center justify-between px-3 pb-3 gap-2">
          <div className="flex items-center gap-1">
            <Select
              value={categoryFilter ?? "all"}
              onValueChange={(v) => onCategoryFilterChange(v === "all" ? null : v)}
            >
              <SelectTrigger className="h-8 border border-border bg-transparent hover:border-primary/40 px-2 font-mono text-[10px] uppercase tracking-[1.5px] text-muted-foreground transition-colors focus:ring-0 focus:ring-offset-0 gap-1">
                <span className="opacity-50 mr-0.5">Focus:</span>
                <SelectValue placeholder="All" />
                <ChevronDown className="size-3 opacity-50" />
              </SelectTrigger>
              <SelectContent className="font-mono text-[11px]">
                <SelectItem value="all">All Memory</SelectItem>
                {validCategories?.map((c) => (
                  <SelectItem key={c.id} value={c.id}>
                    {c.name}
                    <span className="opacity-40 ml-1.5">({c.count})</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <Button
            onClick={() => handleSubmit()}
            disabled={isLoading || !query.trim()}
            className={cn(
              "h-8 px-4 font-mono text-[10px] font-black uppercase tracking-[2px] transition-all",
              query.trim()
                ? "bg-primary text-primary-foreground hover:bg-primary/90 shadow-[0_0_14px_oklch(0.9_0.148_196.3/0.3)]"
                : "bg-muted text-muted-foreground/30 cursor-not-allowed"
            )}
          >
            {isLoading ? (
              <div className="size-3.5 animate-spin border border-current border-t-transparent" />
            ) : (
              <>
                <Wand2 className="size-3.5 mr-1.5" />
                Search
              </>
            )}
          </Button>
        </div>
      </div>

      {/* Suggestions */}
      {!query && mounted && (
        <div className="flex flex-wrap justify-center gap-2 mt-5 animate-in fade-in slide-in-from-top-2 duration-500 fill-mode-both">
          {SUGGESTIONS.map((s) => (
            <button
              key={s}
              onClick={() => { onQueryChange(s); onSubmit() }}
              className="px-3 py-1.5 border border-border bg-card font-mono text-[10px] uppercase tracking-[1px] text-muted-foreground transition-all hover:border-primary/50 hover:text-primary"
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
