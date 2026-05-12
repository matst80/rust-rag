"use client"

import { useEffect, useState } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import { useSchema, useUpsertSchema, useDeleteSchema, useSchemas } from "@/lib/api/hooks"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"

const DEFAULT_SCHEMA = `{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {},
  "required": [],
  "additionalProperties": false
}`

export function SchemaEditor({ typeName }: { typeName: string }) {
  const { data, error, isLoading, mutate } = useSchema(typeName)
  const { mutate: mutateList } = useSchemas()
  const { trigger: upsert, isMutating } = useUpsertSchema(typeName)
  const { trigger: deleteSchema } = useDeleteSchema()
  const router = useRouter()

  const [title, setTitle] = useState("")
  const [description, setDescription] = useState("")
  const [schemaText, setSchemaText] = useState(DEFAULT_SCHEMA)
  const [parseError, setParseError] = useState<string | null>(null)
  const [saveError, setSaveError] = useState<string | null>(null)

  useEffect(() => {
    if (data) {
      setTitle(data.title ?? "")
      setDescription(data.description ?? "")
      setSchemaText(JSON.stringify(data.json_schema, null, 2))
    }
  }, [data])

  async function onSave() {
    setSaveError(null)
    let parsed: Record<string, unknown>
    try {
      parsed = JSON.parse(schemaText)
      setParseError(null)
    } catch (e) {
      setParseError((e as Error).message)
      return
    }
    try {
      await upsert({
        type_name: typeName,
        json_schema: parsed,
        title: title || null,
        description: description || null,
      })
      await mutate()
      await mutateList()
    } catch (err) {
      setSaveError((err as Error).message)
    }
  }

  async function onDelete() {
    const count = data?.item_count ?? 0
    const force = count > 0
    const confirmText = force
      ? `Delete schema "${typeName}"? ${count} entries reference it — they will be untyped.`
      : `Delete schema "${typeName}"?`
    if (!window.confirm(confirmText)) return
    try {
      await deleteSchema({ typeName, force })
      await mutateList()
      router.push("/schemas")
    } catch (err) {
      window.alert(`Delete failed: ${(err as Error).message}`)
    }
  }

  const isNew = !data && !isLoading && !error

  return (
    <div className="container mx-auto p-6 space-y-4 max-w-4xl">
      <div className="flex items-center justify-between">
        <div>
          <Link href="/schemas" className="text-sm text-muted-foreground hover:underline">
            ← All schemas
          </Link>
          <h1 className="text-2xl font-semibold mt-1">
            <span className="font-mono">{typeName}</span>
            {isNew && <span className="ml-2 text-sm text-muted-foreground">(new)</span>}
          </h1>
        </div>
        <div className="flex gap-2">
          {!isNew && (
            <Button variant="outline" onClick={onDelete}>
              Delete
            </Button>
          )}
          <Button onClick={onSave} disabled={isMutating}>
            {isMutating ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>

      {error && !isNew && (
        <p className="text-destructive">Error loading schema: {(error as Error).message}</p>
      )}

      <div className="grid gap-4">
        <label className="space-y-1">
          <span className="text-sm font-medium">Title</span>
          <Input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Human-readable title" />
        </label>
        <label className="space-y-1">
          <span className="text-sm font-medium">Description</span>
          <Textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What this type represents"
            rows={2}
          />
        </label>
        <label className="space-y-1">
          <span className="text-sm font-medium">JSON Schema</span>
          <Textarea
            value={schemaText}
            onChange={(e) => setSchemaText(e.target.value)}
            rows={24}
            className="font-mono text-xs"
            spellCheck={false}
          />
        </label>
        {parseError && (
          <p className="text-sm text-destructive">JSON parse error: {parseError}</p>
        )}
        {saveError && (
          <p className="text-sm text-destructive">Save failed: {saveError}</p>
        )}
        {data && (
          <p className="text-xs text-muted-foreground">
            {data.item_count ?? 0} entries currently typed as <code>{typeName}</code>. Updated{" "}
            {new Date(data.updated_at).toLocaleString()}.
          </p>
        )}
      </div>
    </div>
  )
}
