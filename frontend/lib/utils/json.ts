export function safeJsonParse(input: string, fallback: any = null): any {
  let clean = input.trim()

  // 1. Handle markdown code blocks
  if (clean.includes("```")) {
    const match = clean.match(/```(?:json)?\s*([\s\S]*?)```/)
    if (match) clean = match[1].trim()
  }

  const tryParse = (str: string) => {
    try {
      return JSON.parse(str)
    } catch (e) {
      // Heuristic fixes for common LLM issues
      try {
        const fixed = str
          .replace(/,\s*([\]}])/g, "$1") // Remove trailing commas
          .replace(/(?<={|,|\s)([a-zA-Z0-9_]+)(?=\s*:)/g, '"$1"') // Quote unquoted keys (simple)
        return JSON.parse(fixed)
      } catch {
        return null
      }
    }
  }

  // 2. Try direct parse
  const firstTry = tryParse(clean)
  if (firstTry !== null) return firstTry

  // 3. Try to find the first { and last } or [ and ]
  const firstBrace = clean.indexOf("{")
  const lastBrace = clean.lastIndexOf("}")
  const firstBracket = clean.indexOf("[")
  const lastBracket = clean.lastIndexOf("]")

  const start = (firstBrace !== -1 && (firstBracket === -1 || firstBrace < firstBracket)) ? firstBrace : firstBracket
  const end = (lastBrace !== -1 && (lastBracket === -1 || lastBrace > lastBracket)) ? lastBrace : lastBracket

  if (start !== -1 && end !== -1 && end > start) {
    const candidate = clean.substring(start, end + 1)
    const secondTry = tryParse(candidate)
    if (secondTry !== null) return secondTry
  }

  return fallback
}
