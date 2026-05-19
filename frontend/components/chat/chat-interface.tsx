"use client"

import { useState, useRef, useEffect, useCallback } from "react"
import { Send, Bot, User, Trash2, Brain, Loader2, Wand2, Terminal, CheckCircle2, Sparkles, Cpu } from "lucide-react"
import { api } from "@/lib/api"
import { cn } from "@/lib/utils"
import type {
  ChatCompletionMessage,
  ChatCompletionChunk,
  ChatCompletionStreamError,
  ChatCompletionToolResult,
  ChatCompletionAssistantToolCall
} from "@/lib/api/types"
import { MarkdownView } from "@/components/entries/markdown-view"
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion"
import {
  formatLoadProgress,
  useLlmStatus,
  type LocalChatMessage,
  type LocalToolCall,
} from "@rust-rag/llm"
import { useLocalChat } from "@/hooks/use-local-chat"
import { useHostedChat, type ExtendedMessage } from "@/hooks/use-hosted-chat"

import { ToolResultView } from "@/components/chat/tool-result-view"

function detectWebGpu(): boolean {
  if (typeof navigator === "undefined") return false
  return Boolean((navigator as unknown as { gpu?: unknown }).gpu)
}

export function ChatInterface() {
  const [messages, setMessages] = useState<ExtendedMessage[]>([
    { role: "assistant", content: "Hello! I'm your RAG assistant. How can I help you build your intelligence repository today?" }
  ])
  const [input, setInput] = useState("")
  const [isStreaming, setIsStreaming] = useState(false)
  const [localMode, setLocalMode] = useState(false)
  const [webgpuAvailable, setWebgpuAvailable] = useState(false)
  useEffect(() => { setWebgpuAvailable(detectWebGpu()) }, [])
  const llmStatus = useLlmStatus()
  const scrollRef = useRef<HTMLDivElement>(null)

  const scrollToBottom = useCallback(() => {
    scrollRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [])

  useEffect(() => {
    scrollToBottom()
  }, [messages, scrollToBottom])

  const { generate: generateLocal, stop: stopLocal } = useLocalChat({
    onUpdate: (partialAnswer, toolCalls) => {
      setMessages((prev) => {
        const next = [...prev]
        const last = next[next.length - 1]
        if (last.role === "assistant") {
          last.content = partialAnswer
          last.local_tool_calls = toolCalls
        }
        return next
      })
    },
    onError: (msg) => {
      setMessages((prev) => {
        const next = [...prev]
        const last = next[next.length - 1]
        if (last.role === "assistant") last.content = `Local chat error: ${msg}`
        return next
      })
    },
    onDone: () => {
      setIsStreaming(false)
    }
  })

  const { generate: generateHosted, stop: stopHosted } = useHostedChat({
    onUpdate: (content, reasoning, toolCalls) => {
      setMessages(prev => {
        const next = [...prev]
        const last = next[next.length - 1]
        if (last.role === "assistant") {
          last.content = content
          last.reasoning = reasoning
          if (toolCalls.length > 0) last.tool_calls = toolCalls
        }
        return next
      })
    },
    onToolResult: (toolCallId, content) => {
      setMessages(prev => {
        const next = [...prev]
        const last = next[next.length - 1]
        if (last.role === "assistant") {
          if (!last.tool_results) last.tool_results = {}
          last.tool_results[toolCallId] = content
        }
        return next
      })
    },
    onError: (msg) => {
      setMessages(prev => [
        ...prev,
        { role: "assistant", content: `Failed to connect to assistant: ${msg}` }
      ])
    },
    onDone: () => {
      setIsStreaming(false)
    }
  })

  const handleSend = async () => {
    if (!input.trim() || isStreaming) return

    const userMessage: ExtendedMessage = { role: "user", content: input.trim() }
    const newMessages = [...messages, userMessage]
    setMessages(newMessages)
    setInput("")
    setIsStreaming(true)

    const assistantMessage: ExtendedMessage = { role: "assistant", content: "", reasoning: "" }
    setMessages(prev => [...prev, assistantMessage])

    if (localMode) {
      await generateLocal(newMessages)
    } else {
      await generateHosted(newMessages)
    }
  }

  const clearChat = () => {
    stopLocal()
    stopHosted()
    setMessages([{ role: "assistant", content: "Hello! I'm your RAG assistant. How can I help you today?" }])
    setIsStreaming(false)
  }

  return (
    <div className="flex flex-col w-full h-[calc(100vh-3rem)]">
      {/* Messages */}
      <div className="flex-1 overflow-y-auto flex flex-col">
        <div className="flex-1 flex flex-col max-w-3xl w-full mx-auto px-6 py-8 gap-8">
          {messages.filter(m => m.role !== "system").map((message, i) => (
            <div
              key={i}
              className={cn(
                "flex gap-4 animate-in fade-in slide-in-from-bottom-2 duration-300 fill-mode-both",
                message.role === "user" ? "flex-row-reverse" : "flex-row"
              )}
              style={{ animationDelay: `${i * 20}ms` }}
            >
              {/* Avatar */}
              <div
                className={cn(
                  "size-8 flex items-center justify-center shrink-0 border transition-all duration-300",
                  message.role === "user"
                    ? "bg-primary text-primary-foreground border-primary/40 shadow-lg shadow-primary/20"
                    : "bg-card border-border text-muted-foreground shadow-sm"
                )}
              >
                {message.role === "user" ? <User className="size-4" /> : <Bot className="size-4" />}
              </div>

              <div className={cn(
                "flex flex-col gap-2 max-w-[82%]",
                message.role === "user" ? "items-end" : "items-start"
              )}>
                {/* Reasoning */}
                {message.reasoning && (
                  <Accordion type="single" collapsible className="w-full">
                    <AccordionItem value="thinking" className="border-none">
                      <AccordionTrigger className="py-2 px-3 hover:no-underline bg-muted/20 border border-border text-muted-foreground font-mono text-[10px] uppercase tracking-[1.5px] flex gap-2">
                        <div className="flex items-center gap-2">
                          <Wand2 className="size-3 animate-pulse text-primary" />
                          Thought process
                        </div>
                      </AccordionTrigger>
                      <AccordionContent className="pt-3 px-3 text-sm text-muted-foreground border-l-2 border-primary/30 ml-2 italic">
                        <div className="whitespace-pre-wrap font-serif opacity-80">{message.reasoning}</div>
                      </AccordionContent>
                    </AccordionItem>
                  </Accordion>
                )}

                {/* Local tool calls (on-device chat) */}
                {message.local_tool_calls?.map((tc, idx) => (
                  <div key={`local-${idx}`} className="w-full border border-primary/20 bg-primary/5 overflow-hidden text-xs">
                    <div className="flex items-center gap-2 px-3 py-1.5 bg-primary/10 border-b border-primary/10 font-mono font-bold text-primary">
                      <Cpu className="size-3" />
                      <span>{tc.name}</span>
                      {tc.result && !tc.error && <CheckCircle2 className="size-3 text-green-500 ml-auto" />}
                      {tc.error && <span className="ml-auto text-destructive">err</span>}
                    </div>
                    <div className="p-2 font-mono text-[10px] opacity-70 truncate" title={JSON.stringify(tc.args)}>
                      {JSON.stringify(tc.args)}
                    </div>
                    {tc.result && (
                      <Accordion type="single" collapsible className="w-full border-t border-primary/5">
                        <AccordionItem value="result" className="border-none">
                          <AccordionTrigger className="py-1 px-3 font-mono text-[10px] hover:no-underline text-primary/60 uppercase tracking-[1px]">
                            View result
                          </AccordionTrigger>
                          <AccordionContent className="p-3 bg-muted/20 max-h-80 overflow-auto border-t border-primary/5">
                            <ToolResultView name={tc.name} result={tc.result} />
                          </AccordionContent>
                        </AccordionItem>
                      </Accordion>
                    )}
                  </div>
                ))}

                {/* Tool calls */}
                {message.tool_calls?.map((tc, idx) => (
                  <div key={idx} className="w-full border border-primary/20 bg-primary/5 overflow-hidden text-xs">
                    <div className="flex items-center gap-2 px-3 py-1.5 bg-primary/10 border-b border-primary/10 font-mono font-bold text-primary">
                      <Terminal className="size-3" />
                      <span>{tc.function.name}</span>
                      {message.tool_results?.[tc.id] && <CheckCircle2 className="size-3 text-green-500 ml-auto" />}
                    </div>
                    <div className="p-2 font-mono text-[10px] opacity-70 truncate" title={tc.function.arguments}>
                      {tc.function.arguments}
                    </div>
                    {message.tool_results?.[tc.id] && (
                      <Accordion type="single" collapsible className="w-full border-t border-primary/5">
                        <AccordionItem value="result" className="border-none">
                          <AccordionTrigger className="py-1 px-3 font-mono text-[10px] hover:no-underline text-primary/60 uppercase tracking-[1px]">
                            View result
                          </AccordionTrigger>
                          <AccordionContent className="p-3 bg-muted/20 max-h-80 overflow-auto border-t border-primary/5">
                            <ToolResultView name={tc.function.name} result={message.tool_results[tc.id]} />
                          </AccordionContent>
                        </AccordionItem>
                      </Accordion>
                    )}
                  </div>
                ))}

                {/* Bubble */}
                <div
                  className={cn(
                    "px-6 py-4 border leading-relaxed transition-all duration-200",
                    message.role === "user"
                      ? "bg-primary text-primary-foreground border-primary/40 shadow-[0_12px_30px_-10px_rgba(0,0,0,0.15)] ring-1 ring-white/10 ring-inset"
                      : "bg-card text-card-foreground border-border shadow-sm"
                  )}
                >
                  {typeof message.content === "string" ? (
                    message.content ? (
                      <div className="prose prose-sm dark:prose-invert max-w-none">
                        <MarkdownView content={message.content} />
                      </div>
                    ) : (
                      message.role === "assistant" && !message.tool_calls && !message.reasoning ? (
                        <div className="flex gap-1 py-1">
                          <div className="size-1.5 bg-muted-foreground/30 rounded-full animate-bounce" style={{ animationDelay: "0ms" }} />
                          <div className="size-1.5 bg-muted-foreground/30 rounded-full animate-bounce" style={{ animationDelay: "150ms" }} />
                          <div className="size-1.5 bg-muted-foreground/30 rounded-full animate-bounce" style={{ animationDelay: "300ms" }} />
                        </div>
                      ) : null
                    )
                  ) : (
                    <pre className="text-xs overflow-auto">{JSON.stringify(message.content, null, 2)}</pre>
                  )}
                </div>

                <span className="font-mono text-[9px] text-muted-foreground/60 uppercase tracking-[2.5px] px-2 py-0.5 border-l border-primary/30 mt-1 ml-1">
                  {message.role}
                </span>
              </div>
            </div>
          ))}
          <div ref={scrollRef} />
        </div>
      </div>

      {/* Input bar */}
      <div className="border-t border-border bg-background/95 backdrop-blur shrink-0">
        <form
          onSubmit={(e) => { e.preventDefault(); handleSend() }}
          className="max-w-3xl mx-auto px-6 py-4"
        >
          <div
            className={cn(
              "relative flex items-center border border-border bg-card transition-all duration-200",
              "focus-within:border-primary focus-within:[box-shadow:0_0_0_1px_oklch(0.9_0.148_196.3/0.15),inset_0_0_30px_oklch(0.9_0.148_196.3/0.03)]"
            )}
          >
            <span className="px-4 font-mono text-[11px] text-primary select-none shrink-0">›</span>
            <input
              placeholder="Query intelligence repository..."
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend() } }}
              disabled={isStreaming}
              className="flex-1 bg-transparent border-none outline-none py-3.5 font-mono text-sm text-foreground placeholder:text-muted-foreground/60"
            />
            <button
              type="submit"
              disabled={!input.trim() || isStreaming}
              className={cn(
                "mx-3 flex items-center gap-1.5 px-3 py-1.5 font-mono text-[10px] font-black uppercase tracking-[1.5px] transition-all shrink-0",
                input.trim() && !isStreaming
                  ? "bg-primary text-primary-foreground shadow-[0_0_14px_oklch(0.9_0.148_196.3/0.3)] hover:shadow-[0_0_20px_oklch(0.9_0.148_196.3/0.4)]"
                  : "border border-border text-muted-foreground/40 cursor-not-allowed"
              )}
            >
              {isStreaming ? <Loader2 className="size-3.5 animate-spin" /> : <Send className="size-3.5" />}
            </button>
          </div>

          <div className="flex items-center justify-between mt-2">
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={() => webgpuAvailable && setLocalMode((v) => !v)}
                disabled={!webgpuAvailable}
                title={webgpuAvailable ? "Toggle on-device Gemma + RAG tools" : "WebGPU not available"}
                className={cn(
                  "flex items-center gap-1.5 font-mono text-[9px] uppercase tracking-[1.5px] px-2 py-1 border transition-colors",
                  localMode
                    ? "border-primary/50 text-primary bg-primary/10"
                    : "border-border text-muted-foreground/50 hover:text-foreground",
                  !webgpuAvailable && "opacity-30 cursor-not-allowed"
                )}
              >
                {localMode ? <Sparkles className="size-2.5" /> : <Brain className="size-2.5" />}
                {localMode ? "Local · Gemma" : "Backend"}
              </button>
              {localMode && llmStatus.kind === "loading" && (
                <span className="font-mono text-[9px] text-muted-foreground tabular-nums">
                  {formatLoadProgress(llmStatus)}
                </span>
              )}
              {localMode && llmStatus.kind === "error" && (
                <span className="font-mono text-[9px] text-destructive truncate max-w-[16rem]" title={llmStatus.message}>
                  {llmStatus.message}
                </span>
              )}
              {!localMode && (
                <p className="font-mono text-[9px] text-muted-foreground/30 flex items-center gap-1.5 uppercase tracking-[1px]">
                  <Terminal className="size-2.5" /> Backend agent
                </p>
              )}
            </div>
            <button
              type="button"
              onClick={clearChat}
              className="flex items-center gap-1.5 font-mono text-[9px] uppercase tracking-[1px] text-muted-foreground/40 hover:text-destructive transition-colors"
            >
              <Trash2 className="size-2.5" /> Clear
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
