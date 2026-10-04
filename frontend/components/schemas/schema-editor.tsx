"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import CodeMirror from "@uiw/react-codemirror"
import { oneDark } from "@codemirror/theme-one-dark"
import { EditorView, keymap } from "@codemirror/view"
import { jsonSchema } from "codemirror-json-schema"
import { useTheme } from "next-themes"
import { useSchema, useUpsertSchema, useDeleteSchema, useSchemas } from "@/lib/api/hooks"
import { DRAFT_07_META_SCHEMA } from "@/lib/schemas/draft-07-meta-schema"
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

function SchemaJsonEditor({
  value,
  onChange,
  onSave,
}: {
  value: string
  onChange: (text: string) => void
  onSave: () => void
}) {
  const { resolvedTheme } = useTheme()
  const saveRef = useRef(onSave)
  useEffect(() => {
    saveRef.current = onSave
  })

  // jsonSchema() bundles the JSON language, syntax linting, schema validation,
  // completion, and hover docs against the draft-07 meta-schema.
  const extensions = useMemo(
    () => [
      ...jsonSchema(DRAFT_07_META_SCHEMA),
      EditorView.lineWrapping,
      keymap.of([
        {
          key: "Mod-s",
          preventDefault: true,
          run: () => {
            saveRef.current()
            return true
          },
        },
      ]),
    ],
    []
  )

  return (
    <div className="rounded-md border overflow-hidden">
      <CodeMirror
        value={value}
        height="600px"
        theme={resolvedTheme === "dark" ? oneDark : "light"}
        extensions={extensions}
        onChange={onChange}
        basicSetup={{
          lineNumbers: true,
          foldGutter: true,
          highlightActiveLine: true,
          highlightActiveLineGutter: true,
          bracketMatching: true,
          closeBrackets: true,
          autocompletion: true,
        }}
        className="text-xs font-mono [&_.cm-editor]:bg-transparent"
      />
    </div>
  )
}

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

  function onSchemaTextChange(text: string) {
    setSchemaText(text)
    try {
      JSON.parse(text)
      setParseError(null)
    } catch {
      // Invalid mid-edit JSON is flagged inline by the editor's syntax linter;
      // onSave still blocks and reports it.
    }
  }

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
        <div className="space-y-1">
          <span className="text-sm font-medium">
            JSON Schema <span className="text-muted-foreground font-normal">(draft-07, ⌘S to save)</span>
          </span>
          <SchemaJsonEditor value={schemaText} onChange={onSchemaTextChange} onSave={onSave} />
        </div>
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
