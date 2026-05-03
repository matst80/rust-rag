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
				for (const s of list) {
					if (s?.acp_session_id) map[s.acp_session_id] = s
				}
				setSessions(map)
				if (!activeSessionId && list[0]?.acp_session_id) {
					setActiveSessionId(list[0].acp_session_id)
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

						<div className="flex-1 overflow-y-auto p-3 flex flex-col gap-2 font-mono text-xs">
							{activeEvents.length === 0 && (
								<div className="text-muted-foreground italic">No events yet</div>
							)}
							{activeEvents.map((ev) => (
								<div key={ev.localSeq} className="border-l-2 border-border pl-2">
									<div className="text-[10px] text-muted-foreground uppercase tracking-wider">
										{new Date(ev.receivedAt).toLocaleTimeString()} · {ev.kind}
									</div>
									<pre className="whitespace-pre-wrap break-words text-[11px]">
										{JSON.stringify(ev.payload, null, 2)}
									</pre>
								</div>
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
