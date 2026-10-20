import { Suspense } from "react"
import { EntryDetail } from "@/components/entries/entry-detail"

export default async function EntryDetailPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = await params
  return (
    <Suspense fallback={null}>
        <EntryDetail id={decodeURIComponent(id)} />
    </Suspense>
  )
}
