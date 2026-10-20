"use client"

import { Suspense, useEffect } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import { Loader2 } from "lucide-react"
import { api } from "@/lib/api"
import { dailyNoteId, dailyNotePath, parseDailyKey } from "@/lib/journal"

export { dailyNoteId, dailyNotePath }

/** Open-or-create a daily note (today, or `?date=YYYY-MM-DD` from the journal), then land on its entry page. */
export default function TodayPage() {
  return (
    <Suspense
      fallback={
        <div className="flex h-[calc(100vh-3rem)] items-center justify-center gap-3">
          <Loader2 className="size-5 animate-spin text-muted-foreground" />
          <span className="font-mono text-xs uppercase tracking-widest text-muted-foreground">
            Opening today&apos;s note…
          </span>
        </div>
      }
    >
      <TodayResolver />
    </Suspense>
  )
}

function TodayResolver() {
  const router = useRouter()
  const searchParams = useSearchParams()

  useEffect(() => {
    let cancelled = false
    const resolve = async () => {
      const dateParam = searchParams?.get("date")
      const parsed = dateParam ? parseDailyKey(dateParam) : null
      const d = parsed ?? new Date()
      const id = dailyNoteId(d)
      try {
        await api.items.get(id)
      } catch {
        // Missing — create it with the journal template.
        const label = d.toLocaleDateString([], {
          weekday: "long",
          year: "numeric",
          month: "long",
          day: "numeric",
        })
        try {
          await api.items.create({
            id,
            text: `# ${label}\n\n## Log\n\n- `,
            metadata: { kind: "daily-note" },
            source_id: "knowledge",
            path: dailyNotePath(d),
          })
        } catch {
          // Creation raced or failed — fall through to the entry page,
          // which will show the not-found state if it really is missing.
        }
      }
      if (!cancelled) router.replace(`/entries/${encodeURIComponent(id)}`)
    }
    resolve()
    return () => {
      cancelled = true
    }
  }, [router, searchParams])

  return (
    <div className="flex h-[calc(100vh-3rem)] items-center justify-center gap-3">
      <Loader2 className="size-5 animate-spin text-muted-foreground" />
      <span className="font-mono text-xs uppercase tracking-widest text-muted-foreground">
        Opening today's note…
      </span>
    </div>
  )
}
