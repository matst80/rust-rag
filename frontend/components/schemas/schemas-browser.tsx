"use client"

import Link from "next/link"
import { useState } from "react"
import { useRouter } from "next/navigation"
import { useSchemas, useDeleteSchema, useUpsertSchema } from "@/lib/api/hooks"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

export function SchemasBrowser() {
  const { data: schemas, isLoading, error, mutate } = useSchemas()
  const { trigger: deleteSchema } = useDeleteSchema()
  const [newName, setNewName] = useState("")
  const router = useRouter()

  async function onDelete(name: string, count: number | null | undefined) {
    const force = (count ?? 0) > 0
    const confirmText = force
      ? `Delete schema "${name}"? ${count} entries reference it — they will be untyped.`
      : `Delete schema "${name}"?`
    if (!window.confirm(confirmText)) return
    try {
      await deleteSchema({ typeName: name, force })
      mutate()
    } catch (err) {
      window.alert(`Delete failed: ${(err as Error).message}`)
    }
  }

  function onCreate() {
    const slug = newName.trim().toLowerCase().replace(/[^a-z0-9_]+/g, "_")
    if (!slug) return
    router.push(`/schemas/${encodeURIComponent(slug)}`)
  }

  return (
    <div className="container mx-auto p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Typed-entry schemas</h1>
        <div className="flex gap-2">
          <Input
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="new_type_name"
            className="w-56"
          />
          <Button onClick={onCreate} disabled={!newName.trim()}>
            Create
          </Button>
        </div>
      </div>

      {isLoading && <p className="text-muted-foreground">Loading…</p>}
      {error && <p className="text-destructive">Error: {(error as Error).message}</p>}

      {schemas && schemas.length === 0 && (
        <p className="text-muted-foreground">
          No schemas registered. Bundled schemas seed on first boot from
          <code className="ml-1">assets/schemas/*.json</code>.
        </p>
      )}

      {schemas && schemas.length > 0 && (
        <div className="rounded-md border">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr className="text-left">
                <th className="p-3 font-medium">Type</th>
                <th className="p-3 font-medium">Title</th>
                <th className="p-3 font-medium">Items</th>
                <th className="p-3 font-medium">Updated</th>
                <th className="p-3" />
              </tr>
            </thead>
            <tbody>
              {schemas.map((s) => (
                <tr key={s.type_name} className="border-t">
                  <td className="p-3 font-mono">
                    <Link
                      href={`/schemas/${encodeURIComponent(s.type_name)}`}
                      className="text-primary hover:underline"
                    >
                      {s.type_name}
                    </Link>
                  </td>
                  <td className="p-3">{s.title ?? <span className="text-muted-foreground">—</span>}</td>
                  <td className="p-3">{s.item_count ?? 0}</td>
                  <td className="p-3 text-muted-foreground">
                    {new Date(s.updated_at).toLocaleString()}
                  </td>
                  <td className="p-3 text-right">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => onDelete(s.type_name, s.item_count)}
                    >
                      Delete
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
