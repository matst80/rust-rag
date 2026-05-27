import { AcpEvent, Block } from "./types"

export function envelopeKind(envelope: Record<string, unknown>): { kind: string; payload: Record<string, unknown> } | null {
	if (!envelope || typeof envelope !== "object") return null
	const keys = Object.keys(envelope)
	if (keys.length === 1 && envelope[keys[0]] && typeof envelope[keys[0]] === "object") {
		return { kind: keys[0], payload: envelope[keys[0]] as Record<string, unknown> }
	}
	const k = (envelope as { kind?: unknown; type?: unknown }).kind ?? (envelope as { type?: unknown }).type
	if (typeof k === "string") return { kind: k, payload: envelope as Record<string, unknown> }
	return null
}

export function detachAndClose(ws: WebSocket) {
	ws.onopen = null
	ws.onmessage = null
	ws.onerror = null
	ws.onclose = null
	try {
		ws.close()
	} catch {
		// ignore
	}
}

export function sessionIdOf(payload: Record<string, unknown>): string | undefined {
	const a = payload["acp_session_id"]
	if (typeof a === "string") return a
	const b = payload["session_id"]
	if (typeof b === "string") return b
	return undefined
}

export function extractText(content: unknown): string {
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

export function buildBlocks(events: AcpEvent[], prevBlocks: Block[]): Block[] {
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
			const su: Record<string, unknown> =
				suRaw && typeof suRaw === "object"
					? (suRaw as Record<string, unknown>)
					: (inner as Record<string, unknown>)
			const variant = typeof suRaw === "string" ? suRaw : (su.type as string) ?? ""

			if (variant === "working" || variant === "idle" || variant === "ready" || variant === "finished") {
				const last = blocks[blocks.length - 1]
				if (last && last.kind === "status") {
					last.status = variant
					last.ts = ts
				} else {
					blocks.push({ key: `s-${ev.localSeq}`, kind: "status", status: variant, ts })
				}
				assistantBuf = null
				thoughtBuf = null
				continue
			}

			if (variant === "error") {
				const text =
					(typeof su.content === "string" && su.content) ||
					extractText(su.content) ||
					(typeof su.message === "string" ? (su.message as string) : "") ||
					"agent error"
				blocks.push({ key: `e-${ev.localSeq}`, kind: "error", text, ts })
				assistantBuf = null
				thoughtBuf = null
				continue
			}

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

			blocks.push({ key: `r-${ev.localSeq}`, kind: "raw", eventKind: `agent_update/${variant}`, payload: ev.payload, ts })
			assistantBuf = null
			thoughtBuf = null
			continue
		}

		if (
			k === "snapshot" ||
			k === "state_snapshot" ||
			k === "commands_snapshot" ||
			k === "session_started" || k === "sessionstarted" ||
			k === "session_switched" || k === "sessionswitched" ||
			k === "session_ended" || k === "sessionended" ||
			k === "permission_request" || k === "permissionrequest"
		) {
			continue
		}

		blocks.push({ key: `r-${ev.localSeq}`, kind: "raw", eventKind: ev.kind, payload: ev.payload, ts })
		assistantBuf = null
		thoughtBuf = null
	}

	for (let i = 0; i < Math.min(blocks.length, prevBlocks.length); i++) {
		const nb = blocks[i]
		const ob = prevBlocks[i]
		if (nb.key === ob.key && nb.kind === ob.kind) {
			let identical = false
			if (nb.kind === "user" && ob.kind === "user") identical = nb.text === ob.text
			else if (nb.kind === "assistant" && ob.kind === "assistant") identical = nb.text === ob.text
			else if (nb.kind === "thought" && ob.kind === "thought") identical = nb.text === ob.text
			else if (nb.kind === "status" && ob.kind === "status") identical = nb.status === ob.status
			else if (nb.kind === "tool" && ob.kind === "tool") {
				identical = nb.status === ob.status && nb.content === ob.content && nb.title === ob.title
			} else if (nb.kind === "plan" && ob.kind === "plan") {
				identical = JSON.stringify(nb.entries) === JSON.stringify(ob.entries)
			} else if (nb.kind === "error" && ob.kind === "error") identical = nb.text === ob.text
			else if (nb.kind === "raw" && ob.kind === "raw") {
				identical = nb.eventKind === ob.eventKind && JSON.stringify(nb.payload) === JSON.stringify(ob.payload)
			}

			if (identical) {
				blocks[i] = ob
			}
		}
	}

	return blocks
}

export function timeOf(ts: number): string {
	return new Date(ts).toLocaleTimeString()
}
