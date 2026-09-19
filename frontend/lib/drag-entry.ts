"use client"

import { useSyncExternalStore } from "react"

export interface EntryDragPayload {
  id: string
  source_id: string
  path?: string | null
  title: string
}

const MIME = "application/json"

// Module-level "currently dragging" state so any component (drop targets,
// banners) can render feedback without prop drilling. dataTransfer is unreadable
// during dragover, so this is also the only way to know what is in flight.
let currentDrag: EntryDragPayload | null = null
const listeners = new Set<() => void>()

function getSnapshot(): EntryDragPayload | null {
  return currentDrag
}

function getServerSnapshot(): EntryDragPayload | null {
  return null
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function emit() {
  for (const listener of listeners) listener()
}

/** The entry currently being dragged, or null. */
export function useDraggingEntry(): EntryDragPayload | null {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot)
}

/** Call from a drag source's onDragStart. */
export function setEntryDragData(dataTransfer: DataTransfer, payload: EntryDragPayload) {
  dataTransfer.setData(MIME, JSON.stringify(payload))
  dataTransfer.setData("text/plain", payload.id)
  dataTransfer.effectAllowed = "move"
  currentDrag = payload
  emit()
}

/** Call from a drag source's onDragEnd (fires even for cancelled drops). */
export function clearEntryDrag() {
  if (currentDrag) {
    currentDrag = null
    emit()
  }
}

/** Whether a dragover/drop event carries an entry payload at all
 *  (types are inspectable during dragover, data is not). */
export function hasEntryDragData(dataTransfer: DataTransfer): boolean {
  return dataTransfer.types.includes(MIME) || dataTransfer.types.includes("text/plain")
}

/** Call from a drop target's onDrop. */
export function readEntryDragData(dataTransfer: DataTransfer): EntryDragPayload | null {
  const raw = dataTransfer.getData(MIME)
  if (raw) {
    try {
      const parsed = JSON.parse(raw) as Partial<EntryDragPayload>
      if (parsed && typeof parsed.id === "string") {
        return {
          id: parsed.id,
          source_id: typeof parsed.source_id === "string" ? parsed.source_id : "",
          path: parsed.path ?? null,
          title: parsed.title ?? parsed.id,
        }
      }
    } catch {
      // fall through to text/plain
    }
  }
  const id = dataTransfer.getData("text/plain")
  return id ? { id, source_id: "", path: null, title: id } : null
}
