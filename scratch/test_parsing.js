
function parseToolCall(text) {
  // Existing Markdown format
  const reMarkdown = /```tool\s*([\s\S]*?)```/i
  const mMarkdown = text.match(reMarkdown)
  if (mMarkdown) {
    const before = text.slice(0, mMarkdown.index || 0)
    try {
      const json = JSON.parse(mMarkdown[1].trim())
      if (json && typeof json.name === "string") {
        return {
          before,
          call: { name: json.name, args: json.args ?? {} },
        }
      }
    } catch {
      // not a tool call
    }
  }

  // New native token format
  // Example: <|tool_call>call:search{query: "enduro service date"}<tool_call|>
  const reNative = /<\|tool_call>\s*call:(\w+)\s*(\{[\s\S]*?\})\s*<tool_call\|>/i
  const mNative = text.match(reNative)
  if (mNative) {
    const before = text.slice(0, mNative.index || 0)
    const name = mNative[1]
    const argsStr = mNative[2]
    
    try {
      // Try parsing as standard JSON first
      const args = JSON.parse(argsStr)
      return { before, call: { name, args } }
    } catch {
      // Try relaxed JSON parsing (unquoted keys)
      try {
        // Simple fix: wrap unquoted keys in quotes
        // Matches: { key: "value" } -> { "key": "value" }
        // We look for start of string, { or , followed by key and :
        const fixedArgsStr = argsStr.replace(/(^|{|,)\s*([a-zA-Z0-9_]+)\s*:/g, '$1"$2":')
        const args = JSON.parse(fixedArgsStr)
        return { before, call: { name, args } }
      } catch (e) {
        console.error("Failed to parse native tool call args:", argsStr, e)
      }
    }
  }

  return { before: text, call: null }
}

const testCases = [
  '```tool\n{"name": "search", "args": {"query": "test"}}\n```',
  '<|tool_call>call:search{query: "enduro service date"}<tool_call|><|tool_response>',
  '<|tool_call>call:search{"query": "enduro service date"}<tool_call|>',
  'Some text before <|tool_call>call:search{query: "test"}<tool_call|>',
  '<|tool_call>call:search{query: "nested", options: {k: 5}}<tool_call|>',
  '<|tool_call>call:search{query: "colon: test", key: "val"}<tool_call|>'
]

testCases.forEach(tc => {
  console.log("Input:", tc)
  console.log("Result:", JSON.stringify(parseToolCall(tc), null, 2))
  console.log("-" . repeat(20))
})
