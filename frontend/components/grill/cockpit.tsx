"use client"

import { useCallback } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { PlanTree } from "@/components/grill/plan-tree"
import { AuditPanel } from "@/components/grill/audit-panel"
import { ContextPin } from "@/components/grill/context-pin"
import { TimelineBar } from "@/components/grill/timeline-bar"
import { useHarnessTree } from "@/lib/api"

export function GrillCockpit() {
  const router = useRouter()
  const searchParams = useSearchParams()
  const selectedId = searchParams.get("node")

  const { data: tree, isLoading, error, mutate } = useHarnessTree()

  const onSelect = useCallback(
    (id: string) => {
      const params = new URLSearchParams(searchParams.toString())
      params.set("node", id)
      router.replace(`/grill?${params.toString()}`, { scroll: false })
    },
    [router, searchParams]
  )

  return (
    <main className="flex-1 flex flex-col min-h-0 font-mono">
      <ResizablePanelGroup direction="vertical">
        <ResizablePanel defaultSize={78} minSize={50}>
          <ResizablePanelGroup direction="horizontal">
            <ResizablePanel defaultSize={26} minSize={16}>
              <PlanTree
                tree={tree ?? null}
                isLoading={isLoading}
                error={error}
                selectedId={selectedId}
                onSelect={onSelect}
              />
            </ResizablePanel>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={46} minSize={25}>
              <AuditPanel
                tree={tree ?? null}
                selectedId={selectedId}
                onSelect={onSelect}
                onChanged={() => mutate()}
              />
            </ResizablePanel>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={28} minSize={18}>
              <ContextPin tree={tree ?? null} selectedId={selectedId} />
            </ResizablePanel>
          </ResizablePanelGroup>
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={22} minSize={10}>
          <TimelineBar
            tree={tree ?? null}
            selectedId={selectedId}
            onSelect={onSelect}
          />
        </ResizablePanel>
      </ResizablePanelGroup>
    </main>
  )
}
