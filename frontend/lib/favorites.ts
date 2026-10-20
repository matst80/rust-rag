"use client"

export interface PinnedEntry {
  id: string
  title: string
  at: number
}

const STARRED_KEY = "rag:starred-entries:v1"
const RECENTS_KEY = "rag:recent-entries:v1"
const MAX_RECENTS = 12

function readList(key: string): PinnedEntry[] {
  if (typeof window === "undefined") return []
  try {
    const raw = window.localStorage.getItem(key)
    if (!raw) return []
    const parsed = JSON.parse(raw) as PinnedEntry[]
    return Array.isArray(parsed) ? parsed.filter((e) => e && typeof e.id === "string") : []
  } catch {
    return []
  }
}

function writeList(key: string, list: PinnedEntry[]): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(list))
  } catch {
    // quota errors — ignore
  }
}

export function getStarred(): PinnedEntry[] {
  return readList(STARRED_KEY)
}

export function isStarred(id: string): boolean {
  return readList(STARRED_KEY).some((e) => e.id === id)
}

/** Toggle star; returns the new starred state. */
export function toggleStar(id: string, title: string): boolean {
  const list = readList(STARRED_KEY)
  const existing = list.findIndex((e) => e.id === id)
  if (existing >= 0) {
    list.splice(existing, 1)
    writeList(STARRED_KEY, list)
    return false
  }
  writeList(STARRED_KEY, [{ id, title, at: Date.now() }, ...list].slice(0, 50))
  return true
}

export function getRecents(): PinnedEntry[] {
  return readList(RECENTS_KEY)
}

/** Record a visit; recents ring, most recent first, starred entries excluded by callers. */
export function pushRecent(id: string, title: string): void {
  const list = readList(RECENTS_KEY).filter((e) => e.id !== id)
  writeList(RECENTS_KEY, [{ id, title, at: Date.now() }, ...list].slice(0, MAX_RECENTS))
}
