"use client"
import { useState } from "react"
import { useSWRConfig } from "swr"
import { api, type Entry } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { toast } from "sonner"
export function InboxAssignment({ entry, onAssigned }: { entry: Entry; onAssigned: (id: string, hide: boolean) => void }) {
  const [path, setPath] = useState(""); const [tag, setTag] = useState(""); const [busy, setBusy] = useState(false)
  const { mutate } = useSWRConfig()
  const assign = async () => {
    if (busy || (!path.trim() && !tag.trim())) return
    setBusy(true); onAssigned(entry.id, true)
    try {
      await api.items.update(entry.id, { text: entry.text, source_id: entry.source_id, type: entry.type, data: entry.data, path: path.trim() || undefined, metadata: { ...entry.metadata, ...(tag.trim() ? { tags: tag.trim() } : {}) } })
      await mutate(key => Array.isArray(key) && ["items", "item", "entries-tree", "entries-paths"].includes(key[0]))
      toast.success("Entry organized")
    } catch { onAssigned(entry.id, false); toast.error("Could not organize entry"); } finally { setBusy(false) }
  }
  return <form className="flex flex-wrap gap-2 px-4 pb-4" onSubmit={event => { event.preventDefault(); void assign() }}><Input aria-label="Assign path" placeholder="Path" className="w-48" value={path} onChange={e => setPath(e.target.value)} /><Input aria-label="Assign tag" placeholder="Tag" className="w-40" value={tag} onChange={e => setTag(e.target.value)} /><Button disabled={busy || (!path.trim() && !tag.trim())}>Assign</Button></form>
}
