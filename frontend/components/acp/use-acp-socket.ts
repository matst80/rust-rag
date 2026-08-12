import { useCallback, useEffect, useRef, useState } from "react"
import { toast } from "sonner"
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
const TERMINAL_INPUT_BATCH_MS = 12

function encodeBase64(data: string): string {
	const bytes = new TextEncoder().encode(data)
	let binary = ""
	for (let i = 0; i < bytes.length; i++) {
		binary += String.fromCharCode(bytes[i])
	}
	return btoa(binary)
}

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
	const [filePreview, setFilePreview] = useState<{ path: string; content: string; startLine: number; totalLines: number } | null>(null)
	const [suggestions, setSuggestions] = useState<{ query: string; directories?: string[]; files?: string[] } | null>(null)

	const wsRef = useRef<WebSocket | null>(null)
	const reconnectAttemptRef = useRef(0)
	const seqRef = useRef(0)
	const activeInstanceRef = useRef<string | null>(null)
	const terminalInputRef = useRef<Record<string, string>>({})
	const terminalInputTimersRef = useRef<Record<string, number>>({})
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

	const flushTerminalInput = useCallback((terminalId: string) => {
		const data = terminalInputRef.current[terminalId]
		const ws = wsRef.current
		if (!data || !ws || ws.readyState !== WebSocket.OPEN) return false

		try {
			ws.send(JSON.stringify({
				type: "terminal_input",
				terminal_id: terminalId,
				data: encodeBase64(data),
			}))
			delete terminalInputRef.current[terminalId]
			return true
		} catch (err) {
			console.warn("ACP: failed to send terminal input; retaining buffer", err)
			return false
		}
	}, [])

	const sendTerminalInput = useCallback((terminalId: string, data: string) => {
		if (!data) return true
		terminalInputRef.current[terminalId] =
			(terminalInputRef.current[terminalId] ?? "") + data

		if (terminalInputTimersRef.current[terminalId] === undefined) {
			terminalInputTimersRef.current[terminalId] = window.setTimeout(() => {
				delete terminalInputTimersRef.current[terminalId]
				flushTerminalInput(terminalId)
			}, TERMINAL_INPUT_BATCH_MS)
		}
		return true
	}, [flushTerminalInput])

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
			// Flush text buffered while the socket was reconnecting. This keeps
			// terminal input lossless without queueing control commands forever.
			for (const terminalId of Object.keys(terminalInputRef.current)) {
				flushTerminalInput(terminalId)
			}
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
					setActiveTerminalId((prev) => {
						const next = { ...prev }
						for (const sid in next) {
							if (next[sid] === tid) next[sid] = null
						}
						return next
					})
				}
			}

			if (k === "state_snapshot" || (k === "snapshot" && Array.isArray((payload as any).sessions))) {
				const list = (payload as any).sessions as SessionInfo[]
				const terminalList = (payload as any).terminals as TerminalInfo[] ?? []
				
				const sessMap: Record<string, SessionInfo> = {}
				const eventMap: Record<string, AcpEvent[]> = {}
				for (const s of list) {
					sessMap[s.acp_session_id] = s
					if (Array.isArray(s.history)) {
						eventMap[s.acp_session_id] = s.history.map((h: any, idx) => ({
							kind: h.type || h.kind || "unknown",
							payload: h,
							receivedAt: Date.now(),
							localSeq: idx + 1,
						}))
					}
				}
				setSessions(sessMap)
				setEventsBySession((prev) => ({ ...prev, ...eventMap }))

				const termMap: Record<string, TerminalInfo> = {}
				const sessionTerms: Record<string, string[]> = {}
				for (const t of terminalList) {
					termMap[t.terminal_id] = t
					let sid = t.session_id
					if (!sid && t.cwd) {
						// Try to match orphaned terminal to a session by folder or path
						const match = list.find(s => s.folder === t.cwd || s.project_path === t.cwd)
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
					for (const sid of Object.keys(next)) {
						if (!sessionTerms[sid]) next[sid] = null
					}
					for (const [sid, tids] of Object.entries(sessionTerms)) {
						if (!tids.includes(next[sid] ?? "")) next[sid] = tids[0] ?? null
					}
					return next
				})

				// Existing xterm views may have survived a websocket resync. Ask
				// them to re-attach so the daemon sends a fresh terminal snapshot.
				for (const t of terminalList) {
					window.dispatchEvent(new CustomEvent(`acp:term:resync:${t.terminal_id}`))
				}

				const projects = (payload as any).projects as ProjectInfo[] ?? []
				setProjects(projects)
			}

			if (k === "session_started" || k === "sessionstarted" || k === "session_switched" || k === "sessionswitched") {
				const info = payload as unknown as SessionInfo
				setSessions((prev) => ({ ...prev, [info.acp_session_id]: info }))
				setActiveSessionId(info.acp_session_id)
			}

			if (k === "session_ended" || k === "sessionended" || k === "session_removed" || k === "sessionremoved") {
				const sid = sessionIdOf(payload) || (payload["acp_session_id"] as string | undefined)
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
				} else if (payload["thread_id"]) {
					// Fallback for session_removed which might only have thread_id
					const tid = Number(payload["thread_id"])
					setSessions((prev) => {
						const next = { ...prev }
						for (const id in next) {
							if (next[id].thread_id === tid) {
								delete next[id]
								// Also clean up events for this id if we found it
								setEventsBySession((prevEvents) => {
									const nextEvents = { ...prevEvents }
									delete nextEvents[id]
									return nextEvents
								})
								break
							}
						}
						return next
					})
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
					} else if (variant === "idle" || variant === "ready" || variant === "finished") {
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

			if (k === "directory_suggestions" || k === "directorysuggestions") {
				const query = payload["query"] as string
				const dirs = (payload["directories"] as any[])?.map(d => d.path)
				setSuggestions(prev => ({ ...prev, query, directories: dirs }))
			}

			if (k === "find_files_result" || k === "findfilesresult") {
				const query = payload["query"] as string
				const files = payload["files"] as string[]
				setSuggestions(prev => ({ ...prev, query, files }))
			}

			if (k === "read_file_result" || k === "readfileresult") {
				const path = payload["path"] as string
				const content = payload["content"] as string
				const startLine = (payload["start_line"] as number) || 1
				const totalLines = (payload["total_lines"] as number) || 0
				setFilePreview({ path, content, startLine, totalLines })
			}

			if (k === "clipboard_updated" || k === "clipboardupdated") {
				const content = payload["content"] as string
				const source = payload["source"] as string
				const truncated = payload["truncated"] as boolean

				if (content) {
					toast.info("Clipboard Updated", {
						description: truncated ? `${content.slice(0, 100)}... (truncated)` : content.slice(0, 200),
						duration: 10000,
						action: {
							label: "Copy",
							onClick: () => {
								navigator.clipboard.writeText(content)
							}
						}
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
	}, [flushTerminalInput])

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
			// Do not replay keystrokes into a different ACP instance if the
			// user changes targets while input is buffered.
			for (const timer of Object.values(terminalInputTimersRef.current)) {
				window.clearTimeout(timer)
			}
			terminalInputTimersRef.current = {}
			terminalInputRef.current = {}
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

	const listDirectories = useCallback((sid: string | null, query: string) => {
		send({ type: "list_directories", session_id: sid, query })
	}, [send])

	const findFiles = useCallback((sid: string | null, query: string) => {
		send({ type: "find_files", session_id: sid, query })
	}, [send])

	const readFile = useCallback((sid: string | null, path: string, startLine?: number, lineCount?: number) => {
		send({ type: "read_file", session_id: sid, path, start_line: startLine, line_count: lineCount })
	}, [send])

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
			for (const timer of Object.values(terminalInputTimersRef.current)) {
				window.clearTimeout(timer)
			}
			terminalInputTimersRef.current = {}
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
		sendTerminalInput,
		drafts,
		setDraft,
		sidebarOpen,
		setSidebarOpen,
		isDesktop,
		filePreview,
		setFilePreview,
		suggestions,
		listDirectories,
		findFiles,
		readFile,
	}
}
