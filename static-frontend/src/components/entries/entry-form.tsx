import { useState } from "react"
import { Plus, X, Save, ArrowLeft } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { useCreateItem, useUpdateItem, type Entry, type EntryMetadata } from "@/lib/api"
import { useSWRConfig } from "swr"

interface EntryFormProps {
  entry?: Entry
  mode: "create" | "edit"
}

export function EntryForm({ entry, mode }: EntryFormProps) {
  const { mutate } = useSWRConfig()
  const { trigger: createItem, isMutating: isCreating } = useCreateItem()
  const { trigger: updateItem, isMutating: isUpdating } = useUpdateItem(entry?.id ?? "")

  const [id, setId] = useState(entry?.id ?? "")
  const [text, setText] = useState(entry?.text ?? "")
  const [sourceId, setSourceId] = useState(entry?.source_id ?? "knowledge")
  const [metadata, setMetadata] = useState<EntryMetadata>(entry?.metadata ?? {})
  const [newMetaKey, setNewMetaKey] = useState("")
  const [newMetaValue, setNewMetaValue] = useState("")
  const [error, setError] = useState<string | null>(null)

  const isMutating = isCreating || isUpdating

  const handleAddMetadata = () => {
    if (!newMetaKey.trim()) return
    setMetadata((prev) => ({
      ...prev,
      [newMetaKey.trim()]: newMetaValue.trim(),
    }))
    setNewMetaKey("")
    setNewMetaValue("")
  }

  const handleRemoveMetadata = (key: string) => {
    setMetadata((prev) => {
      const next = { ...prev }
      delete next[key]
      return next
    })
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)

    if (!text.trim() || !sourceId.trim()) {
      setError("Text and source are required")
      return
    }

    try {
      if (mode === "create") {
        await createItem({
          ...(id.trim() && { id: id.trim() }),
          text: text.trim(),
          source_id: sourceId.trim(),
          metadata,
        })
      } else {
        await updateItem({
          text: text.trim(),
          source_id: sourceId.trim(),
          metadata,
        })
      }
      mutate("items")
      mutate("categories")
      window.location.href = "/entries/"
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save entry")
    }
  }

  return (
    <form onSubmit={handleSubmit} className="mx-auto max-w-2xl p-4">
      <div className="mb-6 flex items-center gap-4">
        <Button variant="ghost" size="icon" asChild>
          <a href="/entries/">
            <ArrowLeft className="size-4" />
          </a>
        </Button>
        <h1 className="text-2xl font-bold">
          {mode === "create" ? "Create Entry" : "Edit Entry"}
        </h1>
      </div>

      {error && (
        <div className="mb-4 rounded-md bg-destructive/10 p-3 text-sm text-destructive">
          {error}
        </div>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Entry Details</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <Label htmlFor="id">ID {mode === "create" ? "(optional)" : ""}</Label>
            <Input
              id="id"
              value={id}
              onChange={(e) => setId(e.target.value)}
              placeholder={mode === "create" ? "Leave blank to auto-generate" : "doc-knowledge-1"}
              disabled={mode === "edit"}
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="source">Source</Label>
            <Input
              id="source"
              value={sourceId}
              onChange={(e) => setSourceId(e.target.value)}
              placeholder="knowledge"
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="text">Content</Label>
            <Textarea
              id="text"
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="Enter the entry content..."
              className="min-h-32"
            />
          </div>
        </CardContent>
      </Card>

      <Card className="mt-4">
        <CardHeader>
          <CardTitle>Metadata</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {Object.keys(metadata).length > 0 && (
            <div className="flex flex-wrap gap-2">
              {Object.entries(metadata).map(([key, value]) => (
                <Badge key={key} variant="secondary" className="gap-1 pr-1">
                  {key}: {String(value)}
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    className="size-4 hover:bg-destructive hover:text-destructive-foreground"
                    onClick={() => handleRemoveMetadata(key)}
                  >
                    <X className="size-3" />
                  </Button>
                </Badge>
              ))}
            </div>
          )}
          <div className="flex gap-2">
            <Input
              placeholder="Key"
              value={newMetaKey}
              onChange={(e) => setNewMetaKey(e.target.value)}
              className="flex-1"
            />
            <Input
              placeholder="Value"
              value={newMetaValue}
              onChange={(e) => setNewMetaValue(e.target.value)}
              className="flex-1"
            />
            <Button type="button" variant="outline" onClick={handleAddMetadata}>
              <Plus className="size-4" />
            </Button>
          </div>
        </CardContent>
      </Card>

      <div className="mt-6 flex justify-end gap-2">
        <Button variant="outline" asChild>
          <a href="/entries/">Cancel</a>
        </Button>
        <Button type="submit" disabled={isMutating}>
          <Save className="size-4" />
          {isMutating ? "Saving..." : "Save Entry"}
        </Button>
      </div>
    </form>
  )
}
