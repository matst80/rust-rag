export interface AcpEvent {
	kind: string
	payload: Record<string, unknown>
	receivedAt: number
	localSeq: number
}

export interface UserPromptHistory {
	type: "user_prompt"
	thread_id: number | null
	acp_session_id: string | null
	text: string
}

export interface AgentUpdateHistory {
	type: "agent_update"
	thread_id: number | null
	acp_session_id: string
	event: {
		type: "update"
		sessionUpdate: string | { type: string }
		content?: any
		message?: string
		toolCallId?: string
		fields?: any
		entries?: any[]
	}
}

export interface SessionInfo {
	acp_session_id: string
	project_path?: string // Legacy/Fallback
	folder?: string       // Stable project label
	thread_id: number | null
	status: string
	name?: string | null  // Display label
	agent_command: string
	agent_name?: string | null
	available_commands?: { name: string; description?: string; schema?: Record<string, unknown> }[]
	history?: (UserPromptHistory | AgentUpdateHistory | any)[]
}

export interface TerminalInfo {
	terminal_id: string
	thread_id?: number | null
	session_id?: string | null
	cwd: string
	command?: string[] | null
	cols: number
	rows: number
	running: boolean
	exit_code?: number | null
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
