import { AppHeader } from "@/components/app-header"
import { EntryDetail } from "@/components/entries/entry-detail"

export default async function EntryDetailPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = await params
  return (
    <>
      <AppHeader />
      <main>
        <EntryDetail id={decodeURIComponent(id)} />
      </main>
    </>
  )
}
