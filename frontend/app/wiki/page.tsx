"use client"

import { Suspense } from "react"
import { useSearchParams } from "next/navigation"
import { EntryTree } from "@/components/wiki/entry-tree"

function WikiInner() {
  const params = useSearchParams()
  const sourceId = params.get("source_id") || "knowledge"
  const path = params.get("path") || undefined
  const entryId = params.get("entry") || undefined
  return <EntryTree sourceId={sourceId} prefix={path} selectedEntryId={entryId} />
}

export default function WikiPage() {
  return (
    <>
        <Suspense fallback={null}>
          <WikiInner />
        </Suspense>
    </>
  )
}
