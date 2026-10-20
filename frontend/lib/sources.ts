"use client"

const HIDDEN_KEY = "rag:hidden-sources:v1"
const SHOW_HIDDEN_KEY = "rag:show-hidden-sources:v1"

/** Sources that are noisy by default (agent scratch memory, not curated wiki). */
export function isMemorySource(sourceId: string): boolean {
  const s = sourceId.trim()
  if (!s) return false
  const lower = s.toLowerCase()
  if (lower === "memory" || lower === "manager_memory" || lower === "agent_notes") return true
  if (lower.endsWith("_memory")) return true
  // Per-session buckets like project:foo:todos are task noise, not wiki docs.
  if (lower.endsWith(":todos")) return true
  return false
}

/** System-ish buckets hidden behind the "hidden collections" toggle. */
export function isHiddenByDefault(sourceId: string): boolean {
  return isMemorySource(sourceId)
}

function readJson<T>(key: string, fallback: T): T {
  if (typeof window === "undefined") return fallback
  try {
    const raw = window.localStorage.getItem(key)
    if (!raw) return fallback
    return JSON.parse(raw) as T
  } catch {
    return fallback
  }
}

export function getHiddenSources(): string[] {
  return readJson<string[]>(HIDDEN_KEY, [])
}

export function setHiddenSources(ids: string[]): void {
  try {
    window.localStorage.setItem(HIDDEN_KEY, JSON.stringify(ids))
  } catch {
    // ignore quota errors
  }
}

export function getShowHiddenSources(): boolean {
  return readJson<boolean>(SHOW_HIDDEN_KEY, false)
}

export function setShowHiddenSources(v: boolean): void {
  try {
    window.localStorage.setItem(SHOW_HIDDEN_KEY, JSON.stringify(v))
  } catch {
    // ignore
  }
}

/** Split a category/source list into visible + hidden (default-hidden or user-hidden). */
export function splitSources<T extends { id: string }>(
  items: T[] | undefined,
  userHidden: string[],
): { visible: T[]; hidden: T[] } {
  if (!items) return { visible: [], hidden: [] }
  const userSet = new Set(userHidden)
  const visible: T[] = []
  const hidden: T[] = []
  for (const item of items) {
    if (userSet.has(item.id) || isHiddenByDefault(item.id)) hidden.push(item)
    else visible.push(item)
  }
  return { visible, hidden }
}
