import { memo } from "react"
import { Circle, Plus, Hash, X, Terminal, ChevronRight } from "lucide-react"
import { cn } from "@/lib/utils"
import { 
    SessionInfo, 
    ConnectionState, 
    WorkerStatus, 
    AcpInstance,
    TerminalInfo
} from "./types"

interface SessionSidebarProps {
	sessions: Record<string, SessionInfo>
	activeSessionId: string | null
	activeStandaloneTerminalId: string | null
	onSelectSession: (id: string) => void
	onSpawn: () => void
	onNewTerminal?: () => void
	onClose: () => void
	onCreateTerminal: (sessionId: string) => void
	onSelectTerminal: (sessionId: string, terminalId: string) => void
	terminals: Record<string, TerminalInfo>
	sessionTerminals: Record<string, string[]>
	activeTerminalId: Record<string, string | null>
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
	activeStandaloneTerminalId,
	onSelectSession,
	onSpawn,
	onNewTerminal,
	onClose,
	onCreateTerminal,
	onSelectTerminal,
	terminals,
	sessionTerminals,
	activeTerminalId,
	conn,
	workers,
	instances,
	activeInstance,
	onSelectInstance,
	sidebarOpen,
}: SessionSidebarProps) {
	const sessionList = Object.values(sessions)
	const activeRemote = instances.find((instance) => instance.name === activeInstance)
		?? (instances.length === 1 ? instances[0] : undefined)
	const remoteLabel = activeRemote?.name || activeInstance || "Remote terminal"
	const statusDot =
		conn.status === "open" ? "fill-emerald-500 text-emerald-500" :
			conn.status === "connecting" ? "fill-amber-500 text-amber-500 animate-pulse" :
				"fill-muted-foreground text-muted-foreground"

	return (
		<aside className={cn(
			"z-50 flex w-60 flex-col border-r border-border bg-background md:bg-muted/20 transition-transform duration-300 ease-in-out",
			"fixed inset-y-0 left-0 h-screen md:relative md:h-auto md:translate-x-0",
			sidebarOpen ? "translate-x-0" : "-translate-x-full md:hidden",
		)}>
			<div className="flex items-center justify-between px-3 py-2.5 border-b border-border">
				<span className="font-mono text-[10px] font-bold uppercase tracking-[2px] text-foreground/70">
					Terminals
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
						onClick={onNewTerminal}
						className="text-muted-foreground hover:text-foreground"
						aria-label="New standalone terminal"
						title="New standalone terminal"
					>
						<Terminal className="size-4" />
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

			<ul className="flex-1 overflow-y-auto py-1.5 no-scrollbar">
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

					const sessionTerms = sessionTerminals[s.acp_session_id] || []

					return (
						<li key={s.acp_session_id} className="group/row mb-0.5 px-1.5">
							<div className="flex items-center gap-1">
								<button
									type="button"
									onClick={() => onSelectSession(s.acp_session_id)}
									className={cn(
										"flex flex-1 items-center gap-2 rounded-md px-2.5 py-1.5 text-left transition-colors",
										isActive && !activeTerminalId[s.acp_session_id]
											? "bg-primary/10 text-primary shadow-sm"
											: "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
									)}
								>
									<div className={cn("size-1.5 shrink-0 rounded-full", statusColor)} />
									<div className="min-w-0 flex-1">
										<div className="truncate text-xs font-semibold leading-tight">
											{s.name || s.folder || s.project_path?.split("/").pop() || s.acp_session_id.slice(0, 8)}
										</div>											<div className="truncate font-mono text-[8px] opacity-50">
												{s.folder || s.project_path}
											</div>									</div>
									{s.thread_id ? (
										<Hash className="size-3 shrink-0 opacity-20" />
									) : null}
								</button>

								<button
									type="button"
									onClick={(e) => {
										e.stopPropagation()
										onCreateTerminal(s.acp_session_id)
									}}
									className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground/60 hover:bg-primary/10 hover:text-primary transition-colors opacity-100 md:opacity-0 md:group-hover/row:opacity-100"
									title="New Terminal"
								>
									<Plus className="size-3.5" />
								</button>
							</div>

							{sessionTerms.length > 0 && (
								<ul className="mt-0.5 space-y-0.5 pl-5">
									{sessionTerms.map((tid) => {
										const t = terminals[tid]
										if (!t) return null
										const isTermActive = activeTerminalId[s.acp_session_id] === tid && isActive

										return (
											<li key={tid}>
												<button
													type="button"
													onClick={() => onSelectTerminal(s.acp_session_id, tid)}
													className={cn(
														"flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors group/term",
														isTermActive
															? "bg-primary/5 text-primary shadow-sm"
															: "text-muted-foreground/60 hover:bg-muted/40 hover:text-foreground"
													)}
												>
													<Terminal className={cn("size-3 shrink-0", isTermActive ? "text-primary" : "opacity-40")} />
													<div className="min-w-0 flex-1">
														<div className="truncate text-[10px] font-medium leading-tight">
															Terminal {t.terminal_id.slice(0, 6)}
														</div>
														<div className="truncate font-mono text-[8px] opacity-50">
															{t.cwd}
														</div>
													</div>
													{isTermActive && (
														<div className="size-1 rounded-full bg-primary" />
													)}
												</button>
											</li>
										)
									})}
								</ul>
							)}
						</li>
					)
				})}
			</ul>

			{/* STANDALONE TERMINALS */}
			{Object.keys(terminals).some(tid => !Object.values(sessionTerminals).some(tids => tids.includes(tid))) && (
				<div className="border-t border-border mt-auto py-2">
					<div className="px-4 py-2 flex items-center justify-between">
						<span className="font-mono text-[9px] font-bold uppercase tracking-wider text-muted-foreground/60">
							Standalone Terminals
						</span>
					</div>
					<ul className="px-2 space-y-0.5">
						{Object.entries(terminals)
							.filter(([tid]) => !Object.values(sessionTerminals).some(tids => tids.includes(tid)))
							.map(([tid, t]) => {
								const isTermActive = (activeStandaloneTerminalId === tid) && !activeSessionId

								return (
									<li key={tid}>
										<button
											type="button"
											onClick={() => onSelectTerminal("", tid)}
											className={cn(
												"flex w-full items-center gap-3 rounded-md px-3 py-2 text-left transition-colors",
												isTermActive
													? "bg-primary/10 text-primary shadow-sm"
													: "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
											)}
										>
											<Terminal className={cn("size-3.5 shrink-0", isTermActive ? "text-primary" : "opacity-40")} />
											<div className="min-w-0 flex-1">
												<div className="truncate text-[11px] font-semibold leading-tight">
													{remoteLabel}
												</div>
												<div className="truncate font-mono text-[9px] opacity-60">
													{t.cwd}
												</div>
												<div className="truncate font-mono text-[8px] opacity-35">
													Terminal {tid.slice(0, 8)}{activeRemote?.host ? ` · ${activeRemote.host}` : ""}
												</div>
											</div>
											{isTermActive && (
												<div className="size-1.5 rounded-full bg-primary" />
											)}
										</button>
									</li>
								)
							})
						}
					</ul>
				</div>
			)}
		</aside>
	)
})
