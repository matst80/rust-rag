import { AppHeader } from "@/components/app-header"
import { NewEntryClient } from "@/components/entries/new-entry-client"

export const metadata = {
  title: "New Entry | RAG Memory & Knowledge",
  description: "Create a new knowledge base entry",
}

interface PageProps {
  searchParams: Promise<{ tab?: string }>
}

export default async function NewEntryPage({ searchParams }: PageProps) {
  const resolvedSearchParams = await searchParams
  const defaultTab = resolvedSearchParams.tab || "manual"

  return (
    <>
      <AppHeader />
      <main>
        <NewEntryClient defaultTab={defaultTab} />
      </main>
    </>
  )
}
