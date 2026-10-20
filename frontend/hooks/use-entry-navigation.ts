"use client"
import { useRef } from "react"
import { useRouter } from "next/navigation"
import { toast } from "sonner"
import { toggleStar } from "@/lib/favorites"
import { entryTitle } from "@/lib/utils"
import type { Entry } from "@/lib/api"
export function useEntryNavigation(entries: Entry[]) {
  const ref = useRef<HTMLDivElement>(null)
  const router = useRouter()
  const onKeyDown = (event: React.KeyboardEvent) => {
    const target = event.target as HTMLElement
    if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey || target.closest("input, textarea, select, button, [contenteditable=true], [role=dialog], [role=combobox]")) return
    const rows = Array.from(ref.current?.querySelectorAll<HTMLElement>("[data-entry-row]") ?? [])
    const current = rows.findIndex(row => row.contains(target))
    if (event.key === "j" || event.key === "k") {
      event.preventDefault()
      const next = Math.max(0, Math.min(rows.length - 1, current + (event.key === "j" ? 1 : -1)))
      rows[next]?.focus(); rows[next]?.scrollIntoView({ block: "nearest" })
    } else if (current >= 0 && (event.key === "e" || event.key === "s")) {
      const entry = entries[current]; if (!entry) return
      event.preventDefault()
      if (event.key === "e") router.push(`/entries/${encodeURIComponent(entry.id)}?edit=true`)
      else { const starred = toggleStar(entry.id, entryTitle(entry)); window.dispatchEvent(new Event("favorites-changed")); toast.success(starred ? "Starred" : "Unstarred") }
    }
  }
  return { ref, onKeyDown, tabIndex: 0, "aria-label": "Entries. J and K to navigate, E to edit, S to star." }
}
