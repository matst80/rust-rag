"use client"

import { api } from "@/lib/api"
import { getLlmClient } from "./llm-client"

export interface LocalChatMessage {
  role: "user" | "assistant"
  content: string
}

export interface LocalToolCall {
  id: string
  name: string
  args: Record<string, unknown>
  result?: string
  error?: string
}

export interface LocalChatStepUpdate {
  partialAnswer?: string
  toolCalls?: LocalToolCall[]
  done?: boolean
}

const SYSTEM_PROMPT = `You are a careful research assistant for a personal knowledge base (RAG).

You can call tools to look things up. To call a tool, output exactly one JSON code block on its own:

\`\`\`tool
{"name": "search", "args": {"query": "auth middleware", "top_k": 5}}
\`\`\`

Available tools:
- search(query: string, top_k?: number) — semantic search over all entries. Returns list with id, source_id, score, snippet.
- get_entry(id: string) — fetch the full text of a single entry by id.
- list_paths(source_id?: string) — list known wiki paths.

Rules:
- Use tools when the user's question depends on stored notes.
- Cite entry ids in your final answer like (id_here).
- After getting tool results, you may call another tool or give a final answer.
- Final answer: plain markdown, no tool block.
- Be brief. 3-6 sentences unless asked for detail.`

const MAX_ROUNDS = 4

interface ToolCallParseResult {
  before: string
  call: { name: string; args: Record<string, unknown> } | null
  after: string
}

function parseToolCall(text: string): ToolCallParseResult {
  // Look for ```tool ... ``` fenced block.
  const re = /```tool\s*([\s\S]*?)```/i
  const m = text.match(re)
  if (!m) return { before: text, call: null, after: "" }
  const before = text.slice(0, m.index ?? 0)
  const after = text.slice((m.index ?? 0) + m[0].length)
  try {
    const json = JSON.parse(m[1].trim())
    if (json && typeof json.name === "string") {
      return {
        before,
        call: { name: json.name, args: json.args ?? {} },
        after,
      }
    }
  } catch {
    // Fall through — treat as not a tool call.
  }
  return { before: text, call: null, after: "" }
}

async function runTool(
  name: string,
  args: Record<string, unknown>
): Promise<string> {
  if (name === "search") {
    const query = String(args.query ?? "").trim()
    if (!query) return JSON.stringify({ error: "query is required" })
    const top_k = Math.min(8, Math.max(1, Number(args.top_k ?? 5)))
    const bundle = await api.search({ query, top_k })
    return JSON.stringify(
      bundle.results.slice(0, top_k).map((r) => ({
        id: r.id,
        source_id: r.source_id,
        score: Number(r.score.toFixed(3)),
        snippet: (r.text ?? "").slice(0, 280).replace(/\s+/g, " "),
      }))
    )
  }
  if (name === "get_entry") {
    const id = String(args.id ?? "").trim()
    if (!id) return JSON.stringify({ error: "id is required" })
    const entry = await api.items.get(id)
    return JSON.stringify({
      id: entry.id,
      source_id: entry.source_id,
      path: entry.path ?? null,
      text: (entry.text ?? "").slice(0, 4000),
      metadata: entry.metadata,
    })
  }
  if (name === "list_paths") {
    const source_id =
      typeof args.source_id === "string" && args.source_id ? args.source_id : undefined
    const data = await api.tree.paths(source_id)
    return JSON.stringify(
      data.paths.slice(0, 80).map((p) => ({
        source_id: p.source_id,
        path: p.path,
        count: p.count,
      }))
    )
  }
  return JSON.stringify({ error: `unknown tool: ${name}` })
}

function buildPrompt(history: LocalChatMessage[], scratch: string): string {
  const lines: string[] = [SYSTEM_PROMPT, ""]
  for (const m of history) {
    lines.push(m.role === "user" ? `User: ${m.content}` : `Assistant: ${m.content}`)
  }
  lines.push("Assistant:")
  if (scratch) lines.push(scratch)
  return lines.join("\n")
}

export interface RunLocalChatArgs {
  history: LocalChatMessage[]
  onUpdate: (update: LocalChatStepUpdate) => void
  signal?: AbortSignal
}

/**
 * Run a ReAct loop: generate, parse for tool calls, execute, append observation,
 * regenerate. Stops on plain answer or MAX_ROUNDS.
 */
export async function runLocalChat({
  history,
  onUpdate,
  signal,
}: RunLocalChatArgs): Promise<string> {
  const client = getLlmClient()
  const toolLog: LocalToolCall[] = []
  let scratch = ""

  for (let round = 0; round < MAX_ROUNDS; round++) {
    if (signal?.aborted) throw new Error("aborted")
    const prompt = buildPrompt(history, scratch)

    let lastPartial = ""
    const raw = await client.generate(
      prompt,
      (partial) => {
        lastPartial = partial
        // Stream the visible portion (before any tool fence).
        const visible = partial.split("```tool")[0]
        onUpdate({
          partialAnswer: (scratch + visible).trim(),
          toolCalls: toolLog,
        })
      },
      signal
    )
    const text = raw || lastPartial

    const parsed = parseToolCall(text)
    if (!parsed.call) {
      const final = (scratch + text).trim()
      onUpdate({ partialAnswer: final, toolCalls: toolLog, done: true })
      return final
    }

    const call: LocalToolCall = {
      id: `tc-${Date.now()}-${round}`,
      name: parsed.call.name,
      args: parsed.call.args,
    }
    toolLog.push(call)
    onUpdate({
      partialAnswer: (scratch + parsed.before).trim(),
      toolCalls: toolLog,
    })

    try {
      call.result = await runTool(call.name, call.args)
    } catch (err) {
      call.error = err instanceof Error ? err.message : String(err)
      call.result = JSON.stringify({ error: call.error })
    }
    onUpdate({
      partialAnswer: (scratch + parsed.before).trim(),
      toolCalls: [...toolLog],
    })

    // Append tool call + observation back into the assistant scratch for next round.
    scratch +=
      parsed.before +
      "\n```tool\n" +
      JSON.stringify({ name: call.name, args: call.args }) +
      "\n```\n" +
      "```tool_result\n" +
      (call.result ?? "") +
      "\n```\n"
  }

  // Hit max rounds — return what we have.
  const final = scratch.trim() || "(no answer)"
  onUpdate({ partialAnswer: final, toolCalls: toolLog, done: true })
  return final
}
