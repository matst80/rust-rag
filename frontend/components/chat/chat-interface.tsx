"use client"

import { useState, useRef, useEffect, useCallback } from "react"
import { Send, Bot, User, Trash2, Brain, Loader2, Wand2, Terminal, CheckCircle2, ChevronDown, ChevronUp } from "lucide-react"
import { api } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
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

// Extended message type to handle internal state like reasoning
interface ExtendedMessage extends ChatCompletionMessage {
  reasoning?: string
  tool_results?: Record<string, string>
}

export function ChatInterface() {
  const [messages, setMessages] = useState<ExtendedMessage[]>([
    { 
      role: "system", 
      content: `You are a RAG Intelligence Assistant. Your goal is to build and query a high-quality knowledge base.

CORE GUIDELINES:
1. CRAWLING: Use 'ingest_web_content' to research new information.
2. LARGE PAGES: If a page is too large (>20k chars), it will be saved to disk. Use 'read_file_range' to read it line-by-line.
3. CHUNKING: NEVER store a whole page as a single entry. It ruins embedding quality.
4. EXTRACTION: When you ingest a page, extract specific, meaningful sections.
5. STORAGE: Use 'store_entry' to save focused chunks of 1000-1500 characters.
6. CONTEXT: Ensure each stored chunk is self-contained (include relevant titles/context in the text).
7. HYBRID SEARCH: Use 'search_entries' to find information you've already stored.

Be concise and analytical.`
    },
    { role: "assistant", content: "Hello! I'm your RAG assistant. How can I help you build your intelligence repository today?" }
  ])
  const [input, setInput] = useState("")
  const [isStreaming, setIsStreaming] = useState(false)
  const scrollRef = useRef<HTMLDivElement>(null)
  const abortControllerRef = useRef<AbortController | null>(null)

  const scrollToBottom = useCallback(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollIntoView({ behavior: "smooth" })
    }
  }, [])

  useEffect(() => {
    scrollToBottom()
  }, [messages, scrollToBottom])

  const handleSend = async () => {
    if (!input.trim() || isStreaming) return

    const userMessage: ExtendedMessage = { role: "user", content: input.trim() }
    const newMessages = [...messages, userMessage]
    setMessages(newMessages)
    setInput("")
    setIsStreaming(true)

    // Prepare assistant message placeholder
    const assistantMessage: ExtendedMessage = { role: "assistant", content: "", reasoning: "" }
    setMessages(prev => [...prev, assistantMessage])

    abortControllerRef.current = new AbortController()

    try {
      let fullContent = ""
      let fullReasoning = ""
      let accumulatedToolCalls: Record<number, Partial<ChatCompletionAssistantToolCall>> = {}

      await api.chat.stream(
        {
          messages: newMessages.map(m => ({
            role: m.role,
            content: m.content,
            name: m.name,
            tool_call_id: m.tool_call_id,
            tool_calls: m.tool_calls
          })),
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
              setMessages(prev => {
                const next = [...prev]
                const last = next[next.length - 1]
                if (last.role === "assistant") {
                  last.content = fullContent
                  last.reasoning = fullReasoning
                  
                  const toolCalls = Object.values(accumulatedToolCalls)
                  if (toolCalls.length > 0) {
                    last.tool_calls = toolCalls as ChatCompletionAssistantToolCall[]
                  }
                }
                return next
              })
            }
          },
          onToolResult: (result: ChatCompletionToolResult) => {
             setMessages(prev => {
                const next = [...prev]
                const last = next[next.length - 1]
                if (last.role === "assistant") {
                   if (!last.tool_results) last.tool_results = {}
                   last.tool_results[result.tool_call_id] = result.content
                }
                return next
             })
          },
          onError: (error: ChatCompletionStreamError) => {
            console.error("Chat error:", error)
            setMessages(prev => [
              ...prev,
              { role: "assistant", content: `Error: ${error.error.message}` }
            ])
          },
          onDone: () => {
            setIsStreaming(false)
          }
        },
        { signal: abortControllerRef.current.signal }
      )
    } catch (error: any) {
      if (error.name === "AbortError") {
        console.log("Stream aborted")
      } else {
        console.error("Chat error:", error)
        setMessages(prev => [
          ...prev,
          { role: "assistant", content: `Failed to connect to assistant: ${error.message}` }
        ])
      }
      setIsStreaming(false)
    }
  }

  const clearChat = () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort()
    }
    setMessages([{ role: "assistant", content: "Hello! I'm your RAG assistant. How can I help you today?" }])
    setIsStreaming(false)
  }

  return (
    <div className="flex flex-col h-[calc(100vh-8rem)] max-w-4xl mx-auto w-full border rounded-lg overflow-hidden bg-background/50 backdrop-blur shadow-2xl">
      <div className="flex items-center justify-between p-4 border-b bg-muted/30">
        <div className="flex items-center gap-2">
          <Brain className="size-5 text-primary" />
          <h2 className="font-semibold tracking-tight">RAG Intelligence</h2>
        </div>
        <Button variant="ghost" size="icon" onClick={clearChat} title="Clear chat" className="hover:text-destructive transition-colors">
          <Trash2 className="size-4" />
        </Button>
      </div>

      <ScrollArea className="flex-1 p-4 md:p-6">
        <div className="flex flex-col gap-8">
          {messages.filter(m => m.role !== 'system').map((message, i) => (
            <div
              key={i}
              className={cn(
                "flex gap-4",
                message.role === "user" ? "flex-row-reverse" : "flex-row"
              )}
            >
              <div
                className={cn(
                  "size-9 rounded-full flex items-center justify-center shrink-0 border shadow-md",
                  message.role === "user" ? "bg-primary text-primary-foreground ring-2 ring-primary/20" : "bg-card border-muted-foreground/20"
                )}
              >
                {message.role === "user" ? <User className="size-5" /> : <Bot className="size-5" />}
              </div>
              
              <div className={cn(
                "flex flex-col gap-2 max-w-[85%]",
                message.role === "user" ? "items-end" : "items-start"
              )}>
                {/* Thinking Block */}
                {message.reasoning && (
                  <Accordion type="single" collapsible className="w-full">
                    <AccordionItem value="thinking" className="border-none">
                      <AccordionTrigger className="py-2 hover:no-underline px-3 rounded-lg bg-muted/30 text-muted-foreground text-xs border border-muted flex gap-2">
                        <div className="flex items-center gap-2 text-left">
                          <Wand2 className="size-3 animate-pulse text-primary" />
                          <span>Assistant's Thought Process</span>
                        </div>
                      </AccordionTrigger>
                      <AccordionContent className="pt-3 px-3 text-sm text-muted-foreground border-l-2 border-primary/30 ml-2 italic">
                         <div className="whitespace-pre-wrap font-serif opacity-80">{message.reasoning}</div>
                      </AccordionContent>
                    </AccordionItem>
                  </Accordion>
                )}

                {/* Tool Calls */}
                {message.tool_calls?.map((tc, idx) => (
                  <div key={idx} className="w-full rounded-lg border border-primary/20 bg-primary/5 overflow-hidden text-xs shadow-sm">
                    <div className="flex items-center gap-2 px-3 py-1.5 bg-primary/10 border-b border-primary/10 font-mono font-bold text-primary">
                      <Terminal className="size-3" />
                      <span>{tc.function.name}</span>
                      {message.tool_results?.[tc.id] && <CheckCircle2 className="size-3 text-green-500 ml-auto" />}
                    </div>
                    <div className="p-2 font-mono text-[10px] opacity-70 truncate max-w-full" title={tc.function.arguments}>
                      {tc.function.arguments}
                    </div>
                    {message.tool_results?.[tc.id] && (
                       <Accordion type="single" collapsible className="w-full border-t border-primary/5">
                          <AccordionItem value="result" className="border-none">
                            <AccordionTrigger className="py-1 px-3 text-[10px] hover:no-underline text-primary/60">
                               View Tool Result
                            </AccordionTrigger>
                            <AccordionContent className="p-3 bg-muted/20 max-h-40 overflow-auto border-t border-primary/5">
                               <pre className="text-[10px] whitespace-pre-wrap font-mono">
                                  {message.tool_results[tc.id]}
                               </pre>
                            </AccordionContent>
                          </AccordionItem>
                       </Accordion>
                    )}
                  </div>
                ))}

                {/* Content Block */}
                <div
                  className={cn(
                    "rounded-2xl px-5 py-3 shadow-md border leading-relaxed",
                    message.role === "user" 
                      ? "bg-[#2563eb] text-white border-blue-600 font-medium" 
                      : "bg-card text-card-foreground border-muted"
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
                             <div className="size-1.5 bg-muted-foreground/30 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                             <div className="size-1.5 bg-muted-foreground/30 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                             <div className="size-1.5 bg-muted-foreground/30 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                          </div>
                       ) : null
                    )
                  ) : (
                    <pre className="text-xs overflow-auto">
                      {JSON.stringify(message.content, null, 2)}
                    </pre>
                  )}
                </div>
                
                <span className="text-[10px] text-muted-foreground px-1 uppercase font-bold tracking-[0.2em] opacity-40">
                  {message.role}
                </span>
              </div>
            </div>
          ))}
          <div ref={scrollRef} className="h-4" />
        </div>
      </ScrollArea>

      <div className="p-4 border-t bg-muted/10 backdrop-blur-sm">
        <form
          onSubmit={(e) => {
            e.preventDefault()
            handleSend()
          }}
          className="relative flex items-center gap-3 max-w-3xl mx-auto"
        >
          <div className="relative flex-1 group">
            <Input
              placeholder="Query intelligence repository..."
              value={input}
              onChange={(e) => setInput(e.target.value)}
              disabled={isStreaming}
              className="pr-12 h-13 bg-background/80 border-muted focus-visible:ring-primary shadow-inner rounded-xl transition-all group-focus-within:shadow-lg"
            />
            <div className="absolute inset-y-0 right-3 flex items-center pointer-events-none text-muted-foreground opacity-30 group-focus-within:opacity-100 transition-opacity">
              <kbd className="hidden sm:inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium">
                Enter
              </kbd>
            </div>
          </div>
          <Button
            type="submit"
            size="icon"
            disabled={!input.trim() || isStreaming}
            className="size-13 rounded-xl shadow-xl transition-all active:scale-95 hover:shadow-primary/20 bg-blue-600 hover:bg-blue-700 text-white"
          >
            {isStreaming ? (
              <Loader2 className="size-5 animate-spin" />
            ) : (
              <Send className="size-5" />
            )}
          </Button>
        </form>
        <div className="flex items-center justify-center gap-4 mt-3 opacity-40 hover:opacity-100 transition-opacity">
           <p className="text-[10px] text-center text-muted-foreground flex items-center gap-1.5">
             <Terminal className="size-3" />
             Autonomous Agents Enabled
           </p>
           <div className="w-1 h-1 bg-muted-foreground rounded-full" />
           <p className="text-[10px] text-center text-muted-foreground flex items-center gap-1.5">
             <Brain className="size-3" />
             Cognitive RAG Active
           </p>
        </div>
      </div>
    </div>
  )
}
