import { useState, memo } from "react"
import { X } from "lucide-react"
import { cn } from "@/lib/utils"
import { ProjectInfo } from "./types"

interface SpawnDialogProps {
	projects: ProjectInfo[]
	onSpawn: (projectPath: string, agentCommand: string) => void
	onCancel: () => void
}

export const SpawnDialog = memo(function SpawnDialog({
	projects,
	onSpawn,
	onCancel,
}: SpawnDialogProps) {
	const [projectPath, setProjectPath] = useState("")
	const [agentCommand, setAgentCommand] = useState("")
	const [projectPickerOpen, setProjectPickerOpen] = useState(false)
	const [projectPickerHighlight, setProjectPickerHighlight] = useState(0)

	const filtered = projects.filter((p) =>
		p.path.toLowerCase().includes(projectPath.toLowerCase()) ||
		p.name.toLowerCase().includes(projectPath.toLowerCase())
	)
	const max = filtered.length

	return (
		<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4">
			<div className="w-full max-w-md border border-border bg-card p-6 shadow-2xl animate-in zoom-in-95 duration-200">
				<div className="flex items-center justify-between mb-6">
					<h2 className="font-mono text-sm font-black uppercase tracking-[3px]">
						Spawn Session
					</h2>
					<button
						type="button"
						className="text-muted-foreground hover:text-foreground"
						onClick={onCancel}
						aria-label="Close"
					>
						<X className="size-4" />
					</button>
				</div>

				<div className="mb-4 relative">
					<label className="block font-mono text-[10px] font-bold uppercase tracking-[1.5px] text-muted-foreground mb-1">
						project_path
					</label>
					<input
						id="spawn-project-path"
						name="project_path"
						type="text"
						value={projectPath}
						onChange={(e) => {
							setProjectPath(e.target.value)
							setProjectPickerOpen(true)
							setProjectPickerHighlight(0)
						}}
						onFocus={() => {
							if (projects.length > 0) setProjectPickerOpen(true)
						}}
						onBlur={() => {
							window.setTimeout(() => setProjectPickerOpen(false), 120)
						}}
						onKeyDown={(e) => {
							if (!projectPickerOpen || max === 0) return
							if (e.key === "ArrowDown") {
								e.preventDefault()
								setProjectPickerHighlight((i) => (i + 1) % max)
							} else if (e.key === "ArrowUp") {
								e.preventDefault()
								setProjectPickerHighlight((i) => (i - 1 + max) % max)
							} else if (e.key === "Enter") {
								const pick = filtered[projectPickerHighlight]
								if (pick) {
									e.preventDefault()
									setProjectPath(pick.path)
									setProjectPickerOpen(false)
								}
							} else if (e.key === "Escape") {
								setProjectPickerOpen(false)
							}
						}}
						placeholder="/abs/path or filter projects…"
						className="w-full font-mono text-xs bg-background border border-border px-2 py-2 outline-none focus:border-primary/50 transition-colors"
						autoComplete="off"
					/>
					{projectPickerOpen && filtered.length > 0 && (
						<ul className="absolute z-10 mt-1 max-h-56 w-full overflow-y-auto border border-border bg-background shadow-lg no-scrollbar">
							{filtered.map((p, i) => (
								<li
									key={p.path}
									onMouseDown={(e) => {
										e.preventDefault()
										setProjectPath(p.path)
										setProjectPickerOpen(false)
									}}
									onMouseEnter={() => setProjectPickerHighlight(i)}
									className={cn(
										"cursor-pointer px-2 py-1.5 font-mono text-xs",
										i === projectPickerHighlight
											? "bg-primary/10 text-primary"
											: "hover:bg-muted/40",
									)}
								>
									<div className="truncate font-medium">{p.name}</div>
									<div className="truncate text-[10px] text-muted-foreground">
										{p.path}
									</div>
								</li>
							))}
						</ul>
					)}
				</div>

				<div className="mb-4">
					<label className="block font-mono text-[10px] font-bold uppercase tracking-[1.5px] text-muted-foreground mb-1">
						agent_command (optional)
					</label>
					<input
						id="spawn-agent-command"
						name="agent_command"
						type="text"
						value={agentCommand}
						onChange={(e) => setAgentCommand(e.target.value)}
						placeholder="claude / gemini / amp / …"
						className="w-full font-mono text-xs bg-background border border-border px-2 py-2 outline-none focus:border-primary/50 transition-colors"
					/>
				</div>

				<div className="flex justify-end gap-2 mt-8">
					<button
						type="button"
						onClick={onCancel}
						className="font-mono text-[10px] uppercase tracking-[1.5px] px-4 py-2 border border-border hover:bg-muted/40 transition-colors"
					>
						Cancel
					</button>
					<button
						type="button"
						onClick={() => onSpawn(projectPath, agentCommand)}
						disabled={!projectPath.trim()}
						className="font-mono text-[10px] uppercase tracking-[1.5px] px-4 py-2 border border-primary bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-40 transition-colors"
					>
						Spawn
					</button>
				</div>
			</div>
		</div>
	)
})
