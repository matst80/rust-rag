import { getLlmClient, type ProfileKey } from "./client"

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

export interface ToolDef {
  name: string
  /** Short, model-facing description (signature + when to use). */
  description: string
  run: (args: Record<string, unknown>) => Promise<string>
}

const MAX_ROUNDS = 4

function buildSystemPrompt(tools: ToolDef[]): string {
  const toolList = tools
    .map((t) => `- ${t.name}: ${t.description}`)
    .join("\n")
  return `You are a careful research assistant for a personal knowledge base (RAG).

You can call tools to look things up. To call a tool, output exactly one JSON code block on its own:

\`\`\`tool
{"name": "search", "args": {"query": "auth middleware", "top_k": 5}}
\`\`\`

Available tools:
${toolList}

Rules:
- Use tools when the user's question depends on stored notes.
- Cite entry ids in your final answer like (id_here).
- After getting tool results, you may call another tool or give a final answer.
- Final answer: plain markdown, no tool block.
- Be brief. 3-6 sentences unless asked for detail.`
}

interface ToolCallParseResult {
  before: string
  call: { name: string; args: Record<string, unknown> } | null
}

function parseToolCall(text: string): ToolCallParseResult {
  const re = /```tool\s*([\s\S]*?)```/i
  const m = text.match(re)
  if (!m) return { before: text, call: null }
  const before = text.slice(0, m.index ?? 0)
  try {
    const json = JSON.parse(m[1].trim())
    if (json && typeof json.name === "string") {
      return {
        before,
        call: { name: json.name, args: json.args ?? {} },
      }
    }
  } catch {
    // not a tool call
  }
  return { before: text, call: null }
}

function buildPrompt(
  system: string,
  history: LocalChatMessage[],
  scratch: string
): string {
  const parts: string[] = []

  // System turn
  parts.push(`<|turn|>system\n${system}<turn|>`)

  // History turns
  for (const m of history) {
    const role = m.role === "user" ? "user" : "model"
    parts.push(`<|turn|>${role}\n${m.content}<turn|>`)
  }

  // Final model turn (left open for completion)
  parts.push(`<|turn|>model\n${scratch}`)

  return parts.join("\n")
}

export interface RunLocalChatArgs {
  history: LocalChatMessage[]
  tools: ToolDef[]
  onUpdate: (update: LocalChatStepUpdate) => void
  signal?: AbortSignal
  profile?: ProfileKey
}

export async function runLocalChat({
  history,
  tools,
  onUpdate,
  signal,
  profile = "text",
}: RunLocalChatArgs): Promise<string> {
  const client = getLlmClient(profile)
  const toolByName = new Map(tools.map((t) => [t.name, t]))
  const system = buildSystemPrompt(tools)
  const toolLog: LocalToolCall[] = []
  let scratch = ""

  for (let round = 0; round < MAX_ROUNDS; round++) {
    if (signal?.aborted) throw new Error("aborted")
    const prompt = buildPrompt(system, history, scratch)

    let lastPartial = ""
    const raw = await client.generate(
      prompt,
      (partial) => {
        lastPartial = partial
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

    const def = toolByName.get(parsed.call.name)
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
      if (!def) throw new Error(`unknown tool: ${call.name}`)
      call.result = await def.run(call.args)
    } catch (err) {
      call.error = err instanceof Error ? err.message : String(err)
      call.result = JSON.stringify({ error: call.error })
    }
    onUpdate({
      partialAnswer: (scratch + parsed.before).trim(),
      toolCalls: [...toolLog],
    })

    scratch +=
      parsed.before +
      "\n```tool\n" +
      JSON.stringify({ name: call.name, args: call.args }) +
      "\n```\n" +
      "```tool_result\n" +
      (call.result ?? "") +
      "\n```\n"
  }

  const final = scratch.trim() || "(no answer)"
  onUpdate({ partialAnswer: final, toolCalls: toolLog, done: true })
  return final
}
