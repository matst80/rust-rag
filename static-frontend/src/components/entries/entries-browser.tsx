import { useState, useEffect } from "react"
import { CategorySidebar } from "./category-sidebar"
import { EntriesList } from "./entries-list"
import { LargeItemsPanel } from "./large-items-panel"
import { EntryDetail } from "./entry-detail"
import { EntryForm } from "./entry-form"

export function EntriesBrowser() {
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null)
  const [view, setView] = useState<{ type: 'list' | 'detail' | 'create', id?: string }>({ type: 'list' })

  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const id = params.get('id')
    const isNew = window.location.pathname.endsWith('/new/') || params.has('new')
    
    if (isNew) {
      setView({ type: 'create' })
    } else if (id) {
      setView({ type: 'detail', id })
    } else {
      setView({ type: 'list' })
    }
  }, [])

  if (view.type === 'create') {
    return <EntryForm mode="create" />
  }

  if (view.type === 'detail' && view.id) {
    return <EntryDetail id={view.id} />
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
        <LargeItemsPanel />
        <EntriesList selectedCategory={selectedCategory} />
      </div>
    </div>
  )
}
