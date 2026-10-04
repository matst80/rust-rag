import { useState, useRef, useCallback } from "react"
import { api } from "@/lib/api"
import type {
  ChatCompletionMessage,
  ChatCompletionChunk,
  ChatCompletionStreamError,
  ChatCompletionToolResult,
  ChatCompletionAssistantToolCall
} from "@/lib/api/types"
import type { LocalToolCall } from "@rust-rag/llm"

export interface ExtendedMessage extends ChatCompletionMessage {
  reasoning?: string
  tool_results?: Record<string, string>
  local_tool_calls?: LocalToolCall[]
}

export interface UseHostedChatArgs {
  onUpdate: (content: string, reasoning: string, toolCalls: ChatCompletionAssistantToolCall[]) => void
  onToolResult: (toolCallId: string, content: string) => void
  onError?: (error: string) => void
  onDone?: () => void
}

// Replayed history keeps every tool result but truncates oversized payloads:
// full entries would otherwise grow each subsequent request until it exceeds
// the backend's request body limit — the connection is then reset mid-upload,
// which the browser reports as a bare network error. The model saw the full
// result in the turn it happened; for context it can always re-fetch.
const MAX_REPLAYED_TOOL_RESULT_CHARS = 2000

function truncateToolResult(content: string): string {
  if (content.length <= MAX_REPLAYED_TOOL_RESULT_CHARS) return content
  return `${content.slice(0, MAX_REPLAYED_TOOL_RESULT_CHARS)}…[truncated ${content.length - MAX_REPLAYED_TOOL_RESULT_CHARS} chars]`
}

export function useHostedChat({ onUpdate, onToolResult, onError, onDone }: UseHostedChatArgs) {
  const [isGenerating, setIsGenerating] = useState(false)
  const abortControllerRef = useRef<AbortController | null>(null)

  const stop = useCallback(() => {
    abortControllerRef.current?.abort()
    setIsGenerating(false)
  }, [])

  const generate = useCallback(async (messages: ExtendedMessage[]) => {
    if (isGenerating) return

    setIsGenerating(true)
    abortControllerRef.current = new AbortController()

    try {
      let fullContent = ""
      let fullReasoning = ""
      // Tool calls finalized from earlier tool-loop iterations of this
      // exchange. The backend runs one upstream response per loop iteration,
      // and each response restarts its delta `index` at 0 — so accumulation
      // must be scoped per response id and completed calls carried over,
      // otherwise two iterations merge into one garbled tool call
      // (concatenated name/arguments) that breaks history replay.
      let finalizedToolCalls: ChatCompletionAssistantToolCall[] = []
      let accumulatedToolCalls: Record<number, Partial<ChatCompletionAssistantToolCall>> = {}
      let currentResponseId: string | undefined

      // Expand messages to include tool results in the format expected by the API
      const expandedMessages = messages.flatMap(m => {
        if (m.tool_calls?.length) {
          // An assistant message with tool_calls must be followed by a tool
          // message per call id — replay only the calls that have results and
          // strip the field entirely when none do (e.g. generation was stopped
          // mid-loop), or the upstream rejects the whole request.
          const paired = m.tool_calls.filter(tc => m.tool_results?.[tc.id] !== undefined)
          if (paired.length === 0) {
            return [{
              role: m.role,
              content: m.content || null,
              name: m.name,
              tool_call_id: m.tool_call_id
            } as ChatCompletionMessage]
          }
          const expanded: ChatCompletionMessage[] = []
          expanded.push({ role: m.role, content: m.content || null, tool_calls: paired })
          for (const tc of paired) {
            expanded.push({
              role: "tool" as const,
              tool_call_id: tc.id,
              content: truncateToolResult(m.tool_results![tc.id]),
              name: tc.function.name
            })
          }
          return expanded
        }
        return [{
          role: m.role,
          content: m.content || null,
          name: m.name,
          tool_call_id: m.tool_call_id,
          tool_calls: m.tool_calls
        } as ChatCompletionMessage]
      }) as ChatCompletionMessage[]

      await api.chat.stream(
        {
          messages: expandedMessages,
          stream: true
        },
        {
          onChunk: (chunk: ChatCompletionChunk) => {
            const delta = chunk.choices[0]?.delta
            if (!delta) return

            if (chunk.id !== undefined && chunk.id !== currentResponseId) {
              // First chunk of a new upstream response: the previous
              // response's tool calls are complete.
              currentResponseId = chunk.id
              finalizedToolCalls = [
                ...finalizedToolCalls,
                ...Object.values(accumulatedToolCalls)
              ] as ChatCompletionAssistantToolCall[]
              accumulatedToolCalls = {}
            }

            let updated = false

            if (delta.content) {
              fullContent += delta.content
              updated = true
            }

            const reasoning = delta.reasoning_content || delta.reasoning
            if (reasoning) {
              fullReasoning += reasoning
              updated = true
            }

            if (delta.tool_calls) {
              for (const tc of delta.tool_calls) {
                const existing = accumulatedToolCalls[tc.index] || {
                  id: "",
                  type: "function",
                  function: { name: "", arguments: "" }
                }
                if (tc.id) existing.id = tc.id
                if (tc.function?.name) {
                  if (!existing.function) existing.function = { name: "", arguments: "" }
                  existing.function.name += tc.function.name
                }
                if (tc.function?.arguments) {
                  if (!existing.function) existing.function = { name: "", arguments: "" }
                  existing.function.arguments += tc.function.arguments
                }
                accumulatedToolCalls[tc.index] = existing
              }
              updated = true
            }

            if (updated) {
              onUpdate(
                fullContent,
                fullReasoning,
                [...finalizedToolCalls, ...Object.values(accumulatedToolCalls)] as ChatCompletionAssistantToolCall[]
              )
            }
          },
          onToolResult: (result: ChatCompletionToolResult) => {
            onToolResult(result.tool_call_id, result.content)
          },
          onError: (error: ChatCompletionStreamError) => {
            onError?.(error.error.message)
          },
          onDone: () => {
            setIsGenerating(false)
            onDone?.()
          }
        },
        { signal: abortControllerRef.current.signal }
      )
    } catch (error: unknown) {
      if ((error as { name?: string })?.name !== "AbortError") {
        onError?.((error as { message?: string })?.message ?? String(error))
      }
      setIsGenerating(false)
    }
  }, [isGenerating, onUpdate, onToolResult, onError, onDone])

  return {
    generate,
    stop,
    isGenerating,
  }
}
