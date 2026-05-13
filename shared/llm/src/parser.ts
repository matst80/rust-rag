
export interface ToolCallParseResult {
  before: string
  call: { name: string; args: Record<string, unknown> } | null
}

/**
 * Parses a string for tool calls. Supports:
 * 1. Markdown code blocks: ```tool {"name": "...", "args": {...}} ```
 * 2. Native tokens: <|tool_call|>call:NAME{ARGS}<|tool_call|>
 */
export function parseToolCall(text: string): ToolCallParseResult {
  // 1. Markdown format
  const reMarkdown = /```tool\s*([\s\S]*?)```/i
  const mMarkdown = text.match(reMarkdown)
  if (mMarkdown) {
    const before = text.slice(0, mMarkdown.index ?? 0)
    try {
      const json = JSON.parse(mMarkdown[1].trim())
      if (json && typeof json.name === "string") {
        return {
          before,
          call: { name: json.name, args: json.args ?? {} },
        }
      }
    } catch {
      // ignore
    }
  }

  // 2. Native format
  // Example: <|tool_call|>call:search{query: "test"}<|tool_call|>
  const reNative = /<\|?tool_call\|?>\s*call:(\w+)\s*([\s\S]*?)\s*<\|?tool_call\|?>/i
  const mNative = text.match(reNative)
  if (mNative) {
    const before = text.slice(0, mNative.index ?? 0)
    const name = mNative[1]
    const argsStr = mNative[2].trim()

    try {
      const args = JSON.parse(argsStr)
      return { before, call: { name, args } }
    } catch {
      // Relaxed JSON parsing for unquoted keys
      try {
        const fixedArgsStr = argsStr.replace(/(^|{|,)\s*([a-zA-Z0-9_]+)\s*:/g, '$1"$2":')
        const args = JSON.parse(fixedArgsStr)
        return { before, call: { name, args } }
      } catch {
        // failed
      }
    }
  }

  return { before: text, call: null }
}

/**
 * Splits text at the first occurrence of a tool calling token.
 * Useful for hiding tool calls during streaming.
 */
export function hideToolTokens(text: string): string {
  return text.split(/```tool|<\|?tool_call/i)[0]
}
