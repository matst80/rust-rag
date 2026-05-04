"use client"

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import useSWR from "swr"
import {
  Bell,
  BellOff,
  BookmarkPlus,
  Bot,
  Check,
  Circle,
  Cpu,
  Eraser,
  Hash,
  Loader2,
  Menu,
  Plus,
  Send,
  Trash2,
  User2,
  X,
} from "lucide-react"
import { api } from "@/lib/api"
import type {
  ActiveUser,
  AgentRootDiscoveryMetadata,
  Message,
  MessageChannel,
  MessageSenderKind,
  PermissionOption,
  PermissionRequestMetadata,
} from "@/lib/api/types"
import { cn } from "@/lib/utils"
import { MessageMarkdown } from "./message-markdown"
import { MessageComposer } from "./message-composer"

const PAGE_SIZE = 50
const POLL_WAIT_SECS = 25
const NEAR_BOTTOM_PX = 80
const LOAD_MORE_TRIGGER_PX = 120
const USER_STORAGE_KEY = "rag.messages.user"

interface SessionUser {
  authenticated: boolean
  user?: {
    sub?: string
    name?: string
    preferred_username?: string
    email?: string
  }
}

function randomGuestName(): string {
  const id = Math.random().toString(36).slice(2, 7)
  return `guest-${id}`
}

function loadOrCreateUser(): string {
  if (typeof window === "undefined") return "guest"
  const existing = window.localStorage.getItem(USER_STORAGE_KEY)
  if (existing) return existing
  const fresh = randomGuestName()
  window.localStorage.setItem(USER_STORAGE_KEY, fresh)
  return fresh
}

function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  })
}

function formatDay(ms: number): string {
  return new Date(ms).toLocaleDateString([], {
    month: "short",
    day: "numeric",
    year: "numeric",
  })
}

function senderIcon(kind: MessageSenderKind) {
  if (kind === "agent") return Bot
  if (kind === "system") return Cpu
  return User2
}

const MENTION_TOKEN_RE = /(@[\w.\-]+)/g
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

function renderMessageBody(
  text: string,
  knownUsers: Set<string>,
  selfUser?: string
): ReactNode[] {
  const parts = text.split(MENTION_TOKEN_RE)
  return parts.map((part, idx) => {
    if (idx % 2 === 1) {
      const handle = part.slice(1)
      const known = knownUsers.has(handle.toLowerCase())
      const isSelf = !!selfUser && handle.toLowerCase() === selfUser.toLowerCase()
      if (known || isSelf) {
        return (
          <span
            key={idx}
            className={cn(
              "rounded px-1 font-medium",
              isSelf
                ? "bg-amber-500/20 text-amber-700 dark:text-amber-300"
                : "bg-primary/15 text-primary"
            )}
          >
            {part}
          </span>
        )
      }
    }
    return <span key={idx}>{part}</span>
  })
}

function MessageRow({
  message,
  resolvedPermissions,
  onPermissionResponse,
  onPromote,
  promoteState,
  knownUsers,
  selfUser,
  onSendMessage,
  selectedAgent,
  onSelectAgent,
}: {
  message: Message
  resolvedPermissions: Map<string, string>
  onPermissionResponse: (requestId: string, optionId: string) => void | Promise<void>
  onPromote: (m: Message) => void | Promise<void>
  promoteState?: "pending" | "stored"
  knownUsers: Set<string>
  selfUser?: string
  onSendMessage?: (text: string) => void | Promise<void>
  selectedAgent?: string | null
  onSelectAgent?: (agent: string) => void
}) {
  useEffect(() => {
    if (message.kind === "agent_root_discovery" && !selectedAgent) {
      const md = (message.metadata ?? {}) as Record<string, unknown>
      const meta = md as unknown as AgentRootDiscoveryMetadata
      if (meta.agents && meta.agents.length > 0) {
        onSelectAgent?.(meta.agents[0])
      }
    }
  }, [message, selectedAgent, onSelectAgent])

  const Icon = senderIcon(message.sender_kind)
  const md = (message.metadata ?? {}) as Record<string, unknown>

  // permission_request: render options as buttons; if resolved, grey out + mark.
  if (message.kind === "permission_request") {
    const meta = md as unknown as PermissionRequestMetadata
    const requestId = typeof meta.request_id === "string" ? meta.request_id : null
    const options: PermissionOption[] = Array.isArray(meta.options) ? meta.options : []
    const resolved = requestId ? resolvedPermissions.get(requestId) : undefined
    const toolTitle = meta.tool_call?.title

    return (
      <div className="mb-3 flex gap-3">
        <div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-amber-500/10 text-amber-600 dark:text-amber-400">
          <Bot className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="font-semibold text-sm">{message.sender}</span>
            <span className="text-[10px] uppercase tracking-wide text-amber-600 dark:text-amber-400">
              permission request
            </span>
            <span className="text-[10px] text-muted-foreground">
              {formatTime(message.created_at)}
            </span>
          </div>
          <div className="mt-1 rounded-md border border-amber-500/30 bg-amber-500/5 p-3">
            {toolTitle ? (
              <p className="text-xs font-mono text-muted-foreground">
                tool: {toolTitle}
              </p>
            ) : null}
            {message.text ? (
              <p className="mt-1 whitespace-pre-wrap break-words text-sm text-foreground">
                {renderMessageBody(message.text, knownUsers, selfUser)}
              </p>
            ) : null}
            <div className="mt-2 flex flex-wrap gap-2">
              {options.map((opt) => {
                const active = resolved === opt.option_id
                return (
                  <button
                    key={opt.option_id}
                    type="button"
                    disabled={!!resolved || !requestId}
                    onClick={() =>
                      requestId && onPermissionResponse(requestId, opt.option_id)
                    }
                    className={cn(
                      "rounded-md border px-3 py-1 text-xs font-medium transition-colors",
                      active
                        ? "border-emerald-500 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                        : resolved
                          ? "border-border bg-muted/40 text-muted-foreground"
                          : opt.kind === "allow_once" || opt.kind === "allow_always"
                            ? "border-emerald-500/40 bg-emerald-500/5 text-emerald-700 hover:bg-emerald-500/10 dark:text-emerald-400"
                            : opt.kind === "reject_once" || opt.kind === "reject_always"
                              ? "border-red-500/40 bg-red-500/5 text-red-700 hover:bg-red-500/10 dark:text-red-400"
                              : "border-border bg-background hover:bg-muted/40"
                    )}
                  >
                    {opt.name}
                    {active ? " ✓" : ""}
                  </button>
                )
              })}
            </div>
            {resolved ? (
              <p className="mt-2 text-[10px] text-muted-foreground">
                resolved → {resolved}
              </p>
            ) : null}
          </div>
        </div>
      </div>
    )
  }

  // permission_response: compact summary line.
  if (message.kind === "permission_response") {
    const optionId = typeof md.option_id === "string" ? md.option_id : "?"
    return (
      <div className="mb-2 flex items-center gap-2 pl-11 text-xs text-muted-foreground">
        <span className="font-medium">{message.sender}</span>
        <span>responded</span>
        <span className="font-mono text-foreground">{optionId}</span>
        <span className="text-[10px]">{formatTime(message.created_at)}</span>
      </div>
    )
  }

  // agent_root_discovery: list of folders with buttons.
  if (message.kind === "agent_root_discovery") {
    const meta = md as unknown as AgentRootDiscoveryMetadata
    const folders = Array.isArray(meta.folders) ? meta.folders : []
    const spawnAgent =
      selectedAgent ?? (meta.agents && meta.agents.length > 0 ? meta.agents[0] : null)

    return (
      <div className="mb-3 flex gap-3">
        <div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
          <Bot className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="font-semibold text-sm">{message.sender}</span>
            <span className="text-[10px] uppercase tracking-wide text-primary">
              projects discovered
            </span>
            <span className="text-[10px] text-muted-foreground">
              {formatTime(message.created_at)}
            </span>
          </div>
          <div className="mt-1 rounded-md border border-primary/20 bg-primary/5 p-3">
            {meta.agents && meta.agents.length > 0 && (
              <div className="mb-3">
                <p className="text-[10px] uppercase font-semibold text-muted-foreground mb-1.5">
                  Select Agent Type
                </p>
                <div className="flex flex-wrap gap-2">
                  {meta.agents.map((agent) => {
                    const active = selectedAgent === agent
                    return (
                      <button
                        key={agent}
                        type="button"
                        onClick={() => onSelectAgent?.(agent)}
                        className={cn(
                          "rounded-md border px-3 py-1 text-xs font-medium transition-all active:scale-95",
                          active
                            ? "border-amber-500 bg-amber-500/10 text-amber-700 dark:text-amber-400 shadow-sm"
                            : "border-amber-500/30 bg-background text-amber-600 hover:bg-amber-500/10 hover:border-amber-500"
                        )}
                      >
                        {agent}
                        {active ? " ✓" : ""}
                      </button>
                    )
                  })}
                </div>
              </div>
            )}
            <p className="text-[10px] uppercase font-semibold text-muted-foreground mb-1.5">
              Spawn Project in <code className="bg-muted px-1 rounded lowercase">{meta.root}</code>
            </p>
            <div className="flex flex-wrap gap-2">
              {folders.map((folder) => (
                <button
                  key={folder}
                  type="button"
                  disabled={!spawnAgent}
                  onClick={() => {
                    if (!spawnAgent) return
                    const cmd = `@${message.sender} spawn ${spawnAgent} ${folder}`
                    onSendMessage?.(cmd)
                  }}
                  className={cn(
                    "rounded-md border px-3 py-1 text-xs font-medium transition-all active:scale-95",
                    spawnAgent
                      ? "border-primary/30 bg-background text-primary hover:bg-primary/10 hover:border-primary"
                      : "border-border bg-muted/40 text-muted-foreground cursor-not-allowed"
                  )}
                >
                  Spawn {folder}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Default text/agent_chunk/tool_call.
  const promotable = message.text.trim().length > 0
  const meta = (message.metadata ?? {}) as Record<string, unknown>
  const isThinking = meta.thinking === true
  const isManager = meta.manager === true || message.sender === "manager"
  return (
    <div className="group relative mb-3 flex gap-3">
      <div
        className={cn(
          "mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md",
          isManager
            ? "bg-amber-500/15 text-amber-600 dark:text-amber-400"
            : message.sender_kind === "agent"
              ? "bg-primary/10 text-primary"
              : message.sender_kind === "system"
                ? "bg-muted text-muted-foreground"
                : "bg-secondary text-secondary-foreground",
          isThinking && "animate-pulse"
        )}
      >
        <Icon className="size-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="font-semibold text-sm">{message.sender}</span>
          {isManager ? (
            <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-amber-600 dark:text-amber-400">
              MANAGER
            </span>
          ) : (
            <span className="text-[10px] uppercase tracking-wide text-muted-foreground">
              {message.sender_kind}
            </span>
          )}
          {message.kind !== "text" && !isThinking ? (
            <span className="text-[10px] uppercase tracking-wide text-muted-foreground">
              · {message.kind}
            </span>
          ) : null}
          {isThinking ? (
            <span className="flex items-center gap-1 text-[10px] uppercase tracking-wide text-amber-600 dark:text-amber-400">
              <span className="inline-block size-1.5 animate-pulse rounded-full bg-amber-500" />
              thinking
            </span>
          ) : null}
          <span className="text-[10px] text-muted-foreground">
            {formatTime(message.created_at)}
          </span>
        </div>
        <div
          className={cn(
            "break-words text-sm text-foreground",
            isThinking && "italic text-muted-foreground"
          )}
        >
          <MessageMarkdown
            text={message.text}
            knownUsers={knownUsers}
            selfUser={selfUser}
          />
        </div>
      </div>
      {promotable ? (
        <button
          type="button"
          onClick={() => onPromote(message)}
          disabled={promoteState === "pending" || promoteState === "stored"}
          title={
            promoteState === "stored"
              ? "Stored as RAG entry"
              : promoteState === "pending"
                ? "Storing…"
                : "Promote to RAG knowledge"
          }
          className={cn(
            "absolute right-0 top-0 flex size-7 items-center justify-center rounded-md border bg-background transition-opacity",
            promoteState === "stored"
              ? "border-emerald-500/40 text-emerald-600 dark:text-emerald-400 opacity-100"
              : promoteState === "pending"
                ? "border-border text-muted-foreground opacity-100"
                : "border-border text-muted-foreground opacity-0 hover:text-foreground group-hover:opacity-100"
          )}
          aria-label="Promote to RAG"
        >
          {promoteState === "pending" ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : promoteState === "stored" ? (
            <Check className="size-3.5" />
          ) : (
            <BookmarkPlus className="size-3.5" />
          )}
        </button>
      ) : null}
    </div>
  )
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve()
    const timer = setTimeout(resolve, ms)
    signal.addEventListener("abort", () => {
      clearTimeout(timer)
      resolve()
    })
  })
}

export function MessagesInterface() {
  const [activeChannel, setActiveChannel] = useState<string>("general")
  const [sending, setSending] = useState(false)
  const [newChannelOpen, setNewChannelOpen] = useState(false)
  const [newChannelName, setNewChannelName] = useState("")
  const [user, setUser] = useState<string>("")
  const [sidebarOpen, setSidebarOpen] = useState<boolean>(false)
  const [isDesktop, setIsDesktop] = useState<boolean>(false)

  useEffect(() => {
    if (typeof window === "undefined") return
    const mql = window.matchMedia("(min-width: 768px)")
    const sync = () => {
      setIsDesktop(mql.matches)
      setSidebarOpen(mql.matches)
    }
    sync()
    mql.addEventListener("change", sync)
    return () => mql.removeEventListener("change", sync)
  }, [])

  const [selectedAgent, setSelectedAgent] = useState<string | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
  const [activeUsers, setActiveUsers] = useState<ActiveUser[]>([])
  const [hasMore, setHasMore] = useState(false)
  const [loadingMore, setLoadingMore] = useState(false)
  const [initialLoaded, setInitialLoaded] = useState(false)
  // Per-message promote state: `pending` while POST in flight, `stored` after success.
  const [promoted, setPromoted] = useState<Map<string, "pending" | "stored">>(
    new Map()
  )

  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  // Pinned-to-bottom decision is captured *before* DOM updates land (commit phase).
  // Using a ref avoids stale-closure surprises in the poll loop.
  const wasNearBottomRef = useRef(true)
  // For prepend (load-more): preserve scroll position so the user doesn't jump.
  const pendingScrollAdjustRef = useRef<number | null>(null)

  // Resolve user identity once.
  useEffect(() => {
    let cancelled = false
    fetch("/api/auth/session", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : null))
      .then((session: SessionUser | null) => {
        if (cancelled) return
        const u = session?.user
        const sessionName =
          u?.preferred_username ?? u?.name ?? u?.email ?? u?.sub
        if (session?.authenticated && sessionName && sessionName.trim()) {
          setUser(sessionName.trim())
        } else {
          setUser(loadOrCreateUser())
        }
      })
      .catch(() => {
        if (!cancelled) setUser(loadOrCreateUser())
      })
    return () => {
      cancelled = true
    }
  }, [])

  // Channels list (cheap, separate from thread loop).
  const { data: channelsData, mutate: refreshChannels } = useSWR<MessageChannel[]>(
    "messages.channels",
    () => api.messages.channels(),
    { refreshInterval: 10_000 }
  )

  const channels = useMemo<MessageChannel[]>(() => {
    const list = channelsData ?? []
    const ensured: MessageChannel[] = list.some((c) => c.channel === "manager")
      ? [...list]
      : [{ channel: "manager", message_count: 0, last_message_at: 0 }, ...list]
    if (ensured.length === 0) {
      return [
        { channel: "manager", message_count: 0, last_message_at: 0 },
        { channel: "general", message_count: 0, last_message_at: 0 },
      ]
    }
    ensured.sort((a, b) => {
      if (a.channel === "manager") return -1
      if (b.channel === "manager") return 1
      return 0
    })
    return ensured
  }, [channelsData])

  useEffect(() => {
    if (channels.length > 0 && !channels.some((c) => c.channel === activeChannel)) {
      setActiveChannel(channels[0].channel)
    }
  }, [channels, activeChannel])

  // Track permission_request ids we've already notified about so we don't
  // re-fire notifications on each render or on initial load.
  const notifiedRequestsRef = useRef<Set<string>>(new Set())
  const notifSeededRef = useRef(false)
  const [notifPermission, setNotifPermission] = useState<NotificationPermission | "unsupported">(
    typeof window !== "undefined" && "Notification" in window
      ? Notification.permission
      : "unsupported"
  )

  // Reset thread on channel/user change. The poll-loop effect below picks up
  // from the empty state.
  useEffect(() => {
    setMessages([])
    setActiveUsers([])
    setHasMore(false)
    setInitialLoaded(false)
    wasNearBottomRef.current = true
    pendingScrollAdjustRef.current = null
    notifiedRequestsRef.current = new Set()
    notifSeededRef.current = false
  }, [activeChannel, user])

  // Long-poll loop: initial fetch (desc limit N, reversed for asc display),
  // then chained `since`-cursor polls with wait=25s. Cursor advances on the
  // greater of created_at and updated_at so streamed agent_chunk patches
  // surface as in-place updates rather than fresh rows.
  useEffect(() => {
    if (!activeChannel || !user) return
    const ctrl = new AbortController()
    const signal = ctrl.signal
    let cursor = 0

    const advanceCursor = (msgs: Message[]) => {
      for (const m of msgs) {
        const t = Math.max(m.created_at, m.updated_at ?? 0)
        if (t > cursor) cursor = t
      }
    }

    const mergeMessages = (incoming: Message[]) => {
      setMessages((prev) => {
        if (incoming.length === 0) return prev
        const byId = new Map(prev.map((m) => [m.id, m]))
        let appended = false
        for (const m of incoming) {
          if (byId.has(m.id)) {
            byId.set(m.id, m)
          } else {
            byId.set(m.id, m)
            appended = true
          }
        }
        // Re-emit in stable order: original order for known ids + new ones at the end.
        const order: Message[] = []
        const seen = new Set<string>()
        for (const m of prev) {
          const next = byId.get(m.id)
          if (next) {
            order.push(next)
            seen.add(m.id)
          }
        }
        for (const m of incoming) {
          if (!seen.has(m.id)) {
            order.push(m)
            seen.add(m.id)
          }
        }
        return appended || incoming.some((m) => prev.some((p) => p.id === m.id))
          ? order
          : prev
      })
    }

    const poll = async () => {
      // Initial fetch: latest PAGE_SIZE in desc, reversed for display.
      try {
        const r = await api.messages.list({
          channel: activeChannel,
          limit: PAGE_SIZE,
          sort_order: "desc",
          user,
          user_kind: "human",
        })
        if (signal.aborted) return
        const ascending = [...r.messages].reverse()
        setMessages(ascending)
        setActiveUsers(r.active_users)
        setHasMore(r.total_count > ascending.length)
        setInitialLoaded(true)
        advanceCursor(ascending)
      } catch (err) {
        if (signal.aborted) return
        console.error("messages initial err", err)
        await sleep(2000, signal)
      }

      while (!signal.aborted) {
        try {
          const r = await api.messages.list({
            channel: activeChannel,
            since: cursor + 1,
            limit: 200,
            sort_order: "asc",
            user,
            user_kind: "human",
            wait: POLL_WAIT_SECS,
          })
          if (signal.aborted) return
          setActiveUsers(r.active_users)
          if (r.deleted_ids.length > 0) {
            const drop = new Set(r.deleted_ids)
            setMessages((prev) => prev.filter((m) => !drop.has(m.id)))
          }
          if (r.messages.length > 0) {
            mergeMessages(r.messages)
            advanceCursor(r.messages)
          }
        } catch (err) {
          if (signal.aborted) return
          console.error("messages poll err", err)
          await sleep(2000, signal)
        }
      }
    }

    poll()
    return () => ctrl.abort()
  }, [activeChannel, user])

  // Capture near-bottom state before render commits, so we can decide whether
  // to scroll-pin after appended messages mount.
  useLayoutEffect(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight
    wasNearBottomRef.current = distance <= NEAR_BOTTOM_PX
  })

  // After messages change: pin to bottom only if user was near bottom (or
  // first load). Prepend-from-load-more preserves scroll position via
  // pendingScrollAdjustRef.
  useLayoutEffect(() => {
    const el = scrollContainerRef.current
    if (!el) return
    if (pendingScrollAdjustRef.current !== null) {
      el.scrollTop = el.scrollHeight - pendingScrollAdjustRef.current
      pendingScrollAdjustRef.current = null
      return
    }
    if (!initialLoaded) return
    if (wasNearBottomRef.current) {
      messagesEndRef.current?.scrollIntoView({ block: "end" })
    }
  }, [messages, initialLoaded])

  // Force scroll to bottom on channel switch / first load of a channel.
  useLayoutEffect(() => {
    if (!initialLoaded) return
    const el = scrollContainerRef.current
    if (!el) return
    el.scrollTop = el.scrollHeight
    wasNearBottomRef.current = true
  }, [activeChannel, initialLoaded])

  const loadMore = useCallback(async () => {
    if (loadingMore || !hasMore || messages.length === 0 || !activeChannel) return
    const oldest = messages[0]
    const el = scrollContainerRef.current
    if (!el) return
    setLoadingMore(true)
    try {
      const r = await api.messages.list({
        channel: activeChannel,
        until: oldest.created_at - 1,
        limit: PAGE_SIZE,
        sort_order: "desc",
        // No user/user_kind here — load-more shouldn't reset presence kind.
      })
      const older = [...r.messages].reverse()
      if (older.length === 0) {
        setHasMore(false)
      } else {
        // Preserve scroll: record distance from bottom before prepend, restore after.
        pendingScrollAdjustRef.current = el.scrollHeight - el.scrollTop
        setMessages((prev) => {
          const existing = new Set(prev.map((m) => m.id))
          const fresh = older.filter((m) => !existing.has(m.id))
          return [...fresh, ...prev]
        })
        setHasMore(r.total_count > messages.length + older.length)
      }
    } catch (err) {
      console.error("load-more error", err)
    } finally {
      setLoadingMore(false)
    }
  }, [activeChannel, hasMore, loadingMore, messages])

  // Load-more on scroll-up.
  useEffect(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const onScroll = () => {
      if (el.scrollTop <= LOAD_MORE_TRIGGER_PX) {
        void loadMore()
      }
    }
    el.addEventListener("scroll", onScroll, { passive: true })
    return () => el.removeEventListener("scroll", onScroll)
  }, [loadMore])

  const sendMessage = useCallback(
    async (text: string) => {
      if (!text || sending || !activeChannel) return
      setSending(true)
      try {
        const sent = await api.messages.send({
          channel: activeChannel,
          text,
        })
        // Optimistic append; long-poll will dedupe via id set.
        setMessages((prev) =>
          prev.some((m) => m.id === sent.id) ? prev : [...prev, sent]
        )
        // Force scroll-to-bottom on own send regardless of prior position.
        wasNearBottomRef.current = true
        void refreshChannels()
      } catch (err) {
        console.error("send error", err)
      } finally {
        setSending(false)
      }
    },
    [activeChannel, user, sending, refreshChannels]
  )

  const handleCreateChannel = () => {
    const name = newChannelName.trim().toLowerCase().replace(/[^a-z0-9-_]/g, "-")
    if (!name) return
    setActiveChannel(name)
    setNewChannelName("")
    setNewChannelOpen(false)
  }

  const wipeChannel = useCallback(
    async (channel: string): Promise<boolean> => {
      try {
        await api.messages.clearChannel(channel)
        // Drop locally-cached thread immediately if it's the active one;
        // long-poll tombstones cover any other clients.
        if (channel === activeChannel) {
          setMessages([])
          setHasMore(false)
        }
        await refreshChannels()
        return true
      } catch (err) {
        console.error("clear channel error", err)
        return false
      }
    },
    [activeChannel, refreshChannels]
  )

  const handleClearActive = useCallback(async () => {
    if (!activeChannel) return
    if (
      !window.confirm(
        `Clear all messages in #${activeChannel}? This cannot be undone.`
      )
    )
      return
    await wipeChannel(activeChannel)
  }, [activeChannel, wipeChannel])

  const handleDeleteChannel = useCallback(
    async (channel: string) => {
      if (
        !window.confirm(
          `Delete channel #${channel}? All messages will be wiped. This cannot be undone.`
        )
      )
        return
      const ok = await wipeChannel(channel)
      if (!ok) return
      if (channel === activeChannel) {
        const remaining = (channelsData ?? []).filter((c) => c.channel !== channel)
        setActiveChannel(remaining[0]?.channel ?? "general")
      }
    },
    [activeChannel, channelsData, wipeChannel]
  )

  // Mention candidates list (kept in parent because it's derived from
  // messages/activeUsers; passed to the composer which owns its own draft).
  const mentionCandidates = useMemo<string[]>(() => {
    const set = new Set<string>()
    for (const u of activeUsers) if (u.user) set.add(u.user)
    for (const m of messages) if (m.sender) set.add(m.sender)
    if (user) set.delete(user)
    return Array.from(set).sort((a, b) => a.localeCompare(b))
  }, [activeUsers, messages, user])

  const knownUsersLower = useMemo(() => {
    const s = new Set<string>()
    for (const c of mentionCandidates) s.add(c.toLowerCase())
    if (user) s.add(user.toLowerCase())
    return s
  }, [mentionCandidates, user])

  // request_id -> option_id of any permission_response we've seen, to grey out
  // resolved permission_request rows.
  const resolvedPermissions = useMemo(() => {
    const map = new Map<string, string>()
    for (const m of messages) {
      if (m.kind !== "permission_response") continue
      const md = (m.metadata ?? {}) as Record<string, unknown>
      const requestId = typeof md.request_id === "string" ? md.request_id : null
      const optionId = typeof md.option_id === "string" ? md.option_id : null
      if (requestId && optionId) map.set(requestId, optionId)
    }
    return map
  }, [messages])

  const requestNotificationPermission = useCallback(async () => {
    if (typeof window === "undefined" || !("Notification" in window)) return
    if (Notification.permission === "granted") {
      setNotifPermission("granted")
      return
    }
    try {
      const result = await Notification.requestPermission()
      setNotifPermission(result)
    } catch (err) {
      console.error("notification permission error", err)
    }
  }, [])

  // Surface live permission_request rows as native browser notifications.
  // First pass after channel load seeds the seen-set so historical rows don't
  // fire; subsequent passes notify on previously-unseen, unresolved requests.
  useEffect(() => {
    if (!initialLoaded) return
    if (typeof window === "undefined" || !("Notification" in window)) return

    if (!notifSeededRef.current) {
      for (const m of messages) {
        if (m.kind !== "permission_request") continue
        const md = (m.metadata ?? {}) as Record<string, unknown>
        const rid = typeof md.request_id === "string" ? md.request_id : null
        if (rid) notifiedRequestsRef.current.add(rid)
      }
      notifSeededRef.current = true
      return
    }

    if (Notification.permission !== "granted") return

    for (const m of messages) {
      if (m.kind !== "permission_request") continue
      const md = (m.metadata ?? {}) as Record<string, unknown>
      const rid = typeof md.request_id === "string" ? md.request_id : null
      if (!rid || notifiedRequestsRef.current.has(rid)) continue
      // Already-resolved on arrival: just record, don't fire.
      if (resolvedPermissions.has(rid)) {
        notifiedRequestsRef.current.add(rid)
        continue
      }
      const toolCall = md.tool_call as { title?: string } | undefined
      const toolTitle =
        toolCall && typeof toolCall.title === "string" ? toolCall.title : null
      const title = `Permission request from ${m.sender}`
      const body =
        (toolTitle ? `tool: ${toolTitle}\n` : "") +
        (m.text || "Tap to review.")
      try {
        const n = new Notification(title, {
          body,
          tag: `perm-${rid}`,
          icon: "/favicon.ico",
          requireInteraction: true,
        })
        n.onclick = () => {
          window.focus()
          n.close()
        }
      } catch (err) {
        console.error("notification error", err)
      }
      notifiedRequestsRef.current.add(rid)
    }
  }, [messages, initialLoaded, resolvedPermissions])

  const promoteMessage = useCallback(
    async (m: Message) => {
      const text = m.text.trim()
      if (!text) return
      setPromoted((prev) => {
        const next = new Map(prev)
        next.set(m.id, "pending")
        return next
      })
      try {
        await api.items.create({
          text,
          source_id: m.channel,
          metadata: {
            promoted_from: "messages",
            message_id: m.id,
            channel: m.channel,
            sender: m.sender,
            sender_kind: m.sender_kind,
            created_at: m.created_at,
          },
        })
        setPromoted((prev) => {
          const next = new Map(prev)
          next.set(m.id, "stored")
          return next
        })
      } catch (err) {
        console.error("promote err", err)
        setPromoted((prev) => {
          const next = new Map(prev)
          next.delete(m.id)
          return next
        })
      }
    },
    []
  )

  const respondToPermission = useCallback(
    async (requestId: string, optionId: string) => {
      if (!activeChannel) return
      try {
        await api.messages.send({
          channel: activeChannel,
          text: `permission ${optionId}`,
          kind: "permission_response",
          metadata: { request_id: requestId, option_id: optionId },
        })
      } catch (err) {
        console.error("permission response error", err)
      }
    },
    [activeChannel, user]
  )

  const groupedThread = useMemo(() => {
    const groups: { day: string; messages: Message[] }[] = []
    for (const m of messages) {
      const day = formatDay(m.created_at)
      const tail = groups[groups.length - 1]
      if (tail && tail.day === day) {
        tail.messages.push(m)
      } else {
        groups.push({ day, messages: [m] })
      }
    }
    return groups
  }, [messages])

  return (
    <div className="relative flex h-[calc(100dvh-49px)]">
      {sidebarOpen && !isDesktop ? (
        <button
          type="button"
          aria-label="Close channels"
          onClick={() => setSidebarOpen(false)}
          className="fixed inset-0 z-30 bg-black/40 md:hidden"
        />
      ) : null}

      {/* Sidebar */}
      <aside
        className={cn(
          "z-40 flex w-64 flex-col border-r border-border bg-background transition-transform duration-200 md:bg-muted/20",
          isDesktop
            ? sidebarOpen
              ? "relative translate-x-0"
              : "hidden"
            : sidebarOpen
              ? "fixed inset-y-0 left-0 translate-x-0"
              : "fixed inset-y-0 left-0 -translate-x-full"
        )}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <span className="font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground">
            Channels
          </span>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => setNewChannelOpen((v) => !v)}
              className="text-muted-foreground hover:text-foreground"
              aria-label="New channel"
            >
              <Plus className="size-4" />
            </button>
            <button
              type="button"
              onClick={() => setSidebarOpen(false)}
              className="text-muted-foreground hover:text-foreground md:hidden"
              aria-label="Close channels"
            >
              <X className="size-4" />
            </button>
          </div>
        </div>
        {newChannelOpen ? (
          <div className="flex gap-1 border-b border-border p-2">
            <input
              autoFocus
              value={newChannelName}
              onChange={(e) => setNewChannelName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleCreateChannel()
                if (e.key === "Escape") setNewChannelOpen(false)
              }}
              placeholder="channel-name"
              className="flex-1 rounded-md border border-input bg-background px-2 py-1 text-xs"
            />
            <button
              type="button"
              onClick={handleCreateChannel}
              className="rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground"
            >
              Add
            </button>
          </div>
        ) : null}
        <ul className="flex-1 overflow-y-auto py-2">
          {channels.map((c) => (
            <li key={c.channel} className="group/row relative">
              <button
                type="button"
                onClick={() => {
                  setActiveChannel(c.channel)
                  if (!isDesktop) setSidebarOpen(false)
                }}
                className={cn(
                  "flex w-full items-center justify-between gap-2 px-4 py-1.5 pr-9 text-left text-sm transition-colors",
                  c.channel === activeChannel
                    ? "bg-primary/10 text-primary"
                    : "text-foreground hover:bg-muted/40"
                )}
              >
                <span className="flex items-center gap-2 truncate">
                  <Hash className="size-3.5 shrink-0" />
                  <span className="truncate">{c.channel}</span>
                  {c.channel === "manager" ? (
                    <span className="ml-1 rounded bg-amber-500/20 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-amber-600 dark:text-amber-400">
                      LLM
                    </span>
                  ) : null}
                </span>
                {c.message_count > 0 ? (
                  <span className="text-[10px] text-muted-foreground">
                    {c.message_count}
                  </span>
                ) : null}
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation()
                  void handleDeleteChannel(c.channel)
                }}
                className="absolute right-2 top-1/2 -translate-y-1/2 flex size-6 items-center justify-center rounded text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive group-hover/row:opacity-100 focus:opacity-100"
                aria-label={`Delete channel ${c.channel}`}
                title="Delete channel"
              >
                <Trash2 className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      </aside>

      {/* Thread */}
      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-3 py-2 md:px-6 md:py-3">
          <button
            type="button"
            onClick={() => setSidebarOpen((v) => !v)}
            className="flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            aria-label={sidebarOpen ? "Hide channels" : "Show channels"}
            title={sidebarOpen ? "Hide channels" : "Show channels"}
          >
            <Menu className="size-4" />
          </button>
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <Hash className="size-4 shrink-0 text-muted-foreground" />
            <h1 className="truncate font-semibold">{activeChannel}</h1>
            <div className="hidden items-center gap-1.5 rounded-md bg-muted/40 px-2 py-1 sm:flex">
              <Circle className="size-2 fill-emerald-500 text-emerald-500" />
              <span className="text-xs">{activeUsers.length}</span>
              <span className="hidden text-xs text-muted-foreground lg:inline">
                {activeUsers.length > 0
                  ? `· ${activeUsers
                      .map((u) => u.user)
                      .slice(0, 3)
                      .join(", ")}${activeUsers.length > 3 ? ` +${activeUsers.length - 3}` : ""}`
                  : "active"}
              </span>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            {user ? (
              <span className="hidden font-mono text-[10px] uppercase tracking-[2px] text-muted-foreground md:inline">
                you: {user}
              </span>
            ) : null}
            {notifPermission !== "unsupported" ? (
              <button
                type="button"
                onClick={() => void requestNotificationPermission()}
                disabled={notifPermission === "denied"}
                className={cn(
                  "flex size-8 items-center justify-center rounded-md border border-border bg-background",
                  notifPermission === "granted"
                    ? "text-emerald-600 dark:text-emerald-400"
                    : notifPermission === "denied"
                      ? "text-muted-foreground opacity-50"
                      : "text-muted-foreground hover:text-foreground"
                )}
                title={
                  notifPermission === "granted"
                    ? "Notifications on"
                    : notifPermission === "denied"
                      ? "Notifications blocked"
                      : "Enable notifications"
                }
                aria-label="Toggle notifications"
              >
                {notifPermission === "denied" ? (
                  <BellOff className="size-3.5" />
                ) : (
                  <Bell
                    className={cn(
                      "size-3.5",
                      notifPermission === "granted" && "fill-current"
                    )}
                  />
                )}
              </button>
            ) : null}
            <button
              type="button"
              onClick={() => void handleClearActive()}
              disabled={!activeChannel || messages.length === 0}
              className="flex size-8 items-center justify-center gap-1 rounded-md border border-border bg-background text-xs text-muted-foreground hover:text-foreground disabled:opacity-40 disabled:hover:text-muted-foreground md:size-auto md:px-2 md:py-1"
              title="Clear all messages"
              aria-label="Clear channel"
            >
              <Eraser className="size-3.5" />
              <span className="hidden md:inline">Clear</span>
            </button>
          </div>
        </div>

        <div
          ref={scrollContainerRef}
          className="flex-1 overflow-y-auto px-3 py-3 md:px-6 md:py-4"
        >
          {hasMore ? (
            <div className="mb-3 flex justify-center">
              <button
                type="button"
                onClick={() => void loadMore()}
                disabled={loadingMore}
                className="flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1 text-xs text-muted-foreground hover:text-foreground"
              >
                {loadingMore ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : null}
                {loadingMore ? "Loading..." : "Load older"}
              </button>
            </div>
          ) : null}
          {!initialLoaded ? (
            <p className="text-center text-sm text-muted-foreground">Loading…</p>
          ) : groupedThread.length === 0 ? (
            <p className="text-center text-sm text-muted-foreground">
              No messages yet. Be the first to post in #{activeChannel}.
            </p>
          ) : (
            groupedThread.map((group) => (
              <div key={group.day}>
                <div className="my-3 flex items-center gap-3">
                  <div className="flex-1 border-t border-border" />
                  <span className="text-[10px] font-mono uppercase tracking-[2px] text-muted-foreground">
                    {group.day}
                  </span>
                  <div className="flex-1 border-t border-border" />
                </div>
                {group.messages.map((m) => (
                  <MessageRow
                    key={m.id}
                    message={m}
                    resolvedPermissions={resolvedPermissions}
                    onPermissionResponse={respondToPermission}
                    onPromote={promoteMessage}
                    promoteState={promoted.get(m.id)}
                    knownUsers={knownUsersLower}
                    selfUser={user}
                    onSendMessage={sendMessage}
                    selectedAgent={selectedAgent}
                    onSelectAgent={setSelectedAgent}
                  />
                ))}
              </div>
            ))
          )}
          <div ref={messagesEndRef} />
        </div>

        <MessageComposer
          channel={activeChannel}
          sending={sending}
          candidates={mentionCandidates}
          activeUsers={activeUsers}
          onSend={sendMessage}
        />
      </section>
    </div>
  )
}
