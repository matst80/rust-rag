import { DatabaseBrowser } from "@/components/database/database-browser"

export const metadata = {
  title: "Databases | RAG Memory & Knowledge",
  description: "Notion-style table, board, and calendar views over typed entries",
}

export default function DatabasesPage() {
  return (
    <div className="mx-auto flex min-h-[calc(100vh-3rem)] w-full max-w-5xl flex-col gap-5 p-4 md:p-8">
      <header>
        <h1 className="font-mono text-sm font-black uppercase tracking-[3px]">
          Databases
        </h1>
        <p className="mt-1 text-xs text-muted-foreground">
          Every typed schema as a Notion-style database — table, board, and calendar views over the same rows.
        </p>
      </header>
      <DatabaseBrowser />
    </div>
  )
}
