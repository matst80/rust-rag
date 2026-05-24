import { memo } from "react"
import { Circle, Plus, Hash, X } from "lucide-react"
import { cn } from "@/lib/utils"
import { 
    SessionInfo, 
    ConnectionState, 
    WorkerStatus, 
    AcpInstance 
} from "./types"

interface SessionSidebarProps {
	sessions: Record<string, SessionInfo>
	activeSessionId: string | null
	onSelectSession: (id: string) => void
	onSpawn: () => void
	onClose: () => void
	conn: ConnectionState
	workers: WorkerStatus[]
	instances: AcpInstance[]
	activeInstance: string | null
	onSelectInstance: (name: string) => void
	sidebarOpen: boolean
}

export const SessionSidebar = memo(function SessionSidebar({
	sessions,
	activeSessionId,
	onSelectSession,
	onSpawn,
	onClose,
	conn,
	workers,
	instances,
	activeInstance,
	onSelectInstance,
	sidebarOpen,
}: SessionSidebarProps) {
	const sessionList = Object.values(sessions)
	const statusDot =
		conn.status === "open" ? "fill-emerald-500 text-emerald-500" :
			conn.status === "connecting" ? "fill-amber-500 text-amber-500 animate-pulse" :
				"fill-muted-foreground text-muted-foreground"

	return (
		<aside className={cn(
			"z-50 flex w-72 flex-col border-r border-border bg-background md:bg-muted/20 transition-transform duration-300 ease-in-out",
			"fixed inset-y-0 left-0 h-screen md:relative md:h-auto md:translate-x-0",
			sidebarOpen ? "translate-x-0" : "-translate-x-full md:hidden",
		)}>
			<div className="flex items-center justify-between px-4 py-3 border-b border-border">
				<span className="font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground">
					Sessions
				</span>
				<div className="flex items-center gap-2">
					<Circle className={cn("size-2", statusDot)} aria-label={conn.status} />
					<button
						type="button"
						onClick={onSpawn}
						className="text-muted-foreground hover:text-foreground"
						aria-label="Spawn session"
						title="Spawn headless session"
					>
						<Plus className="size-4" />
					</button>
					<button
						type="button"
						onClick={onClose}
						className="text-muted-foreground hover:text-foreground md:hidden"
						aria-label="Close sidebar"
					>
						<X className="size-4" />
					</button>
				</div>
			</div>

			{instances.length > 1 && (
				<div className="border-b border-border bg-muted/30 px-3 py-2">
					<div className="flex flex-col gap-1">
						<label className="font-mono text-[9px] uppercase tracking-wider text-muted-foreground/60">
							Instance
						</label>
						<div className="flex flex-wrap gap-1">
							{instances.map((inst) => (
								<button
									key={inst.name}
									onClick={() => onSelectInstance(inst.name)}
									className={cn(
										"rounded border px-1.5 py-0.5 font-mono text-[10px] transition-colors",
										activeInstance === inst.name
											? "border-primary/50 bg-primary/10 text-primary"
											: "border-border/50 bg-background text-muted-foreground hover:border-border hover:text-foreground"
									)}
								>
									{inst.name}
								</button>
							))}
						</div>
					</div>
				</div>
			)}

			<ul className="flex-1 overflow-y-auto py-2 no-scrollbar">
				{sessionList.length === 0 && (
					<li className="px-4 py-8 text-center text-xs text-muted-foreground italic">
						No active sessions
					</li>
				)}
				{sessionList.map((s) => {
					const isActive = s.acp_session_id === activeSessionId
					const statusColor =
						s.status === "Working" ? "bg-amber-500 animate-pulse" :
							s.status === "Idle" ? "bg-emerald-500" :
								"bg-muted-foreground"

					return (
						<li key={s.acp_session_id} className="group/row relative px-2">
							<button
								type="button"
								onClick={() => onSelectSession(s.acp_session_id)}
								className={cn(
									"flex w-full items-center gap-3 rounded-md px-3 py-2 text-left transition-colors",
									isActive
										? "bg-primary/10 text-primary shadow-sm"
										: "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
								)}
							>
								<div className={cn("size-1.5 shrink-0 rounded-full", statusColor)} />
								<div className="min-w-0 flex-1">
									<div className="truncate text-xs font-semibold leading-tight">
										{s.name || s.project_path?.split("/").pop() || s.acp_session_id.slice(0, 8)}
									</div>
									<div className="truncate font-mono text-[9px] opacity-60">
										{s.project_path}
									</div>
								</div>
								{s.thread_id ? (
									<Hash className="size-3 shrink-0 opacity-20" />
								) : null}
							</button>
						</li>
					)
				})}
			</ul>
		</aside>
	)
})
