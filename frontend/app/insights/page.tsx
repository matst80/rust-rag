import { Suspense } from "react"
import { InsightsView } from "@/components/insights/insights-view";
import { AppHeader } from "@/components/app-header";

export default function InsightsPage() {
  return (
    <div className="flex flex-col h-screen">
      <AppHeader />
      <main className="flex-1 p-4 min-h-0">
        <div className="mx-auto h-full max-w-350 flex flex-col gap-3">
          <header className="flex flex-col gap-1">
            <h1 className="text-xl font-bold font-mono uppercase tracking-[3px]">
              Graph Insights
            </h1>
            <p className="text-xs text-muted-foreground font-mono uppercase tracking-widest">
              Cross-POC aggregation over the harness graph
            </p>
          </header>
          <div className="flex-1 min-h-0">
            <Suspense>
              <InsightsView />
            </Suspense>
          </div>
        </div>
      </main>
    </div>
  );
}
