"use client"

import { useState } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import { Loader2 } from "lucide-react"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useHarnessTree } from "@/lib/api"
import type { HarnessTreeResponse } from "@/lib/api"
import { useHarnessIndex } from "@/components/insights/use-harness-index"
import { RiskDashboard } from "@/components/insights/risk-dashboard"
import { DecisionTimeline } from "@/components/insights/decision-timeline"
import { GraphExplore } from "@/components/insights/graph-explore"
import { cn } from "@/lib/utils"

function RepoSelector({
  tree,
  repoId,
  onPick,
}: {
  tree: HarnessTreeResponse | null
  repoId: string | null
  onPick: (id: string | null) => void
}) {
  const repos = (tree?.nodes ?? []).filter((n) => n.type_name === "harness_repo")
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <button
        type="button"
        onClick={() => onPick(null)}
        className={cn(
          "rounded-sm border px-2 py-1 font-mono text-[10px] uppercase tracking-wider",
          repoId === null
            ? "border-primary/50 bg-primary/10 text-primary"
            : "border-border text-muted-foreground hover:text-foreground"
        )}
      >
        All repos
      </button>
      {repos.map((repo) => (
        <button
          key={repo.id}
          type="button"
          onClick={() => onPick(repo.id === repoId ? null : repo.id)}
          className={cn(
            "rounded-sm border px-2 py-1 font-mono text-[10px] uppercase tracking-wider",
            repoId === repo.id
              ? "border-primary/50 bg-primary/10 text-primary"
              : "border-border text-muted-foreground hover:text-foreground"
          )}
        >
          {repo.title}
        </button>
      ))}
    </div>
  )
}

export function InsightsView() {
  const router = useRouter()
  const searchParams = useSearchParams()
  // ?repo=<id> is the deep-link; clicking chips updates local state only so
  // switching views doesn't scroll or rewrite the URL.
  const [repoOverride, setRepoOverride] = useState<string | null | undefined>(undefined)
  const repoId = repoOverride ?? searchParams.get("repo")

  const { data: tree, isLoading, error } = useHarnessTree()
  const safeTree: HarnessTreeResponse | null = tree ?? null
  const index = useHarnessIndex(safeTree)

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center gap-2 font-mono text-xs text-muted-foreground">
        <Loader2 className="h-3 w-3 animate-spin" /> loading harness graph…
      </div>
    )
  }
  if (error) {
    return (
      <div className="flex h-full items-center justify-center font-mono text-xs text-red-500">
        failed to load harness graph
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col gap-4">
      <RepoSelector tree={safeTree} repoId={repoId} onPick={setRepoOverride} />
      <Tabs defaultValue="risks" className="flex flex-1 flex-col">
        <TabsList className="w-fit">
          <TabsTrigger value="risks" className="font-mono text-xs">
            Risk dashboard
          </TabsTrigger>
          <TabsTrigger value="decisions" className="font-mono text-xs">
            Decision timeline
          </TabsTrigger>
          <TabsTrigger value="explore" className="font-mono text-xs">
            Explore
          </TabsTrigger>
        </TabsList>
        <TabsContent value="risks" className="flex-1 overflow-y-auto">
          <RiskDashboard index={index} repoId={repoId} onSelect={(id) => router.push(`/grill?node=${encodeURIComponent(id)}`)} />
        </TabsContent>
        <TabsContent value="decisions" className="flex-1 overflow-y-auto">
          <DecisionTimeline index={index} repoId={repoId} onSelect={(id) => router.push(`/grill?node=${encodeURIComponent(id)}`)} />
        </TabsContent>
        <TabsContent value="explore" className="flex-1 overflow-y-auto">
          <GraphExplore index={index} repoId={repoId} onSelect={(id) => router.push(`/grill?node=${encodeURIComponent(id)}`)} />
        </TabsContent>
      </Tabs>
    </div>
  )
}
