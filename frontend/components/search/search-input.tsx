"use client"

import { useState, useEffect } from "react"
import { Search, X, Mic, Plus, ChevronDown, Wand2 } from "lucide-react"
import { Textarea } from "@/components/ui/textarea"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Badge } from "@/components/ui/badge"
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

export function SearchInput({
  query,
  onQueryChange,
  categoryFilter,
  onCategoryFilterChange,
  onSubmit,
  isLoading,
}: SearchInputProps) {
  const [mounted, setMounted] = useState(false)
  useEffect(() => {
    setMounted(true)
  }, [])

  const { data: categories } = useCategories()
  const validCategories = categories?.filter(
    (category) => category.id.trim().length > 0
  )

  const handleSubmit = (e?: React.FormEvent) => {
    e?.preventDefault()
    if (query.trim()) {
      onSubmit()
    }
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      handleSubmit()
    }
  }

  return (
    <div className="w-full max-w-3xl mx-auto group">
      <div className={cn(
        "relative flex flex-col w-full rounded-[24px] border border-muted-foreground/20 bg-muted/30 backdrop-blur-xl transition-all duration-300",
        "focus-within:border-primary/40 focus-within:ring-4 focus-within:ring-primary/5 focus-within:bg-background shadow-lg",
        query && "shadow-primary/10"
      )}>
        {/* Input Area */}
        <div className="flex-1 px-5 pt-5 pb-2">
          <Textarea
            placeholder="Search your knowledge base..."
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            onKeyDown={onKeyDown}
            className="w-full min-h-[60px] max-h-[300px] border-none !bg-transparent p-0 text-lg md:text-xl font-medium focus-visible:ring-0 resize-none placeholder:text-muted-foreground/50 border-0 shadow-none !ring-0"
            rows={1}
          />
        </div>

        {/* Bottom Controls Area */}
        <div className="flex items-center justify-between px-4 pb-4 gap-2">
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="icon" className="size-9 rounded-full text-muted-foreground hover:bg-muted duration-200">
              <Plus className="size-4" />
            </Button>
            
            <Select
              value={categoryFilter ?? "all"}
              onValueChange={(value) =>
                onCategoryFilterChange(value === "all" ? null : value)
              }
            >
              <SelectTrigger className="h-9 border-none bg-transparent hover:bg-muted px-3 rounded-full text-xs font-semibold text-muted-foreground transition-all gap-1.5 focus:ring-0 focus:ring-offset-0">
                <div className="flex items-center gap-1.5">
                  <span className="opacity-70">Focus:</span>
                  <SelectValue placeholder="All Memory" />
                </div>
              </SelectTrigger>
              <SelectContent className="rounded-2xl border-muted-foreground/10 shadow-2xl">
                <SelectItem value="all" className="rounded-lg">All Memory</SelectItem>
                {validCategories?.map((category) => (
                  <SelectItem key={category.id} value={category.id} className="rounded-lg">
                    {category.name} <span className="text-[10px] opacity-50 ml-1">({category.count})</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Button variant="ghost" size="sm" className="h-9 rounded-full px-3 text-xs font-bold text-muted-foreground hover:bg-muted hidden sm:flex">
              Pro <ChevronDown className="size-3 ml-1" />
            </Button>
          </div>

          <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon" className="size-9 rounded-full text-muted-foreground hover:bg-muted duration-200">
              <Mic className="size-4" />
            </Button>
            <Button 
              onClick={() => handleSubmit()} 
              disabled={isLoading || !query.trim()}
              className={cn(
                "h-9 w-9 p-0 rounded-full transition-all duration-300",
                query.trim() ? "bg-primary text-primary-foreground shadow-md shadow-primary/20 scale-100" : "bg-muted text-muted-foreground/30 scale-95 opacity-50"
              )}
            >
              {isLoading ? (
                <div className="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
              ) : (
                <Wand2 className="size-4" />
              )}
            </Button>
          </div>
        </div>
      </div>
      
      {/* Suggestions/Hints */}
      {!query && mounted && (
        <div className="flex flex-wrap justify-center gap-2 mt-6 animate-in fade-in slide-in-from-top-4 duration-700 fill-mode-both">
          <Badge variant="secondary" className="px-3 py-1 cursor-pointer hover:bg-muted transition-colors rounded-full text-[11px] font-semibold tracking-wide">
            "Summarize my latest entries"
          </Badge>
          <Badge variant="secondary" className="px-3 py-1 cursor-pointer hover:bg-muted transition-colors rounded-full text-[11px] font-semibold tracking-wide">
            "Find connection between rust and rag"
          </Badge>
          <Badge variant="secondary" className="px-3 py-1 cursor-pointer hover:bg-muted transition-colors rounded-full text-[11px] font-semibold tracking-wide">
            "Recent research notes"
          </Badge>
        </div>
      )}
    </div>
  )
}
