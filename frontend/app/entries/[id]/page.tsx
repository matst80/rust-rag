import { EntryDetail } from "@/components/entries/entry-detail"

export default async function EntryDetailPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = await params
  return (
    <>
        <EntryDetail id={decodeURIComponent(id)} />
    </>
  )
}
