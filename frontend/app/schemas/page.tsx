import { AppHeader } from "@/components/app-header"
import { SchemasBrowser } from "@/components/schemas/schemas-browser"

export const metadata = {
  title: "Schemas | RAG Memory & Knowledge",
  description: "Manage typed-entry JSON Schemas",
}

export default function SchemasPage() {
  return (
    <>
      <AppHeader />
      <main>
        <SchemasBrowser />
      </main>
    </>
  )
}
