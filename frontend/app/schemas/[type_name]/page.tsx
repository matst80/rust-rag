import { SchemaEditor } from "@/components/schemas/schema-editor"

export default async function SchemaEditPage({
  params,
}: {
  params: Promise<{ type_name: string }>
}) {
  const { type_name } = await params
  return (
    <>
        <SchemaEditor typeName={decodeURIComponent(type_name)} />
    </>
  )
}
