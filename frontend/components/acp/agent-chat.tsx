"use client"

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { Bot, Circle, Link2, Loader2, Plus, Send, Square, User2, X } from "lucide-react"
import { cn } from "@/lib/utils"
import { MessageMarkdown } from "@/components/messages/message-markdown"

const EMPTY_USERS: Set<string> = new Set()

type AcpEnvelope = Record<string, unknown>

interface AcpEvent {
	kind: string
	payload: Record<string, unknown>
	receivedAt: number
	localSeq: number
}

interface SessionInfo {
	acp_session_id: string
	project_path?: string
	thread_id?: number | null
	status?: string
	agent_command?: string
	history?: unknown[]
}

interface AcpInstance {
	name: string
	host: string
	port: number
	url: string
	txt: Record<string, string>
}

interface ConnectionState {
	status: "connecting" | "open" | "closed" | "error" | "disabled"
	error?: string
}

const RECONNECT_INITIAL_MS = 1000
const RECONNECT_MAX_MS = 30000
const NEAR_BOTTOM_PX = 80

function envelopeKind(envelope: AcpEnvelope): { kind: string; payload: Record<string, unknown> } | null {
	if (!envelope || typeof envelope !== "object") return null
	const keys = Object.keys(envelope)
	if (keys.length === 1 && envelope[keys[0]] && typeof envelope[keys[0]] === "object") {
		return { kind: keys[0], payload: envelope[keys[0]] as Record<string, unknown> }
	}
	const k = (envelope as { kind?: unknown; type?: unknown }).kind ?? (envelope as { type?: unknown }).type
	if (typeof k === "string") return { kind: k, payload: envelope as Record<string, unknown> }
	return null
}

function detachAndClose(ws: WebSocket) {
	ws.onopen = null
	ws.onmessage = null
	ws.onerror = null
	ws.onclose = null
	try {
		ws.close()
	} catch {
		// ignore — closing a connecting socket can throw on some browsers
	}
}

function sessionIdOf(payload: Record<string, unknown>): string | undefined {
	const a = payload["acp_session_id"]
	if (typeof a === "string") return a
	const b = payload["session_id"]
	if (typeof b === "string") return b
	return undefined
}

export function AgentChat() {
	const [conn, setConn] = useState<ConnectionState>({ status: "connecting" })
	const [sessions, setSessions] = useState<Record<string, SessionInfo>>({})
	const [eventsBySession, setEventsBySession] = useState<Record<string, AcpEvent[]>>({})
	const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
	const [pendingPermissions, setPendingPermissions] = useState<Record<string, AcpEvent>>({})
	const [draft, setDraft] = useState("")
	const [instances, setInstances] = useState<AcpInstance[]>([])
	const [activeInstance, setActiveInstance] = useState<string | null>(null)
	const wsRef = useRef<WebSocket | null>(null)
	const reconnectAttemptRef = useRef(0)
	const seqRef = useRef(0)

	const refreshInstances = useCallback(async () => {
		try {
			const res = await fetch("/bff/acp/instances", { credentials: "include" })
			if (!res.ok) return
			const data = (await res.json()) as { instances: AcpInstance[]; active: string | null }
			setInstances(data.instances)
			setActiveInstance(data.active)
		} catch (err) {
			console.warn("acp instances fetch failed", err)
		}
	}, [])

	const send = useCallback((envelope: AcpEnvelope) => {
		const ws = wsRef.current
		if (!ws || ws.readyState !== WebSocket.OPEN) {
			console.warn("acp_ws not open; dropping", envelope)
			return false
		}
		ws.send(JSON.stringify(envelope))
		return true
	}, [])

	const connect = useCallback(async () => {
		setConn({ status: "connecting" })
		let url: string
		let token: string
		try {
			const res = await fetch("/bff/acp/config", { credentials: "include" })
			if (res.status === 503) {
				setConn({ status: "disabled", error: "ACP WS endpoint not configured" })
				return
			}
			if (!res.ok) {
				setConn({ status: "error", error: `config fetch ${res.status}` })
				return
			}
			const data = (await res.json()) as { url?: string; token?: string }
			if (!data.url || !data.token) {
				setConn({ status: "disabled", error: "missing url/token" })
				return
			}
			url = data.url
			token = data.token
		} catch (err) {
			setConn({ status: "error", error: String(err) })
			return
		}

		const sep = url.includes("?") ? "&" : "?"
		const wsUrl = `${url}${sep}token=${encodeURIComponent(token)}`
		const ws = new WebSocket(wsUrl)
		wsRef.current = ws

		ws.onopen = () => {
			setConn({ status: "open" })
			reconnectAttemptRef.current = 0
		}

		ws.onmessage = (msg) => {
			let envelope: AcpEnvelope
			try {
				envelope = JSON.parse(typeof msg.data === "string" ? msg.data : "{}")
			} catch {
				return
			}
			const parsed = envelopeKind(envelope)
			if (!parsed) return
			const { kind, payload } = parsed
			seqRef.current += 1
			const ev: AcpEvent = {
				kind,
				payload,
				receivedAt: Date.now(),
				localSeq: seqRef.current,
			}

			const k = kind.toLowerCase()

			if (k === "state_snapshot" || (k === "snapshot" && Array.isArray((payload as { sessions?: unknown }).sessions))) {
				const list = Array.isArray((payload as { sessions?: unknown }).sessions)
					? (payload as { sessions: SessionInfo[] }).sessions
					: []
				const map: Record<string, SessionInfo> = {}
				const ingestedBySession: Record<string, AcpEvent[]> = {}
				for (const s of list) {
					if (!s?.acp_session_id) continue
					map[s.acp_session_id] = s
					const hist = Array.isArray(s.history) ? s.history : []
					const arr: AcpEvent[] = []
					for (const h of hist) {
						if (!h || typeof h !== "object") continue
						const hp = h as Record<string, unknown>
						const hkind = typeof hp.type === "string" ? hp.type : "unknown"
						seqRef.current += 1
						arr.push({
							kind: hkind,
							payload: hp,
							receivedAt: Date.now(),
							localSeq: seqRef.current,
						})
					}
					if (arr.length > 0) ingestedBySession[s.acp_session_id] = arr
				}
				setSessions(map)
				if (Object.keys(ingestedBySession).length > 0) {
					setEventsBySession((prev) => {
						const next = { ...prev }
						for (const [sid, arr] of Object.entries(ingestedBySession)) {
							const merged = [...(next[sid] ?? []), ...arr]
							if (merged.length > 500) merged.splice(0, merged.length - 500)
							next[sid] = merged
						}
						return next
					})
				}
				if (!activeSessionId) {
					const prompting = list.find((s) => s.status === "Prompting")
					const pick = prompting ?? list[0]
					if (pick?.acp_session_id) setActiveSessionId(pick.acp_session_id)
				}
			}

			if (k === "sessionstarted" || k === "session_started" || k === "sessionswitched" || k === "session_switched") {
				const sid = sessionIdOf(payload)
				if (sid) {
					setSessions((prev) => ({ ...prev, [sid]: { acp_session_id: sid, ...payload } }))
					setActiveSessionId((cur) => cur ?? sid)
				}
			}

			if (k === "sessionended" || k === "session_ended") {
				const sid = sessionIdOf(payload)
				if (sid) {
					setSessions((prev) => {
						const next = { ...prev }
						delete next[sid]
						return next
					})
					setPendingPermissions((prev) => {
						const next: Record<string, AcpEvent> = {}
						for (const [kk, v] of Object.entries(prev)) {
							if (sessionIdOf(v.payload) !== sid) next[kk] = v
						}
						return next
					})
				}
			}

			if (k === "permissionrequest" || k === "permission_request") {
				const reqId = payload["request_id"]
				if (typeof reqId === "string") {
					setPendingPermissions((prev) => ({ ...prev, [reqId]: ev }))
				}
			}

			const sid = sessionIdOf(payload) ?? "_global"
			setEventsBySession((prev) => {
				const list = prev[sid] ? [...prev[sid]] : []
				list.push(ev)
				if (list.length > 500) list.splice(0, list.length - 500)
				return { ...prev, [sid]: list }
			})
		}

		ws.onerror = () => {
			setConn({ status: "error", error: "websocket error" })
		}

		ws.onclose = () => {
			wsRef.current = null
			setConn({ status: "closed" })
			const attempt = reconnectAttemptRef.current + 1
			reconnectAttemptRef.current = attempt
			const delay = Math.min(RECONNECT_INITIAL_MS * 2 ** (attempt - 1), RECONNECT_MAX_MS)
			window.setTimeout(() => {
				if (!wsRef.current) connect()
			}, delay)
		}
	}, [activeSessionId])

	useEffect(() => {
		connect()
		void refreshInstances()
		const t = window.setInterval(() => void refreshInstances(), 10_000)
		return () => {
			window.clearInterval(t)
			const ws = wsRef.current
			wsRef.current = null
			if (ws) detachAndClose(ws)
		}
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [])

	const selectInstance = useCallback(
		async (name: string) => {
			if (name === activeInstance) return
			try {
				const res = await fetch("/bff/acp/select", {
					method: "POST",
					credentials: "include",
					headers: { "content-type": "application/json" },
					body: JSON.stringify({ name }),
				})
				if (!res.ok) {
					console.error("acp select failed", await res.text())
					return
				}
				setActiveInstance(name)
				// Backend swapped its client; force browser reconnect to new URL.
				// Detach handlers before close so any in-flight messages from the
				// old socket don't double-dispatch into the new state.
				const ws = wsRef.current
				wsRef.current = null
				reconnectAttemptRef.current = 0
				if (ws) detachAndClose(ws)
				// Reset session view; new instance has its own state.
				setSessions({})
				setEventsBySession({})
				setActiveSessionId(null)
				setPendingPermissions({})
				connect()
			} catch (err) {
				console.error("acp select error", err)
			}
		},
		[activeInstance, connect],
	)

	const sessionList = useMemo(() => Object.values(sessions), [sessions])
	const activeEvents = useMemo(
		() => (activeSessionId ? eventsBySession[activeSessionId] ?? [] : []),
		[activeSessionId, eventsBySession],
	)
	const blocks = useMemo(() => buildBlocks(activeEvents), [activeEvents])
	const pendingForActive = useMemo(
		() =>
			activeSessionId
				? Object.values(pendingPermissions).filter((p) => sessionIdOf(p.payload) === activeSessionId)
				: [],
		[activeSessionId, pendingPermissions],
	)

	const sendPrompt = () => {
		if (!activeSessionId || !draft.trim()) return
		send({ type: "send_prompt", session_id: activeSessionId, text: draft })
		setDraft("")
	}

	const cancelActive = () => {
		if (!activeSessionId) return
		send({ type: "cancel", session_id: activeSessionId })
	}

	const endActive = () => {
		if (!activeSessionId) return
		send({ type: "end_session", session_id: activeSessionId })
	}

	const respondPermission = (requestId: string, decision: string) => {
		send({ type: "permission_response", request_id: requestId, decision })
		setPendingPermissions((prev) => {
			const next = { ...prev }
			delete next[requestId]
			return next
		})
	}

	const spawn = () => {
		const projectPath = window.prompt("project_path")
		if (!projectPath) return
		send({ type: "spawn_session", project_path: projectPath })
	}

	const bindTelegramThread = () => {
		if (!activeSessionId) return
		const raw = window.prompt(
			"Telegram thread_id (leave blank to auto-create a new forum topic):",
			"",
		)
		if (raw === null) return
		const trimmed = raw.trim()
		const payload: Record<string, unknown> = {
			type: "bind_telegram_thread",
			session_id: activeSessionId,
		}
		if (trimmed === "") {
			payload.thread_id = null
		} else {
			const n = Number(trimmed)
			if (!Number.isInteger(n) || n <= 0) {
				window.alert("thread_id must be a positive integer or blank")
				return
			}
			payload.thread_id = n
		}
		send(payload)
	}

	const scrollContainerRef = useRef<HTMLDivElement>(null)
	const messagesEndRef = useRef<HTMLDivElement>(null)
	const wasNearBottomRef = useRef(true)

	useLayoutEffect(() => {
		const el = scrollContainerRef.current
		if (!el) return
		const distance = el.scrollHeight - el.scrollTop - el.clientHeight
		wasNearBottomRef.current = distance <= NEAR_BOTTOM_PX
	})

	useLayoutEffect(() => {
		const el = scrollContainerRef.current
		if (!el) return
		if (wasNearBottomRef.current) {
			messagesEndRef.current?.scrollIntoView({ block: "end" })
		}
	}, [blocks, pendingForActive])

	useLayoutEffect(() => {
		const el = scrollContainerRef.current
		if (!el) return
		el.scrollTop = el.scrollHeight
		wasNearBottomRef.current = true
	}, [activeSessionId])

	const active = activeSessionId ? sessions[activeSessionId] : undefined
	const statusDot =
		conn.status === "open" ? "fill-emerald-500 text-emerald-500" :
		conn.status === "connecting" ? "fill-amber-500 text-amber-500 animate-pulse" :
		"fill-red-500 text-red-500"
	const sessionStatusColor = (s?: string) =>
		s === "Prompting" ? "text-amber-500" :
		s === "Idle" ? "text-emerald-500" :
		s === "Error" ? "text-red-500" :
		"text-muted-foreground"

	return (
		<div className="relative flex h-[calc(100dvh-49px)]">
			{/* Sidebar */}
			<aside className="z-40 flex w-72 flex-col border-r border-border bg-background md:bg-muted/20">
				<div className="flex items-center justify-between px-4 py-3 border-b border-border">
					<span className="font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground">
						Sessions
					</span>
					<div className="flex items-center gap-2">
						<Circle className={cn("size-2", statusDot)} aria-label={conn.status} />
						<button
							type="button"
							onClick={spawn}
							className="text-muted-foreground hover:text-foreground"
							aria-label="Spawn session"
							title="Spawn headless session"
						>
							<Plus className="size-4" />
						</button>
					</div>
				</div>
				{instances.length > 0 && (
					<div className="border-b border-border px-3 py-2">
						<label className="block font-mono text-[9px] font-bold uppercase tracking-[2px] text-muted-foreground mb-1">
							ACP instance
						</label>
						<select
							value={activeInstance ?? ""}
							onChange={(e) => void selectInstance(e.target.value)}
							className="w-full rounded-md border border-input bg-background px-2 py-1 text-xs"
						>
							{!activeInstance && <option value="">— pick instance —</option>}
							{instances.map((inst) => (
								<option key={inst.name} value={inst.name}>
									{inst.name} ({inst.host}:{inst.port})
								</option>
							))}
						</select>
					</div>
				)}
				{conn.error && (
					<div className="border-b border-border px-4 py-2 text-[11px] text-red-500">
						{conn.error}
					</div>
				)}
				<ul className="flex-1 overflow-y-auto py-2">
					{sessionList.length === 0 && (
						<li className="px-4 py-3 text-xs text-muted-foreground italic">
							No active sessions
						</li>
					)}
					{sessionList.map((s) => {
						const isActive = activeSessionId === s.acp_session_id
						const projectName = s.project_path?.split("/").pop() ?? "(no path)"
						return (
							<li key={s.acp_session_id} className="group/row relative">
								<button
									type="button"
									onClick={() => setActiveSessionId(s.acp_session_id)}
									className={cn(
										"flex w-full items-start gap-2 px-4 py-2 text-left text-sm transition-colors",
										isActive
											? "bg-primary/10 text-primary"
											: "text-foreground hover:bg-muted/40",
									)}
								>
									<Bot className="size-3.5 shrink-0 mt-0.5" />
									<div className="flex flex-col min-w-0 flex-1">
										<span className="truncate text-sm font-medium">{projectName}</span>
										<span className="truncate text-[10px] font-mono text-muted-foreground">
											{s.acp_session_id.slice(0, 8)} · {s.agent_command ?? ""}
										</span>
										<span className={cn("text-[10px]", sessionStatusColor(s.status))}>
											{s.status ?? "—"}
										</span>
									</div>
								</button>
							</li>
						)
					})}
				</ul>
			</aside>

			{/* Thread */}
			<section className="flex min-w-0 flex-1 flex-col">
				{!activeSessionId ? (
					<div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
						Select or spawn a session
					</div>
				) : (
					<>
						<header className="flex items-center gap-2 border-b border-border px-3 py-2 md:px-6 md:py-3">
							<Bot className="size-4 shrink-0 text-muted-foreground" />
							<div className="flex min-w-0 flex-1 flex-col">
								<span className="truncate text-sm font-medium">
									{active?.project_path ?? activeSessionId}
								</span>
								<span className="truncate text-[10px] font-mono text-muted-foreground">
									{activeSessionId} · {active?.agent_command ?? ""}
									<span className={cn("ml-2", sessionStatusColor(active?.status))}>
										{active?.status ?? ""}
									</span>
								</span>
							</div>
							<button
								type="button"
								onClick={bindTelegramThread}
								className={cn(
									"flex size-8 items-center justify-center rounded-md hover:bg-muted/40 hover:text-foreground",
									active?.thread_id != null && active.thread_id > 0
										? "text-emerald-500"
										: "text-muted-foreground",
								)}
								title={
									active?.thread_id != null && active.thread_id > 0
										? `Bound to Telegram thread ${active.thread_id} (click to rebind)`
										: "Bind to Telegram thread"
								}
								aria-label="Bind Telegram thread"
							>
								<Link2 className="size-4" />
							</button>
							<button
								type="button"
								onClick={cancelActive}
								className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
								title="Cancel current prompt"
								aria-label="Cancel"
							>
								<Square className="size-4" />
							</button>
							<button
								type="button"
								onClick={endActive}
								className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
								title="End session"
								aria-label="End session"
							>
								<X className="size-4" />
							</button>
						</header>

						{pendingForActive.length > 0 && (
							<div className="border-b border-border bg-amber-500/10 px-4 py-3 flex flex-col gap-2">
								{pendingForActive.map((p) => {
									const reqId = String(p.payload["request_id"] ?? "")
									const tool = String(p.payload["tool"] ?? "?")
									return (
										<div key={reqId} className="text-xs flex items-center gap-2 flex-wrap">
											<span>Permission requested for <code className="rounded bg-background/60 px-1 py-0.5 font-mono">{tool}</code></span>
											<button onClick={() => respondPermission(reqId, "allow_once")} className="rounded-md border border-border px-2 py-0.5 text-[11px] hover:bg-emerald-500/10 hover:border-emerald-500/40">allow once</button>
											<button onClick={() => respondPermission(reqId, "allow_always")} className="rounded-md border border-border px-2 py-0.5 text-[11px] hover:bg-emerald-500/10 hover:border-emerald-500/40">allow always</button>
											<button onClick={() => respondPermission(reqId, "deny")} className="rounded-md border border-border px-2 py-0.5 text-[11px] hover:bg-red-500/10 hover:border-red-500/40">deny</button>
											<button onClick={() => respondPermission(reqId, "deny_always")} className="rounded-md border border-border px-2 py-0.5 text-[11px] hover:bg-red-500/10 hover:border-red-500/40">deny always</button>
										</div>
									)
								})}
							</div>
						)}

						<div
							ref={scrollContainerRef}
							className="flex-1 overflow-y-auto px-3 py-3 md:px-6 md:py-4"
						>
							{blocks.length === 0 && (
								<div className="text-xs text-muted-foreground italic">No events yet</div>
							)}
							{blocks.map((b) => (
								<BlockView key={b.key} block={b} sessionAgent={active?.agent_command} />
							))}
							<div ref={messagesEndRef} />
						</div>

						<form
							className="border-t border-border p-4"
							onSubmit={(e) => {
								e.preventDefault()
								sendPrompt()
							}}
						>
							<div className="flex items-end gap-2 rounded-lg border border-input bg-background p-2">
								<textarea
									value={draft}
									onChange={(e) => setDraft(e.target.value)}
									onKeyDown={(e) => {
										if (e.key === "Enter" && !e.shiftKey) {
											e.preventDefault()
											sendPrompt()
										}
									}}
									placeholder={`Message session ${activeSessionId.slice(0, 8)}…`}
									rows={1}
									className="flex-1 resize-none bg-transparent px-2 py-1.5 text-sm outline-none"
								/>
								<button
									type="submit"
									disabled={!draft.trim() || conn.status !== "open"}
									className={cn(
										"flex size-9 items-center justify-center rounded-md transition-colors",
										draft.trim() && conn.status === "open"
											? "bg-primary text-primary-foreground hover:bg-primary/90"
											: "bg-muted text-muted-foreground",
									)}
									aria-label="Send"
								>
									{conn.status !== "open" ? (
										<Loader2 className="size-4 animate-spin" />
									) : (
										<Send className="size-4" />
									)}
								</button>
							</div>
						</form>
					</>
				)}
			</section>
		</div>
	)
}

// --------------------------------------------------------------------------
// Event → Block transform + rendering
// --------------------------------------------------------------------------

type Block =
	| { key: string; kind: "user"; text: string; ts: number }
	| { key: string; kind: "assistant"; text: string; ts: number }
	| { key: string; kind: "thought"; text: string; ts: number }
	| {
		key: string
		kind: "tool"
		toolId: string
		title: string
		toolKind?: string
		status: string
		content: string
		locations?: string[]
		ts: number
	}
	| { key: string; kind: "plan"; entries: { title: string; status?: string; depth: number }[]; ts: number }
	| { key: string; kind: "status"; status: string; ts: number }
	| { key: string; kind: "error"; text: string; ts: number }
	| { key: string; kind: "raw"; eventKind: string; payload: unknown; ts: number }

function extractText(content: unknown): string {
	if (!content) return ""
	if (typeof content === "string") return content
	if (Array.isArray(content)) {
		return content
			.map((c) => {
				if (!c || typeof c !== "object") return ""
				const o = c as { type?: string; text?: string; content?: unknown }
				if (o.type === "text" && typeof o.text === "string") return o.text
				if (o.type === "content" && o.content) return extractText(o.content)
				return ""
			})
			.join("")
	}
	if (typeof content === "object") {
		const o = content as { type?: string; text?: string; content?: unknown }
		if (o.type === "text" && typeof o.text === "string") return o.text
		if (o.content) return extractText(o.content)
	}
	return ""
}

function buildBlocks(events: AcpEvent[]): Block[] {
	const blocks: Block[] = []
	const toolIndex: Record<string, number> = {}
	let assistantBuf: { idx: number } | null = null
	let thoughtBuf: { idx: number } | null = null

	for (const ev of events) {
		const k = ev.kind.toLowerCase()
		const ts = ev.receivedAt
		const payload = ev.payload as Record<string, unknown>

		if (k === "user_prompt" || k === "userprompt") {
			assistantBuf = null
			thoughtBuf = null
			const text = (typeof payload.text === "string" && payload.text) || extractText(payload.content)
			blocks.push({ key: `u-${ev.localSeq}`, kind: "user", text, ts })
			continue
		}

		if (k === "agent_update" || k === "agentupdate") {
			const inner = (payload.event as Record<string, unknown>) ?? payload
			const suRaw = inner.sessionUpdate
			// Per spec: sessionUpdate is an object { type: "<variant>", ...fields }.
			// Tolerate the legacy flat shape too (string variant + sibling fields).
			const su: Record<string, unknown> =
				suRaw && typeof suRaw === "object"
					? (suRaw as Record<string, unknown>)
					: (inner as Record<string, unknown>)
			const variant = typeof suRaw === "string" ? suRaw : (su.type as string) ?? ""

			if (variant === "working" || variant === "idle" || variant === "ready") {
				// Collapse consecutive status updates: replace last status block.
				const last = blocks[blocks.length - 1]
				if (last && last.kind === "status") {
					last.status = variant
					last.ts = ts
				} else {
					blocks.push({ key: `s-${ev.localSeq}`, kind: "status", status: variant, ts })
				}
				assistantBuf = null
				thoughtBuf = null
				continue
			}

			if (variant === "error") {
				const text =
					(typeof su.content === "string" && su.content) ||
					extractText(su.content) ||
					(typeof su.message === "string" ? (su.message as string) : "") ||
					"agent error"
				blocks.push({ key: `e-${ev.localSeq}`, kind: "error", text, ts })
				assistantBuf = null
				thoughtBuf = null
				continue
			}

			if (variant === "agent_message_chunk") {
				const text = extractText(su.content)
				if (assistantBuf) {
					const b = blocks[assistantBuf.idx]
					if (b.kind === "assistant") b.text += text
				} else {
					blocks.push({ key: `a-${ev.localSeq}`, kind: "assistant", text, ts })
					assistantBuf = { idx: blocks.length - 1 }
				}
				thoughtBuf = null
				continue
			}

			if (variant === "agent_thought_chunk") {
				const text = extractText(su.content)
				if (thoughtBuf) {
					const b = blocks[thoughtBuf.idx]
					if (b.kind === "thought") b.text += text
				} else {
					blocks.push({ key: `t-${ev.localSeq}`, kind: "thought", text, ts })
					thoughtBuf = { idx: blocks.length - 1 }
				}
				assistantBuf = null
				continue
			}

			if (variant === "tool_call" || variant === "tool_call_update") {
				// tool_call_update wraps mutable fields under `.fields`. Missing fields = unchanged.
				const fields: Record<string, unknown> =
					variant === "tool_call_update" && su.fields && typeof su.fields === "object"
						? (su.fields as Record<string, unknown>)
						: su
				const toolId = (su.toolCallId as string) ?? (fields.toolCallId as string) ?? `unknown-${ev.localSeq}`
				const title = (fields.title as string) ?? undefined
				const status = (fields.status as string) ?? undefined
				const content = fields.content !== undefined ? extractText(fields.content) : undefined
				const toolKind = (fields.kind as string) ?? undefined
				const locations = Array.isArray(fields.locations)
					? (fields.locations as unknown[])
							.map((l) => {
								if (typeof l === "string") return l
								if (l && typeof l === "object") {
									const o = l as { path?: string; line?: number }
									return o.path ? (o.line ? `${o.path}:${o.line}` : o.path) : ""
								}
								return ""
							})
							.filter(Boolean)
					: undefined

				if (toolIndex[toolId] !== undefined) {
					const b = blocks[toolIndex[toolId]]
					if (b.kind === "tool") {
						if (title) b.title = title
						if (status) b.status = status
						if (toolKind) b.toolKind = toolKind
						if (content) b.content = content
						if (locations && locations.length > 0) b.locations = locations
					}
				} else {
					blocks.push({
						key: `tc-${toolId}`,
						kind: "tool",
						toolId,
						title: title ?? toolId,
						toolKind,
						status: status ?? "pending",
						content: content ?? "",
						locations,
						ts,
					})
					toolIndex[toolId] = blocks.length - 1
				}
				assistantBuf = null
				thoughtBuf = null
				continue
			}

			if (variant === "plan") {
				const rawEntries = Array.isArray(su.entries) ? (su.entries as Record<string, unknown>[]) : []
				const entries = rawEntries.map((e) => ({
					title: (e.title as string) ?? (e.content as string) ?? "",
					status: e.status as string | undefined,
					depth: typeof e.depth === "number" ? (e.depth as number) : 0,
				}))
				blocks.push({ key: `p-${ev.localSeq}`, kind: "plan", entries, ts })
				assistantBuf = null
				thoughtBuf = null
				continue
			}

			blocks.push({ key: `r-${ev.localSeq}`, kind: "raw", eventKind: `agent_update/${variant}`, payload, ts })
			assistantBuf = null
			thoughtBuf = null
			continue
		}

		if (
			k === "snapshot" ||
			k === "state_snapshot" ||
			k === "commands_snapshot" ||
			k === "session_started" || k === "sessionstarted" ||
			k === "session_switched" || k === "sessionswitched" ||
			k === "session_ended" || k === "sessionended" ||
			k === "permission_request" || k === "permissionrequest"
		) {
			// Lifecycle / control events — surfaced elsewhere in the UI; skip in the chat log.
			continue
		}

		blocks.push({ key: `r-${ev.localSeq}`, kind: "raw", eventKind: ev.kind, payload, ts })
		assistantBuf = null
		thoughtBuf = null
	}

	return blocks
}

function timeOf(ts: number): string {
	return new Date(ts).toLocaleTimeString()
}

function BlockView({ block, sessionAgent }: { block: Block; sessionAgent?: string }) {
	if (block.kind === "user") {
		return (
			<div className="mb-3 flex gap-3">
				<div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-secondary text-secondary-foreground">
					<User2 className="size-4" />
				</div>
				<div className="min-w-0 flex-1">
					<div className="flex items-baseline gap-2">
						<span className="font-semibold text-sm">you</span>
						<span className="text-[10px] uppercase tracking-wide text-muted-foreground">human</span>
						<span className="text-[10px] text-muted-foreground">{timeOf(block.ts)}</span>
					</div>
					<div className="break-words text-sm text-foreground">
						<MessageMarkdown text={block.text} knownUsers={EMPTY_USERS} />
					</div>
				</div>
			</div>
		)
	}
	if (block.kind === "assistant") {
		return (
			<div className="mb-3 flex gap-3">
				<div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
					<Bot className="size-4" />
				</div>
				<div className="min-w-0 flex-1">
					<div className="flex items-baseline gap-2">
						<span className="font-semibold text-sm">{sessionAgent ?? "agent"}</span>
						<span className="text-[10px] uppercase tracking-wide text-muted-foreground">agent</span>
						<span className="text-[10px] text-muted-foreground">{timeOf(block.ts)}</span>
					</div>
					<div className="break-words text-sm text-foreground">
						{block.text ? (
							<MessageMarkdown text={block.text} knownUsers={EMPTY_USERS} />
						) : (
							<span className="text-muted-foreground italic">…</span>
						)}
					</div>
				</div>
			</div>
		)
	}
	if (block.kind === "thought") {
		return (
			<div className="mb-3 flex gap-3">
				<div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
					<Bot className="size-4" />
				</div>
				<div className="min-w-0 flex-1">
					<details>
						<summary className="cursor-pointer select-none flex items-baseline gap-2">
							<span className="font-semibold text-sm">{sessionAgent ?? "agent"}</span>
							<span className="text-[10px] uppercase tracking-wide text-muted-foreground">thought</span>
							<span className="text-[10px] text-muted-foreground">{timeOf(block.ts)}</span>
						</summary>
						<div className="mt-1 italic text-sm text-muted-foreground">
							<MessageMarkdown text={block.text} knownUsers={EMPTY_USERS} />
						</div>
					</details>
				</div>
			</div>
		)
	}
	if (block.kind === "tool") {
		const statusColor =
			block.status === "completed" ? "text-emerald-500" :
			block.status === "failed" || block.status === "error" ? "text-red-500" :
			block.status === "in_progress" ? "text-amber-500" :
			"text-muted-foreground"
		const statusBg =
			block.status === "completed" ? "bg-emerald-500/10 border-emerald-500/30" :
			block.status === "failed" || block.status === "error" ? "bg-red-500/10 border-red-500/30" :
			block.status === "in_progress" ? "bg-amber-500/10 border-amber-500/30" :
			"bg-muted/40 border-border"
		return (
			<div className="mb-3 rounded-md border border-border bg-muted/20 px-3 py-2.5">
				<div className="flex items-center gap-2 flex-wrap">
					<span className="rounded bg-background px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wide text-muted-foreground border border-border">
						{block.toolKind ?? "tool"}
					</span>
					<span className="font-medium text-sm truncate">{block.title}</span>
					<span className={cn("ml-auto rounded px-1.5 py-0.5 text-[10px] font-medium border", statusBg, statusColor)}>
						{block.status}
					</span>
				</div>
				{block.locations && block.locations.length > 0 && (
					<div className="mt-2 text-[10px] text-muted-foreground font-mono truncate">
						{block.locations.join(" · ")}
					</div>
				)}
				{block.content && (
					<pre className="mt-2 whitespace-pre-wrap break-words text-[11px] text-zinc-100 bg-zinc-900 border border-border rounded p-2 max-h-64 overflow-auto">
						{block.content}
					</pre>
				)}
			</div>
		)
	}
	if (block.kind === "plan") {
		return (
			<div className="mb-3 rounded-md border border-border bg-muted/20 px-3 py-2.5 text-xs">
				<div className="font-semibold mb-1.5 text-sm">Plan</div>
				<ul className="space-y-1">
					{block.entries.map((e, i) => (
						<li
							key={i}
							style={{ paddingLeft: `${(e.depth ?? 0) * 16}px` }}
							className={cn(
								"flex items-start gap-2",
								e.status === "completed" && "line-through text-muted-foreground",
							)}
						>
							<span className="font-mono text-[10px] text-muted-foreground w-20 flex-shrink-0">{e.status ?? "pending"}</span>
							<span>{e.title}</span>
						</li>
					))}
				</ul>
			</div>
		)
	}
	if (block.kind === "status") {
		return (
			<div className="mb-3 flex items-center gap-2 text-[11px] text-muted-foreground">
				<span className={cn(
					"inline-block size-1.5 rounded-full",
					block.status === "working" ? "bg-amber-500 animate-pulse" :
					block.status === "idle" || block.status === "ready" ? "bg-emerald-500" :
					"bg-muted-foreground",
				)} />
				<span className="font-mono uppercase tracking-wide">{block.status}</span>
				<span>· {timeOf(block.ts)}</span>
			</div>
		)
	}
	if (block.kind === "error") {
		return (
			<div className="mb-3 rounded-md border border-red-500/40 bg-red-500/10 px-3 py-2.5">
				<div className="flex items-center gap-2 mb-1">
					<span className="font-semibold text-sm text-red-600 dark:text-red-400">Error</span>
					<span className="text-[10px] text-muted-foreground">{timeOf(block.ts)}</span>
				</div>
				<div className="text-sm text-foreground whitespace-pre-wrap break-words">{block.text}</div>
			</div>
		)
	}
	return (
		<details className="mb-3 text-xs text-muted-foreground border-l-2 border-border pl-2">
			<summary className="cursor-pointer">{block.eventKind} · {timeOf(block.ts)}</summary>
			<pre className="whitespace-pre-wrap break-words mt-1 text-[10px]">{JSON.stringify(block.payload, null, 2)}</pre>
		</details>
	)
}
