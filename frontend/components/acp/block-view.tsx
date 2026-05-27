import { memo } from "react"
import { User2, Bot, Plus } from "lucide-react"
import { cn } from "@/lib/utils"
import { MessageMarkdown } from "@/components/messages/message-markdown"
import { Block, EMPTY_USERS } from "./types"
import { timeOf } from "./utils"

export const BlockView = memo(function BlockView({ 
    block, 
    sessionAgent 
}: { 
    block: Block; 
    sessionAgent?: string 
}) {
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
					<details open>
						<summary className="cursor-pointer select-none flex items-baseline gap-2 group">
							<span className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 group-hover:text-foreground transition-colors">THOUGHT</span>
							<span className="text-[10px] text-muted-foreground/40">{timeOf(block.ts)}</span>
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
						block.status === "idle" || block.status === "ready" || block.status === "finished" ? "bg-emerald-500" :
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
})
