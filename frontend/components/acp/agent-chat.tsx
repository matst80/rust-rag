"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"

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

interface ConnectionState {
	status: "connecting" | "open" | "closed" | "error" | "disabled"
	error?: string
}

const RECONNECT_INITIAL_MS = 1000
const RECONNECT_MAX_MS = 30000

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
	const wsRef = useRef<WebSocket | null>(null)
	const reconnectAttemptRef = useRef(0)
	const seqRef = useRef(0)

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
			const res = await fetch("/api/acp/config", { credentials: "include" })
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

			if (k === "snapshot") {
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
		return () => {
			const ws = wsRef.current
			wsRef.current = null
			if (ws) ws.close()
		}
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [])

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
		send({ SendPrompt: { session_id: activeSessionId, text: draft } })
		setDraft("")
	}

	const cancelActive = () => {
		if (!activeSessionId) return
		send({ Cancel: { session_id: activeSessionId } })
	}

	const endActive = () => {
		if (!activeSessionId) return
		send({ EndSession: { session_id: activeSessionId } })
	}

	const respondPermission = (requestId: string, decision: string) => {
		send({ PermissionResponse: { request_id: requestId, decision } })
		setPendingPermissions((prev) => {
			const next = { ...prev }
			delete next[requestId]
			return next
		})
	}

	const spawn = () => {
		const projectPath = window.prompt("project_path")
		if (!projectPath) return
		send({ SpawnSession: { project_path: projectPath } })
	}

	return (
		<div className="grid grid-cols-[260px_1fr] h-full min-h-[600px] gap-4">
			<aside className="border border-border rounded p-3 flex flex-col gap-3 overflow-y-auto">
				<div className="flex items-center justify-between">
					<h2 className="font-mono text-xs uppercase tracking-wider">Sessions</h2>
					<button onClick={spawn} className="text-xs px-2 py-1 border border-border rounded hover:bg-accent">+ Spawn</button>
				</div>
				<div className="text-xs text-muted-foreground">
					Status: <span className={conn.status === "open" ? "text-green-500" : "text-amber-500"}>{conn.status}</span>
					{conn.error && <span className="block text-red-500 text-[10px]">{conn.error}</span>}
				</div>
				<ul className="flex flex-col gap-1">
					{sessionList.length === 0 && <li className="text-xs text-muted-foreground italic">No active sessions</li>}
					{sessionList.map((s) => (
						<li key={s.acp_session_id}>
							<button
								onClick={() => setActiveSessionId(s.acp_session_id)}
								className={`w-full text-left text-xs px-2 py-1.5 rounded border ${
									activeSessionId === s.acp_session_id ? "border-primary bg-accent" : "border-border hover:bg-accent/50"
								}`}
							>
								<div className="font-mono truncate">{s.acp_session_id.slice(0, 12)}…</div>
								<div className="text-[10px] text-muted-foreground truncate">{s.project_path ?? "(no path)"}</div>
								<div className="text-[10px] text-muted-foreground">{s.status ?? ""}</div>
							</button>
						</li>
					))}
				</ul>
			</aside>

			<section className="border border-border rounded flex flex-col overflow-hidden">
				{!activeSessionId ? (
					<div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
						Select or spawn a session
					</div>
				) : (
					<>
						<header className="border-b border-border p-3 flex items-center justify-between">
							<div className="font-mono text-xs">
								{activeSessionId}
								<span className="ml-2 text-muted-foreground">{sessions[activeSessionId]?.project_path}</span>
							</div>
							<div className="flex gap-2">
								<button onClick={cancelActive} className="text-xs px-2 py-1 border border-border rounded hover:bg-accent">Cancel</button>
								<button onClick={endActive} className="text-xs px-2 py-1 border border-border rounded hover:bg-accent">End</button>
							</div>
						</header>

						{pendingForActive.length > 0 && (
							<div className="border-b border-border bg-amber-50/10 p-3 flex flex-col gap-2">
								{pendingForActive.map((p) => {
									const reqId = String(p.payload["request_id"] ?? "")
									const tool = String(p.payload["tool"] ?? "?")
									return (
										<div key={reqId} className="text-xs flex items-center gap-2 flex-wrap">
											<span>Permission requested for <code>{tool}</code></span>
											<button onClick={() => respondPermission(reqId, "allow_once")} className="px-2 py-0.5 border border-border rounded hover:bg-accent">allow_once</button>
											<button onClick={() => respondPermission(reqId, "allow_always")} className="px-2 py-0.5 border border-border rounded hover:bg-accent">allow_always</button>
											<button onClick={() => respondPermission(reqId, "deny")} className="px-2 py-0.5 border border-border rounded hover:bg-accent">deny</button>
											<button onClick={() => respondPermission(reqId, "deny_always")} className="px-2 py-0.5 border border-border rounded hover:bg-accent">deny_always</button>
										</div>
									)
								})}
							</div>
						)}

						<div className="flex-1 overflow-y-auto p-3 flex flex-col gap-3">
							{blocks.length === 0 && (
								<div className="text-xs text-muted-foreground italic">No events yet</div>
							)}
							{blocks.map((b) => (
								<BlockView key={b.key} block={b} />
							))}
						</div>

						<footer className="border-t border-border p-3 flex gap-2">
							<input
								value={draft}
								onChange={(e) => setDraft(e.target.value)}
								onKeyDown={(e) => {
									if (e.key === "Enter" && !e.shiftKey) {
										e.preventDefault()
										sendPrompt()
									}
								}}
								placeholder="Send prompt…"
								className="flex-1 bg-background border border-border rounded px-2 py-1 text-xs"
							/>
							<button onClick={sendPrompt} className="text-xs px-3 py-1 border border-primary text-primary rounded hover:bg-primary/10">
								Send
							</button>
						</footer>
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

function BlockView({ block }: { block: Block }) {
	if (block.kind === "user") {
		return (
			<div className="flex flex-col items-end">
				<div className="max-w-[80%] rounded-lg bg-primary text-primary-foreground px-3 py-2 text-sm whitespace-pre-wrap break-words">
					{block.text}
				</div>
				<div className="text-[10px] text-muted-foreground mt-1">user · {timeOf(block.ts)}</div>
			</div>
		)
	}
	if (block.kind === "assistant") {
		return (
			<div className="flex flex-col items-start">
				<div className="max-w-[90%] rounded-lg bg-accent px-3 py-2 text-sm whitespace-pre-wrap break-words">
					{block.text || <span className="text-muted-foreground italic">…</span>}
				</div>
				<div className="text-[10px] text-muted-foreground mt-1">agent · {timeOf(block.ts)}</div>
			</div>
		)
	}
	if (block.kind === "thought") {
		return (
			<details className="text-xs text-muted-foreground border-l-2 border-border pl-3">
				<summary className="cursor-pointer select-none">thought · {timeOf(block.ts)}</summary>
				<div className="whitespace-pre-wrap break-words mt-1 italic">{block.text}</div>
			</details>
		)
	}
	if (block.kind === "tool") {
		const statusColor =
			block.status === "completed" ? "text-green-500" :
			block.status === "failed" || block.status === "error" ? "text-red-500" :
			block.status === "in_progress" ? "text-amber-500" :
			"text-muted-foreground"
		return (
			<div className="border border-border rounded px-3 py-2 text-xs">
				<div className="flex items-center gap-2 flex-wrap">
					<span className="font-mono text-muted-foreground">{block.toolKind ?? "tool"}</span>
					<span className="font-medium">{block.title}</span>
					<span className={`ml-auto ${statusColor}`}>{block.status}</span>
				</div>
				{block.locations && block.locations.length > 0 && (
					<div className="mt-1 text-[10px] text-muted-foreground font-mono truncate">
						{block.locations.join(" · ")}
					</div>
				)}
				{block.content && (
					<pre className="mt-2 whitespace-pre-wrap break-words text-[11px] text-muted-foreground max-h-64 overflow-auto">
						{block.content}
					</pre>
				)}
			</div>
		)
	}
	if (block.kind === "plan") {
		return (
			<div className="border border-border rounded px-3 py-2 text-xs">
				<div className="font-medium mb-1">Plan</div>
				<ul className="space-y-0.5">
					{block.entries.map((e, i) => (
						<li
							key={i}
							style={{ paddingLeft: `${(e.depth ?? 0) * 16}px` }}
							className={`flex items-start gap-2 ${e.status === "completed" ? "line-through text-muted-foreground" : ""}`}
						>
							<span className="font-mono text-[10px] text-muted-foreground w-20 flex-shrink-0">{e.status ?? "pending"}</span>
							<span>{e.title}</span>
						</li>
					))}
				</ul>
			</div>
		)
	}
	return (
		<details className="text-xs text-muted-foreground border-l-2 border-border pl-2">
			<summary className="cursor-pointer">{block.eventKind} · {timeOf(block.ts)}</summary>
			<pre className="whitespace-pre-wrap break-words mt-1 text-[10px]">{JSON.stringify(block.payload, null, 2)}</pre>
		</details>
	)
}
