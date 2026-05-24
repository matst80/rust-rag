export interface AcpEvent {
	kind: string
	payload: Record<string, unknown>
	receivedAt: number
	localSeq: number
}

export interface SessionInfo {
	acp_session_id: string
	project_path?: string
	thread_id: number
	status: string
	name?: string | null
	agent_command: string
	agent_name?: string | null
	available_commands?: { name: string; description?: string; schema?: Record<string, unknown> }[]
	history?: unknown[]
}

export interface TerminalInfo {
	terminal_id: string
	cwd: string
	command: string[]
	cols: number
	rows: number
	running: boolean
}

export interface TerminalEventState {
	output?: { data: string; ts: number }
	snapshot?: { data: string; cols: number; rows: number; ts: number }
}

export interface ProjectInfo {
	name: string
	path: string
}

export interface AcpInstance {
	name: string
	host: string
	port: number
	url: string
	txt: Record<string, string>
}

export interface WorkerStatus {
	instance_id: string
	url: string
	connected: boolean
	last_error: string | null
	session_count: number
	pending_permissions: number
	buffered_events: number
}

export interface ConnectionState {
	status: "connecting" | "open" | "closed" | "error" | "disabled"
	error?: string
}

export type Block =
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

export const EMPTY_USERS: Set<string> = new Set()
export const EMPTY_ARRAY: any[] = []
