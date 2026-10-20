"use client"

export interface JournalTask {
  id: string
  text: string
  done: boolean
  entryId: string
  entryTitle: string
  line: number
}

export interface JournalDay {
  date: Date
  key: string // YYYY-MM-DD
  entryId: string | null
  count: number // 1 if a daily note exists, else 0
  openTasks: number
  doneTasks: number
}

export function dailyNoteId(date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0")
  return `daily-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

export function dailyNotePath(date = new Date()): string {
  return `journal/${dailyNoteId(date).replace(/^daily-/, "")}`
}

export function dailyKey(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

export function parseDailyKey(key: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(key)
  if (!m) return null
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]))
  return Number.isNaN(d.getTime()) ? null : d
}

/** `daily-YYYY-MM-DD` -> Date, or null for non-daily ids. */
export function parseDailyId(id: string): Date | null {
  const m = /^daily-(\d{4})-(\d{2})-(\d{2})$/.exec(id)
  if (!m) return null
  return parseDailyKey(`${m[1]}-${m[2]}-${m[3]}`)
}

/** Extract `- [ ]` / `- [x]` tasks from markdown, skipping fenced code blocks. */
export function extractTasks(
  text: string,
  entryId: string,
  entryTitle: string
): JournalTask[] {
  const tasks: JournalTask[] = []
  const lines = text.split("\n")
  let inFence = false
  lines.forEach((line, idx) => {
    if (/^\s*```/.test(line)) {
      inFence = !inFence
      return
    }
    if (inFence) return
    const m = /^\s*[-*]\s*\[([ xX])\]\s*(.+)$/.exec(line)
    if (m) {
      const done = m[1].toLowerCase() === "x"
      const taskText = m[2].trim()
      if (taskText) {
        tasks.push({
          id: `${entryId}:L${idx + 1}`,
          text: taskText,
          done,
          entryId,
          entryTitle,
          line: idx + 1,
        })
      }
    }
  })
  return tasks
}

export function monthGrid(year: number, month: number): Date[] {
  // Month is 0-indexed. Returns the 6x7 grid cell dates (Mon-first) covering it.
  const first = new Date(year, month, 1)
  const startOffset = (first.getDay() + 6) % 7 // Monday = 0
  const start = new Date(year, month, 1 - startOffset)
  return Array.from({ length: 42 }, (_, i) => {
    const d = new Date(start)
    d.setDate(start.getDate() + i)
    return d
  })
}

export function heatmapWeeks(days: JournalDay[], weeks = 16): JournalDay[][] {
  // Last `weeks` Monday-first columns for a GitHub-style heatmap.
  const byKey = new Map(days.map((d) => [d.key, d]))
  const today = new Date()
  today.setHours(0, 0, 0, 0)
  const mondayOffset = (today.getDay() + 6) % 7
  const lastMonday = new Date(today)
  lastMonday.setDate(today.getDate() - mondayOffset - (weeks - 1) * 7)
  const cols: JournalDay[][] = []
  for (let w = 0; w < weeks; w++) {
    const col: JournalDay[] = []
    for (let d = 0; d < 7; d++) {
      const date = new Date(lastMonday)
      date.setDate(lastMonday.getDate() + w * 7 + d)
      const key = dailyKey(date)
      col.push(
        byKey.get(key) ?? {
          date,
          key,
          entryId: null,
          count: 0,
          openTasks: 0,
          doneTasks: 0,
        }
      )
    }
    cols.push(col)
  }
  return cols
}
