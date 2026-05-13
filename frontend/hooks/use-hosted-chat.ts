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
      let accumulatedToolCalls: Record<number, Partial<ChatCompletionAssistantToolCall>> = {}

      // Expand messages to include tool results in the format expected by the API
      const expandedMessages = messages.flatMap(m => {
        if (m.tool_calls && m.tool_results && Object.keys(m.tool_results).length > 0) {
          const expanded = []
          expanded.push({ role: m.role, content: null, tool_calls: m.tool_calls })
          for (const tc of m.tool_calls) {
            if (m.tool_results[tc.id] !== undefined) {
              expanded.push({
                role: "tool" as const,
                tool_call_id: tc.id,
                content: m.tool_results[tc.id],
                name: tc.function.name
              })
            }
          }
          if (m.content) {
            expanded.push({ role: m.role, content: m.content })
          }
          return expanded
        }
        return [{
          role: m.role,
          content: m.content,
          name: m.name,
          tool_call_id: m.tool_call_id,
          tool_calls: m.tool_calls
        }]
      })

      await api.chat.stream(
        {
          messages: expandedMessages,
          stream: true
        },
        {
          onChunk: (chunk: ChatCompletionChunk) => {
            const delta = chunk.choices[0]?.delta
            if (!delta) return

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
              onUpdate(fullContent, fullReasoning, Object.values(accumulatedToolCalls) as ChatCompletionAssistantToolCall[])
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
