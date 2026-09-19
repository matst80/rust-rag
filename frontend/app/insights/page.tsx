import { Suspense } from "react"
import { InsightsView } from "@/components/insights/insights-view";

export default function InsightsPage() {
  return (
    <div className="mx-auto flex h-full max-w-350 flex-col gap-3 p-4">
      <header className="flex flex-col gap-1">
        <h1 className="text-xl font-bold font-mono uppercase tracking-[3px]">
          Graph Insights
        </h1>
        <p className="text-xs text-muted-foreground font-mono uppercase tracking-widest">
          Cross-POC aggregation over the harness graph
        </p>
      </header>
      <div className="min-h-0 flex-1">
        <Suspense>
          <InsightsView />
        </Suspense>
      </div>
    </div>
  );
}
