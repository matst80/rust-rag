"use client"

import { use } from "react"
import { AppHeader } from "@/components/app-header"
import { EntryForm } from "@/components/entries/entry-form"
import { useItem } from "@/lib/api"

export default function EditEntryPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = use(params)
  const decodedId = decodeURIComponent(id)
  const { data: entry, isLoading } = useItem(decodedId)

  if (isLoading) {
    return (
      <>
        <AppHeader />
        <main>
          <div className="flex min-h-[calc(100vh-3.5rem)] items-center justify-center">
            <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
          </div>
        </main>
      </>
    )
  }

  return (
    <>
      <AppHeader />
      <main>
        <EntryForm entry={entry ?? undefined} mode="edit" />
      </main>
    </>
  )
}
