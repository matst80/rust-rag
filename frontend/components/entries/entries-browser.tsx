"use client"

import type { SmartCollection } from "@/lib/smart-collections"
import { useEffect, useState } from "react"
import { CategorySidebar } from "./category-sidebar"
import { EntriesList } from "./entries-list"
import { useSessionState } from "@/hooks/use-session-state"

export function EntriesBrowser() {
  const [collection, setCollection] = useState<SmartCollection | null>(null)
  const [inbox, setInbox] = useState(false)
  const [mounted, setMounted] = useState(false)
  useEffect(() => setMounted(true), [])

  const [selectedCategory, setSelectedCategory] = useSessionState<string | null>(
    "entries-browser:category",
    null
  )
  const [selectedPath, setSelectedPath] = useSessionState<string | null>(
    "entries-browser:path",
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
        inbox={inbox}
        onInbox={() => { setInbox(true); setCollection(null); setSelectedCategory(null); setSelectedPath(null) }}
        onCollection={(value) => { setCollection(value); setInbox(!!value.inbox); setSelectedCategory(value.category); setSelectedPath(value.path) }}
        selectedCategory={selectedCategory}
        onSelectCategory={(value) => { setInbox(false); setCollection(null); setSelectedCategory(value) }}
        selectedPath={selectedPath}
        onSelectPath={setSelectedPath}
      />
      <div className="flex-1 overflow-y-auto">
        <EntriesList
          inbox={inbox}
          collection={collection}
          onInboxChange={setInbox}
          selectedCategory={selectedCategory}
          selectedPath={selectedPath}
          onSelectCategory={(c) => {
            setSelectedCategory(c);
            setSelectedPath(null);
          }}
        />
      </div>
    </div>
  )
}
