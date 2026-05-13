
import { useState, useRef, useCallback } from "react"
import {
  runLocalChat,
  type LocalChatMessage,
  type LocalToolCall,
} from "@rust-rag/llm"
import { buildRagTools } from "@/lib/ai/tools"
import type { ChatCompletionMessage } from "@/lib/api/types"

export interface UseLocalChatArgs {
  onUpdate: (partialAnswer: string, toolCalls: LocalToolCall[]) => void
  onError?: (error: string) => void
  onDone?: (finalAnswer: string) => void
}

export function useLocalChat({ onUpdate, onError, onDone }: UseLocalChatArgs) {
  const [isGenerating, setIsGenerating] = useState(false)
  const abortControllerRef = useRef<AbortController | null>(null)

  const stop = useCallback(() => {
    abortControllerRef.current?.abort()
    setIsGenerating(false)
  }, [])

  const generate = useCallback(async (messages: ChatCompletionMessage[]) => {
    if (isGenerating) return
    
    setIsGenerating(true)
    abortControllerRef.current = new AbortController()

    // Convert ChatCompletionMessage[] to LocalChatMessage[]
    const history: LocalChatMessage[] = messages
      .filter((m) => m.role === "user" || m.role === "assistant")
      .map((m) => ({
        role: m.role as "user" | "assistant",
        content: typeof m.content === "string" ? m.content : JSON.stringify(m.content),
      }))

    try {
      const final = await runLocalChat({
        history,
        tools: buildRagTools(),
        signal: abortControllerRef.current.signal,
        onUpdate: ({ partialAnswer, toolCalls }) => {
          onUpdate(partialAnswer ?? "", toolCalls ?? [])
        },
      })
      onDone?.(final)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      if (msg !== "aborted" && msg !== "AbortError") {
        onError?.(msg)
      }
    } finally {
      setIsGenerating(false)
    }
  }, [isGenerating, onUpdate, onError, onDone])

  return {
    generate,
    stop,
    isGenerating,
  }
}
