"use client"

import { useEffect, useState } from "react"
import { CategorySidebar } from "./category-sidebar"
import { EntriesList } from "./entries-list"
import { useSessionState } from "@/hooks/use-session-state"

export function EntriesBrowser() {
  const [mounted, setMounted] = useState(false)
  useEffect(() => setMounted(true), [])

  const [selectedCategory, setSelectedCategory] = useSessionState<string | null>(
    "entries-browser:category",
    null
  )

  if (!mounted) {
    return (
      <div className="relative flex min-h-[calc(100vh-3.5rem)] flex-col bg-background md:flex-row overflow-hidden" />
    )
  }

  return (
    <div className="relative flex min-h-[calc(100vh-3.5rem)] flex-col bg-background md:flex-row overflow-hidden">
      {/* Background decoration */}
      <div className="absolute inset-x-0 bottom-0 -z-10 h-[500px] bg-gradient-to-t from-primary/5 to-transparent pointer-events-none" />
      
      <CategorySidebar
        selectedCategory={selectedCategory}
        onSelectCategory={setSelectedCategory}
      />
      <div className="flex-1 overflow-y-auto">
        <EntriesList selectedCategory={selectedCategory} />
      </div>
    </div>
  )
}
