import { AppHeader } from "@/components/app-header"
import { EntryForm } from "@/components/entries/entry-form"

export const metadata = {
  title: "New Entry | RAG Memory & Knowledge",
  description: "Create a new knowledge base entry",
}

export default function NewEntryPage() {
  return (
    <>
      <AppHeader />
      <main>
        <EntryForm mode="create" />
      </main>
    </>
  )
}
