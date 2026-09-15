import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

interface TitledEntry {
  id: string
  text: string
  metadata?: { title?: unknown } | null
}

/** Short human label for an entry: metadata.title, else first line of text, else id. */
export function entryTitle(entry: TitledEntry, maxLength = 60): string {
  const metaTitle = entry.metadata?.title
  if (typeof metaTitle === 'string' && metaTitle.trim()) return metaTitle.trim()
  const firstLine = entry.text
    .split('\n')
    .find((l) => l.trim())
    ?.trim()
    .replace(/^#+\s*/, '')
  if (firstLine) return firstLine.length > maxLength ? firstLine.slice(0, maxLength) + '…' : firstLine
  return entry.id
}

/** Label for a graph-edge endpoint we only have an id (and maybe a server-resolved title) for. */
export function edgeEndpointTitle(id: string, title?: string | null): string {
  if (title && title.trim()) return title.trim()
  return id.length > 24 ? id.slice(0, 24) + '…' : id
}

export function formatRelativeTime(timestamp: number): string {
  const now = Date.now()
  const diff = now - timestamp
  
  const seconds = Math.floor(diff / 1000)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)
  
  if (days > 7) {
    return new Date(timestamp).toLocaleDateString()
  }
  if (days > 0) return `${days}d ago`
  if (hours > 0) return `${hours}h ago`
  if (minutes > 0) return `${minutes}m ago`
  return 'just now'
}

export function stringToHslColor(str: string, s = 70, l = 50): string {
  let hash = 0
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash)
  }
  const h = Math.abs(hash) % 360
  return `hsl(${h}, ${s}%, ${l}%)`
}
