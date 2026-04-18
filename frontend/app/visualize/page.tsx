import { Suspense } from "react"
import { AppHeader } from "@/components/app-header"
import { GraphView } from "@/components/graph/graph-view"

export const metadata = {
  title: "Visualize | RAG Memory & Knowledge",
  description: "Visualize connections between knowledge entries",
}

export default function VisualizePage() {
  return (
    <>
      <AppHeader />
      <main>
        <Suspense
          fallback={
            <div className="flex h-[calc(100vh-3.5rem)] items-center justify-center">
              <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
            </div>
          }
        >
          <GraphView />
        </Suspense>
      </main>
    </>
  )
}
