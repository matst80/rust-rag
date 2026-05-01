"use client"

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { Circle, Loader2, Send, User2 } from "lucide-react"
import { cn } from "@/lib/utils"

const MENTION_TRIGGER_RE = /(?:^|\s)@([\w.\-]*)$/

function getMentionTrigger(
  text: string,
  caret: number
): { query: string; start: number } | null {
  const before = text.slice(0, caret)
  const m = MENTION_TRIGGER_RE.exec(before)
  if (!m) return null
  const matchedAt = before.lastIndexOf("@")
  return { query: m[1] ?? "", start: matchedAt }
}

interface MessageComposerProps {
  channel: string
  sending: boolean
  candidates: string[]
  activeUsers: { user: string }[]
  onSend: (text: string) => Promise<void> | void
}

function MessageComposerInner({
  channel,
  sending,
  candidates,
  activeUsers,
  onSend,
}: MessageComposerProps) {
  const [draft, setDraft] = useState("")
  const [mentionState, setMentionState] = useState<{
    start: number
    query: string
  } | null>(null)
  const [mentionIndex, setMentionIndex] = useState(0)
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const filteredMentions = useMemo<string[]>(() => {
    if (!mentionState) return []
    const q = mentionState.query.toLowerCase()
    const list = candidates.filter((u) =>
      q.length === 0 ? true : u.toLowerCase().includes(q)
    )
    return list.slice(0, 8)
  }, [candidates, mentionState])

  useEffect(() => {
    setMentionIndex(0)
  }, [mentionState?.query])

  const updateMentionFromCaret = useCallback(() => {
    const el = textareaRef.current
    if (!el) return
    const caret = el.selectionStart ?? 0
    const trig = getMentionTrigger(el.value, caret)
    setMentionState(trig)
  }, [])

  const insertMention = useCallback(
    (handle: string) => {
      const el = textareaRef.current
      if (!el || !mentionState) return
      const caret = el.selectionStart ?? draft.length
      const before = draft.slice(0, mentionState.start)
      const after = draft.slice(caret)
      const insert = `@${handle} `
      const next = before + insert + after
      setDraft(next)
      setMentionState(null)
      requestAnimationFrame(() => {
        const pos = before.length + insert.length
        el.focus()
        el.setSelectionRange(pos, pos)
      })
    },
    [draft, mentionState]
  )

  const handleSend = useCallback(async () => {
    const text = draft.trim()
    if (!text) return
    await onSend(text)
    setDraft("")
  }, [draft, onSend])

  return (
    <form
      className="border-t border-border p-4"
      onSubmit={(e) => {
        e.preventDefault()
        void handleSend()
      }}
    >
      <div className="relative">
        {mentionState && filteredMentions.length > 0 ? (
          <div className="absolute bottom-full left-0 right-0 mb-2 z-20 max-h-56 overflow-y-auto rounded-md border border-border bg-popover shadow-md">
            <div className="px-3 py-1.5 font-mono text-[10px] uppercase tracking-[2px] text-muted-foreground border-b border-border">
              Mention
              {mentionState.query ? (
                <span className="ml-1 normal-case tracking-normal text-foreground">
                  @{mentionState.query}
                </span>
              ) : null}
            </div>
            <ul>
              {filteredMentions.map((handle, idx) => (
                <li key={handle}>
                  <button
                    type="button"
                    onMouseDown={(e) => {
                      e.preventDefault()
                      insertMention(handle)
                    }}
                    onMouseEnter={() => setMentionIndex(idx)}
                    className={cn(
                      "flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm",
                      idx === mentionIndex
                        ? "bg-primary/10 text-primary"
                        : "text-foreground hover:bg-muted/40"
                    )}
                  >
                    <User2 className="size-3.5 shrink-0 text-muted-foreground" />
                    <span className="truncate">{handle}</span>
                    {activeUsers.some((u) => u.user === handle) ? (
                      <Circle className="ml-auto size-2 fill-emerald-500 text-emerald-500" />
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
            <div className="px-3 py-1 border-t border-border font-mono text-[10px] text-muted-foreground">
              ↑↓ navigate · ↵/tab select · esc close
            </div>
          </div>
        ) : null}
        <div className="flex items-end gap-2 rounded-lg border border-input bg-background p-2">
          <textarea
            ref={textareaRef}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value)
              requestAnimationFrame(updateMentionFromCaret)
            }}
            onKeyUp={(e) => {
              if (
                e.key === "ArrowLeft" ||
                e.key === "ArrowRight" ||
                e.key === "Home" ||
                e.key === "End"
              ) {
                updateMentionFromCaret()
              }
            }}
            onClick={updateMentionFromCaret}
            onBlur={() => {
              setTimeout(() => setMentionState(null), 120)
            }}
            onKeyDown={(e) => {
              if (mentionState && filteredMentions.length > 0) {
                if (e.key === "ArrowDown") {
                  e.preventDefault()
                  setMentionIndex((i) => (i + 1) % filteredMentions.length)
                  return
                }
                if (e.key === "ArrowUp") {
                  e.preventDefault()
                  setMentionIndex(
                    (i) =>
                      (i - 1 + filteredMentions.length) % filteredMentions.length
                  )
                  return
                }
                if (e.key === "Enter" || e.key === "Tab") {
                  e.preventDefault()
                  insertMention(filteredMentions[mentionIndex])
                  return
                }
                if (e.key === "Escape") {
                  e.preventDefault()
                  setMentionState(null)
                  return
                }
              }
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault()
                void handleSend()
              }
            }}
            placeholder={`Message #${channel}`}
            rows={1}
            className="flex-1 resize-none bg-transparent px-2 py-1.5 text-sm outline-none"
          />
          <button
            type="submit"
            disabled={sending || !draft.trim()}
            className={cn(
              "flex size-9 items-center justify-center rounded-md transition-colors",
              draft.trim() && !sending
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-muted text-muted-foreground"
            )}
            aria-label="Send"
          >
            {sending ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Send className="size-4" />
            )}
          </button>
        </div>
      </div>
    </form>
  )
}

export const MessageComposer = memo(MessageComposerInner)
