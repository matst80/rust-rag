"use client"

import { useCallback } from "react"
import { useSWRConfig } from "swr"
import { toast } from "sonner"
import { getItem, updateItem } from "@/lib/api"
import type { Entry } from "@/lib/api"
import { entryTitle } from "@/lib/utils"

/** Revalidate every wiki-related cache. The read hooks use array keys
 *  (["entries-paths", src], ["entries-tree", src, prefix], ["items", opts]),
 *  so string-keyed mutate() calls never match — always invalidate by predicate. */
function invalidateWikiCaches(mutate: ReturnType<typeof useSWRConfig>["mutate"], entryId: string) {
  return Promise.all([
    mutate(["item", entryId]),
    mutate((key) => Array.isArray(key) && key[0] === "items"),
    mutate(
      (key) =>
        Array.isArray(key) && (key[0] === "entries-paths" || key[0] === "entries-tree")
    ),
  ])
}

async function persistMove(entry: Entry, targetSourceId: string, targetPath: string) {
  await updateItem(entry.id, {
    text: entry.text,
    source_id: targetSourceId,
    // "" (not null) — the backend treats an explicit null as "field not
    // provided" and keeps the old path; an empty string clears it.
    path: targetPath,
    metadata: entry.metadata ?? {},
    type: entry.type ?? null,
    data: entry.data ?? null,
  })
}

/** Move an entry to another source/path (empty path = unfile) and refresh all
 *  wiki trees. Returns the moved entry, or null when nothing happened. */
export function useWikiMoveEntry() {
  const { mutate } = useSWRConfig()

  return useCallback(
    async (targetSourceId: string, targetPath: string, entryId: string): Promise<Entry | null> => {
      try {
        const entry = await getItem(entryId)
        if (!entry) {
          toast.error("Entry not found")
          return null
        }

        // Empty source means "keep the entry's current source" (e.g. unfile drops).
        const effectiveSourceId = targetSourceId || entry.source_id
        const sourceChanged = entry.source_id !== effectiveSourceId
        const pathChanged = (entry.path ?? "") !== (targetPath || "")
        if (!sourceChanged && !pathChanged) {
          toast.info(
            `"${entryTitle(entry)}" is already in ${effectiveSourceId}${targetPath ? `/${targetPath}` : ""}`
          )
          return entry
        }

        await persistMove(entry, effectiveSourceId, targetPath)
        await invalidateWikiCaches(mutate, entryId)

        const destination = `${targetSourceId}${targetPath ? `/${targetPath}` : ""}`
        toast.success(`Moved "${entryTitle(entry)}" to ${destination}`, {
          action: {
            label: "Undo",
            onClick: () => {
              void persistMove(entry, entry.source_id, entry.path ?? "")
                .then(() => invalidateWikiCaches(mutate, entryId))
                .then(() =>
                  toast.success(
                    `Undone — "${entryTitle(entry)}" is back in ${entry.source_id}${entry.path ? `/${entry.path}` : ""}`
                  )
                )
                .catch((err: unknown) =>
                  toast.error(err instanceof Error ? err.message : "Failed to undo move")
                )
            },
          },
        })
        return entry
      } catch (err) {
        toast.error(err instanceof Error ? err.message : "Failed to move entry")
        return null
      }
    },
    [mutate]
  )
}
