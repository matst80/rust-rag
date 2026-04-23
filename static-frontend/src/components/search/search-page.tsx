"use client"

import { useState, useCallback } from "react"
import { Brain } from "lucide-react"
import { SearchInput } from "./search-input"
import { SearchResults } from "./search-results"
import { useSearch } from "@/lib/api"

export function SearchPage() {
  const [searchQuery, setSearchQuery] = useState("")
  const [submittedQuery, setSubmittedQuery] = useState("")
  const [categoryFilter, setCategoryFilter] = useState<string | null>(null)

  const { data: results, isLoading } = useSearch(
    submittedQuery,
    categoryFilter ?? undefined
  )

  const handleSubmit = useCallback(() => {
    setSubmittedQuery(searchQuery.trim())
  }, [searchQuery])

  return (
    <div className="relative flex w-full min-h-[calc(100vh-3rem)] flex-col overflow-hidden">
      <div className="mx-auto w-full max-w-3xl flex-1 flex flex-col px-6">
        {!submittedQuery ? (
          <div className="flex flex-1 flex-col items-center justify-center -mt-16">

            <div className="animate-in fade-in zoom-in duration-700 fill-mode-both mb-8">
              <Brain
                className="size-16 text-primary"
                style={{ filter: "drop-shadow(0 0 20px oklch(0.9 0.148 196.3 / 0.5))" }}
              />
            </div>

            <p className="mb-2 font-mono text-[10px] font-black uppercase tracking-[5px] text-primary animate-in fade-in duration-500 delay-100 fill-mode-both">
              rust-rag
            </p>

            <h1 className="mb-4 text-center text-4xl md:text-5xl font-extrabold tracking-tight text-foreground animate-in fade-in slide-in-from-bottom-4 duration-700 delay-200 fill-mode-both">
              Search Intelligence
            </h1>

            <p className="mb-12 text-center text-muted-foreground text-base max-w-md animate-in fade-in slide-in-from-bottom-4 duration-700 delay-300 fill-mode-both leading-relaxed">
              Explore your knowledge base with semantic precision.
            </p>

            <div className="w-full animate-in fade-in slide-in-from-bottom-8 duration-700 delay-500 fill-mode-both">
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
          <div className="flex flex-1 flex-col gap-10 py-8 animate-in fade-in slide-in-from-bottom-4 duration-500 fill-mode-both">
            <div className="sticky top-12 z-40 pb-6 pt-2 -mx-6 px-6 border-b border-border bg-background/95 backdrop-blur">
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
                <div className="flex flex-col items-center justify-center py-24 gap-4">
                  <div className="size-10 animate-spin border-2 border-border border-t-primary" />
                  <p className="font-mono text-[10px] font-black uppercase tracking-[4px] text-muted-foreground animate-pulse">
                    Scanning...
                  </p>
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
