import { useCallback, useEffect, useRef, useState } from "react"
import { 
    ConnectionState, 
    AcpEvent, 
    SessionInfo, 
    TerminalInfo, 
    AcpInstance, 
    WorkerStatus,
    ProjectInfo,
    EMPTY_ARRAY
} from "./types"
import { envelopeKind, detachAndClose, sessionIdOf } from "./utils"

const RECONNECT_INITIAL_MS = 1000
const RECONNECT_MAX_MS = 30000

export function useAcpSocket() {
	const [conn, setConn] = useState<ConnectionState>({ status: "connecting" })
	const [sessions, setSessions] = useState<Record<string, SessionInfo>>({})
	const [eventsBySession, setEventsBySession] = useState<Record<string, AcpEvent[]>>({})
	const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
	const [terminals, setTerminals] = useState<Record<string, TerminalInfo>>({})
	const [sessionTerminals, setSessionTerminals] = useState<Record<string, string[]>>({})
	const [activeTerminalId, setActiveTerminalId] = useState<Record<string, string | null>>({})
	const [viewMode, setViewMode] = useState<Record<string, "chat" | "terminal">>({})
	const [pendingPermissions, setPendingPermissions] = useState<Record<string, AcpEvent>>({})
	const [instances, setInstances] = useState<AcpInstance[]>([])
	const [activeInstance, setActiveInstance] = useState<string | null>(null)
	const [workers, setWorkers] = useState<WorkerStatus[]>([])
	const [projects, setProjects] = useState<ProjectInfo[]>([])
	const [drafts, setDrafts] = useState<Record<string, string>>({})
	const [sidebarOpen, setSidebarOpen] = useState(true)
	const [isDesktop, setIsDesktop] = useState(false)

	const wsRef = useRef<WebSocket | null>(null)
	const reconnectAttemptRef = useRef(0)
	const seqRef = useRef(0)
	const activeInstanceRef = useRef<string | null>(null)
	const workersRef = useRef<WorkerStatus[]>([])

	useEffect(() => {
		activeInstanceRef.current = activeInstance
	}, [activeInstance])

	useEffect(() => {
		workersRef.current = workers
	}, [workers])

	const send = useCallback((envelope: Record<string, unknown>) => {
		const ws = wsRef.current
		if (!ws || ws.readyState !== WebSocket.OPEN) {
			console.warn("ACP: socket not open, cannot send", envelope)
			return false
		}
		ws.send(JSON.stringify(envelope))
		return true
	}, [])

	const connect = useCallback(async () => {
		const target = activeInstanceRef.current
		const ws_count = workersRef.current.length
		if (ws_count === 0) {
			setConn({ status: "disabled", error: "no ACP instances registered" })
			return
		}
		if (!target && ws_count > 1) {
			setConn({ status: "disabled", error: "multiple instances; select one" })
			return
		}

		if (wsRef.current) detachAndClose(wsRef.current)

		const protocol = window.location.protocol === "https:" ? "wss:" : "ws:"
		const host = window.location.host
		const url = target 
            ? `${protocol}//${host}/api/acp/ws?instance=${encodeURIComponent(target)}`
            : `${protocol}//${host}/api/acp/ws`

		setConn({ status: "connecting" })
		const ws = new WebSocket(url)
		wsRef.current = ws

		ws.onopen = () => {
			setConn({ status: "open" })
			reconnectAttemptRef.current = 0
		}

		ws.onmessage = (ev) => {
			let envelope: Record<string, unknown>
			try {
				envelope = JSON.parse(ev.data as string)
			} catch (err) {
				console.error("ACP: failed to parse JSON", err)
				return
			}

			const parsed = envelopeKind(envelope)
			if (!parsed) return
			const { kind, payload } = parsed
			seqRef.current += 1
			const event: AcpEvent = {
				kind,
				payload,
				receivedAt: Date.now(),
				localSeq: seqRef.current,
			}

			const k = kind.toLowerCase()

			if (k === "terminal_created" || k === "terminalcreated") {
				const tinfo = (payload["terminal"] as TerminalInfo) ?? (payload as unknown as TerminalInfo)
				const tid = tinfo.terminal_id
				const sid = (payload["session_id"] as string | undefined) ?? tinfo.session_id
				if (tid) {
					setTerminals((prev) => ({ ...prev, [tid]: tinfo }))
					if (sid) {
						setSessionTerminals((prev) => ({
							...prev,
							[sid]: [...(prev[sid] ?? []).filter((id) => id !== tid), tid],
						}))
						setActiveTerminalId((prev) => ({ ...prev, [sid]: tid }))
						setViewMode((prev) => ({ ...prev, [sid]: "terminal" }))
					}
				}
			}

			if (k === "terminal_output" || k === "terminaloutput") {
				const tid = payload["terminal_id"] as string
				const data = payload["data"] as string
				if (tid && data) {
					window.dispatchEvent(new CustomEvent(`acp:term:output:${tid}`, { detail: data }))
				}
			}

			if (k === "terminal_snapshot" || k === "terminalsnapshot") {
				const tid = payload["terminal_id"] as string
				const data = payload["data"] as string
				const cols = payload["cols"] as number
				const rows = payload["rows"] as number
				if (tid && data) {
					window.dispatchEvent(new CustomEvent(`acp:term:snapshot:${tid}`, { 
						detail: { data, cols, rows } 
					}))
				}
			}

			if (k === "terminal_resized" || k === "terminalresized") {
				const tid = payload["terminal_id"] as string
				const cols = payload["cols"] as number
				const rows = payload["rows"] as number
				if (tid && cols && rows) {
					window.dispatchEvent(new CustomEvent(`acp:term:resized:${tid}`, { 
						detail: { cols, rows } 
					}))
				}
			}

			if (k === "terminal_closed" || k === "terminalclosed") {
				const tid = payload["terminal_id"] as string
				if (tid) {
					setTerminals((prev) => {
						const next = { ...prev }
						delete next[tid]
						return next
					})
					setSessionTerminals((prev) => {
						const next = { ...prev }
						for (const sid in next) {
							next[sid] = (next[sid] || []).filter(id => id !== tid)
						}
						return next
					})
				}
			}

			if (k === "state_snapshot" || (k === "snapshot" && Array.isArray((payload as any).sessions))) {
				const list = (payload as any).sessions as SessionInfo[]
				const terminalList = (payload as any).terminals as TerminalInfo[] ?? []
				
				const sessMap: Record<string, SessionInfo> = {}
				for (const s of list) sessMap[s.acp_session_id] = s
				setSessions(sessMap)

				const termMap: Record<string, TerminalInfo> = {}
				const sessionTerms: Record<string, string[]> = {}
				for (const t of terminalList) {
					termMap[t.terminal_id] = t
					let sid = t.session_id
					if (!sid && t.cwd) {
						// Try to match orphaned terminal to a session by path
						const match = list.find(s => s.project_path === t.cwd)
						if (match) sid = match.acp_session_id
					}

					if (sid) {
						if (!sessionTerms[sid]) sessionTerms[sid] = []
						sessionTerms[sid].push(t.terminal_id)
					}
				}
				setTerminals(termMap)
				setSessionTerminals(sessionTerms)
				setActiveTerminalId((prev) => {
					const next = { ...prev }
					for (const [sid, tids] of Object.entries(sessionTerms)) {
						if (!next[sid] && tids.length > 0) next[sid] = tids[0]
					}
					return next
				})

				const projects = (payload as any).projects as ProjectInfo[] ?? []
				setProjects(projects)
			}

			if (k === "session_started" || k === "sessionstarted") {
				const info = payload as unknown as SessionInfo
				setSessions((prev) => ({ ...prev, [info.acp_session_id]: info }))
				setActiveSessionId(info.acp_session_id)
			}

			if (k === "session_ended" || k === "sessionended") {
				const sid = sessionIdOf(payload)
				if (sid) {
					setSessions((prev) => {
						const next = { ...prev }
						delete next[sid]
						return next
					})
					setEventsBySession((prev) => {
						const next = { ...prev }
						delete next[sid]
						return next
					})
					try {
						window.localStorage.removeItem(`acp:draft:${sid}`)
					} catch {
						// ignore
					}
				}
			}

			if (k === "agent_update" || k === "agentupdate") {
				const sid = sessionIdOf(payload)
				if (sid) {
					const inner = (payload.event as Record<string, unknown>) ?? payload
					const suRaw = inner.sessionUpdate
					const variant = typeof suRaw === "string" ? suRaw : ((suRaw as Record<string, unknown>)?.type as string) ?? ""

					if (variant === "working" || variant === "agent_message_chunk" || variant === "agent_thought_chunk" || variant === "tool_call") {
						setSessions((prev) => {
							const s = prev[sid]
							if (!s || s.status === "Working") return prev
							return { ...prev, [sid]: { ...s, status: "Working" } }
						})
					} else if (variant === "idle" || variant === "ready") {
						setSessions((prev) => {
							const s = prev[sid]
							if (!s || s.status === "Idle") return prev
							return { ...prev, [sid]: { ...s, status: "Idle" } }
						})
					}
					setEventsBySession((prev) => {
						const list = prev[sid] || []
						return { ...prev, [sid]: [...list, event] }
					})
				}
			}

			if (k === "user_prompt" || k === "userprompt") {
				const sid = sessionIdOf(payload)
				if (sid) {
					setEventsBySession((prev) => {
						const list = prev[sid] || []
						return { ...prev, [sid]: [...list, event] }
					})
				}
			}

			if (k === "permission_request" || k === "permissionrequest") {
				const rid = payload["request_id"] as string
				if (rid) {
					setPendingPermissions((prev) => ({ ...prev, [rid]: event }))
				}
			}

			if (k === "permission_resolved" || k === "permissionresolved") {
				const rid = payload["request_id"] as string
				if (rid) {
					setPendingPermissions((prev) => {
						const next = { ...prev }
						delete next[rid]
						return next
					})
				}
			}
		}

		ws.onerror = (err) => {
			console.error("ACP: websocket error", err)
			setConn({ status: "error", error: "websocket connection failed" })
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
	}, [])

	const refreshInstances = useCallback(async () => {
		try {
			const res = await fetch("/bff/acp/instances", { credentials: "include" })
			if (!res.ok) return
			const data = await res.json()
			setInstances(data.instances || [])
			setWorkers(data.workers || [])
			// Optimistically set active if not set
			if (data.active && activeInstanceRef.current === null) {
				setActiveInstance(data.active)
			}
		} catch (err) {
			console.warn("ACP: failed to refresh instances", err)
		}
	}, [])

	const selectInstance = async (name: string) => {
		try {
			setActiveInstance(name)
			const res = await fetch("/bff/acp/select", {
				method: "POST",
				headers: { "content-type": "application/json" },
				body: JSON.stringify({ name }),
			})
			if (res.ok) {
				if (wsRef.current) detachAndClose(wsRef.current)
				connect()
			}
		} catch (err) {
			console.error("ACP: failed to select instance", err)
		}
	}

	const setDraft = useCallback((sid: string, text: string) => {
		setDrafts((prev) => ({ ...prev, [sid]: text }))
		try {
			window.localStorage.setItem(`acp:draft:${sid}`, text)
		} catch {
			// ignore
		}
	}, [])

	useEffect(() => {
		const isD = window.innerWidth >= 768
		setIsDesktop(isD)
		setSidebarOpen(isD)
	}, [])

	useEffect(() => {
		if (typeof window !== "undefined" && window.innerWidth < 768) {
			setSidebarOpen(false)
		}
	}, [activeSessionId])

	useEffect(() => {
		const loaded: Record<string, string> = {}
		for (let i = 0; i < window.localStorage.length; i++) {
			const key = window.localStorage.key(i)
			if (key?.startsWith("acp:draft:")) {
				const sid = key.slice("acp:draft:".length)
				loaded[sid] = window.localStorage.getItem(key) || ""
			}
		}
		setDrafts(loaded)
	}, [])

	useEffect(() => {
		if (conn.status === "open" || conn.status === "connecting") return
		const resolvable = (workers.length === 1 && !activeInstance) || !!activeInstance
		if (!resolvable) return
		connect()
	}, [workers, activeInstance, connect, conn.status])

	useEffect(() => {
		; (async () => {
			await refreshInstances()
			connect()
		})()
		const t = window.setInterval(() => void refreshInstances(), 10_000)
		return () => {
			window.clearInterval(t)
			if (wsRef.current) detachAndClose(wsRef.current)
		}
	}, [refreshInstances, connect])

	return {
		conn,
		sessions,
		eventsBySession,
		activeSessionId,
		setActiveSessionId,
		terminals,
		sessionTerminals,
		activeTerminalId,
		setActiveTerminalId,
		viewMode,
		setViewMode,
		pendingPermissions,
		instances,
		activeInstance,
		selectInstance,
		workers,
		projects,
		send,
		drafts,
		setDraft,
		sidebarOpen,
		setSidebarOpen,
		isDesktop,
	}
}
