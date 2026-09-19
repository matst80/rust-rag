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

/** "REPO: …", "Source: …", "Path: …" — key/value header lines, not titles. */
const HEADER_KEY_LINE = /^[A-Za-z][A-Za-z0-9 _./\\-]{0,24}:\s/
const PATH_LIKE_LINE = /^(\/|~\/|\.{1,2}\/|https?:\/\/|www\.)/
const NOISE_LINE = /^(```|~~~|\||%|<!--|-\-\-)/

function looksLikeHeaderJunk(line: string): boolean {
  return (
    HEADER_KEY_LINE.test(line) ||
    PATH_LIKE_LINE.test(line) ||
    NOISE_LINE.test(line) ||
    line.length < 4
  )
}

function clipTitle(text: string, maxLength: number): string {
  return text.length > maxLength ? text.slice(0, maxLength).trimEnd() + '…' : text
}

/** Short human label for an entry: metadata.title, else a markdown heading near
 *  the top, else the first line that actually reads like a title (skipping
 *  "REPO:"-style headers, paths, fences), else id. */
export function entryTitle(entry: TitledEntry, maxLength = 60): string {
  const metaTitle = entry.metadata?.title
  if (typeof metaTitle === 'string' && metaTitle.trim()) return clipTitle(metaTitle.trim(), maxLength)
  const lines = (entry.text ?? '').split('\n')

  // A markdown heading within the first few lines is almost always the doc title.
  const firstLines = lines.map((l) => l.trim()).filter(Boolean).slice(0, 4)
  for (const line of firstLines) {
    const heading = /^#{1,4}\s+(.{6,})$/.exec(line)
    if (heading) return clipTitle(heading[1].trim(), maxLength)
  }

  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed || looksLikeHeaderJunk(trimmed)) continue
    return clipTitle(trimmed.replace(/^[-*•]\s+/, ''), maxLength)
  }
  return clipTitle(lines.find((l) => l.trim())?.trim() || entry.id, maxLength)
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
