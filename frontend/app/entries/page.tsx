import { AppHeader } from "@/components/app-header"
import { EntriesBrowser } from "@/components/entries/entries-browser"

export const metadata = {
  title: "Entries | RAG Memory & Knowledge",
  description: "Browse and manage your knowledge base entries",
}

export default function EntriesPage() {
  return (
    <>
      <AppHeader />
      <main>
        <EntriesBrowser />
      </main>
    </>
  )
}
