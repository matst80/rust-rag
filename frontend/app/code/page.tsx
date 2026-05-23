import { AppHeader } from "@/components/app-header"
import { CodeBrowser } from "@/components/code/code-browser"

export const metadata = {
  title: "Code Search | RAG",
  description: "Browse source-code repos indexed by rust-rag",
}

export default function CodePage() {
  return (
    <>
      <AppHeader />
      <main className="mx-auto max-w-7xl p-6">
        <CodeBrowser />
      </main>
    </>
  )
}
