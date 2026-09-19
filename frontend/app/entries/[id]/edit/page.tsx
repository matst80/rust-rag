import { EntryEditClient } from "./edit-client"

export default async function EditEntryPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = await params
  const decodedId = decodeURIComponent(id)

  return (
    <>
        <EntryEditClient id={decodedId} />
    </>
  )
}
