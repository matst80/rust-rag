"use client"

import { useState, useCallback, useEffect } from "react"
import { Brain } from "lucide-react"
import { SearchInput } from "./search-input"
import { SearchResults } from "./search-results"
import { useSearch } from "@/lib/api"

export function SearchPage() {
  const [mounted, setMounted] = useState(false)
  const [searchQuery, setSearchQuery] = useState("")
  const [submittedQuery, setSubmittedQuery] = useState("")
  const [categoryFilter, setCategoryFilter] = useState<string | null>(null)

  useEffect(() => {
    setMounted(true)
  }, [])

  const { data: results, isLoading } = useSearch(
    submittedQuery,
    categoryFilter ?? undefined
  )

  const handleSubmit = useCallback(() => {
    setSubmittedQuery(searchQuery.trim())
  }, [searchQuery])

  if (!mounted) {
    return (
      <div className="relative flex min-h-[calc(100vh-3.5rem)] flex-col overflow-hidden opacity-0">
        <div className="mx-auto flex w-full max-w-4xl flex-1 flex-col px-6" />
      </div>
    )
  }

  return (
    <div className="relative flex min-h-[calc(100vh-3.5rem)] flex-col overflow-hidden">
      {/* Background radial gradient for depth */}
      <div className="absolute inset-x-0 top-0 -z-10 h-[500px] bg-gradient-to-b from-primary/5 to-transparent" />

      <div className="mx-auto flex w-full max-w-3xl flex-1 flex-col px-6">
        {!submittedQuery ? (
          <div className="flex flex-1 flex-col items-center justify-center -mt-20">
            <div className="animate-in fade-in zoom-in duration-1000 fill-mode-both">
              <Brain className="mb-8 size-20 text-primary opacity-80" />
            </div>

            <h1 className="mb-4 text-center text-4xl md:text-5xl font-extrabold tracking-tight bg-gradient-to-b from-foreground to-foreground/60 bg-clip-text text-transparent animate-in fade-in slide-in-from-bottom-4 duration-700 delay-200 fill-mode-both">
              Search Intelligence
            </h1>

            <p className="mb-12 text-center text-muted-foreground text-lg max-w-xl animate-in fade-in slide-in-from-bottom-4 duration-700 delay-300 fill-mode-both leading-relaxed">
              Explore your neural network of memories and research notes with semantic precision.
            </p>

            <div className="w-full animate-in fade-in slide-in-from-bottom-8 duration-1000 delay-500 fill-mode-both">
              <SearchInput
                query={searchQuery}
                onQueryChange={setSearchQuery}
                categoryFilter={categoryFilter}
                onCategoryFilterChange={setCategoryFilter}
                onSubmit={handleSubmit}
                isLoading={isLoading}
              />
            </div>
          </div>
        ) : (
          <div className="flex flex-1 flex-col gap-12 py-12 animate-in fade-in slide-in-from-bottom-4 duration-500 fill-mode-both">
            <div className="sticky top-[4.5rem] z-40 pb-8 pt-2 -mx-6 px-6 border-b border-muted/10 shadow-[0_15px_30px_-15px_rgba(0,0,0,0.1)] transition-shadow">
              <SearchInput
                query={searchQuery}
                onQueryChange={setSearchQuery}
                categoryFilter={categoryFilter}
                onCategoryFilterChange={setCategoryFilter}
                onSubmit={handleSubmit}
                isLoading={isLoading}
              />
            </div>

            <div className="flex-1 w-full">
              {isLoading ? (
                <div className="flex flex-col items-center justify-center py-24 gap-6">
                  <div className="size-14 animate-spin rounded-full border-[3px] border-muted border-t-primary shadow-2xl" />
                  <p className="text-xs font-black uppercase tracking-[0.3em] text-muted-foreground animate-pulse">Scanning Intelligence Basin...</p>
                </div>
              ) : results ? (
                <SearchResults
                  results={results.results}
                  related={results.related}
                  query={submittedQuery}
                />
              ) : null}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
