"use client"

import { cn } from "@/lib/utils"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter"
import { atomDark } from "react-syntax-highlighter/dist/esm/styles/prism"
import { useState, useRef, useEffect, memo } from "react"
import { Check, Copy, MessageSquare } from "lucide-react"
import { MermaidBlock } from "./mermaid-block"
import { InlineCommentPopover } from "./inline-comment-popover"

interface MarkdownViewProps {
  content: string
  className?: string
  onAddComment?: (comment: string, quote: string) => Promise<void>
}

const CopyButton = memo(function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <button
      onClick={copy}
      className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/5 px-2.5 py-1.5 text-xs text-white/50 transition-colors hover:bg-white/10 hover:text-white active:scale-95"
      title="Copy to clipboard"
    >
      {copied ? (
        <>
          <Check className="size-3" />
          <span>Copied!</span>
        </>
      ) : (
        <>
          <Copy className="size-3" />
          <span>Copy</span>
        </>
      )}
    </button>
  )
})

function MarkdownViewInner({ content, className, onAddComment }: MarkdownViewProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [selectionBubble, setSelectionBubble] = useState<{
    text: string
    top: number
    left: number
  } | null>(null)
  const [popoverOpen, setPopoverOpen] = useState(false)

  useEffect(() => {
    if (!onAddComment) return

    const handleMouseUp = () => {
      // Delay slightly to let browser update window.getSelection()
      setTimeout(() => {
        const selection = window.getSelection()
        if (!selection || selection.isCollapsed) {
          if (!popoverOpen) setSelectionBubble(null)
          return
        }

        const text = selection.toString().trim()
        if (!text || text.length < 2) {
          if (!popoverOpen) setSelectionBubble(null)
          return
        }

        const range = selection.getRangeAt(0)
        const rect = range.getBoundingClientRect()
        if (rect.width === 0 && rect.height === 0) return

        // Check if selection is within our container
        if (containerRef.current && containerRef.current.contains(range.commonAncestorContainer)) {
          setSelectionBubble({
            text,
            top: rect.top - 38,
            left: rect.left + rect.width / 2,
          })
        }
      }, 20)
    }

    const handleSelectionChange = () => {
      const selection = window.getSelection()
      if (!selection || selection.isCollapsed) {
        if (!popoverOpen) setSelectionBubble(null)
      }
    }

    document.addEventListener("mouseup", handleMouseUp)
    document.addEventListener("selectionchange", handleSelectionChange)

    return () => {
      document.removeEventListener("mouseup", handleMouseUp)
      document.removeEventListener("selectionchange", handleSelectionChange)
    }
  }, [onAddComment, popoverOpen])

  return (
    <div ref={containerRef} className="relative">
      {/* Floating Comment Button when text is selected */}
      {selectionBubble && !popoverOpen && onAddComment && (
        <button
          type="button"
          onMouseDown={(e) => {
            e.preventDefault() // prevent selection clear
            setPopoverOpen(true)
          }}
          className="fixed z-40 -translate-x-1/2 flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-primary text-primary-foreground text-xs font-mono font-medium shadow-lg hover:scale-105 active:scale-95 transition-transform"
          style={{
            top: `${Math.max(10, selectionBubble.top)}px`,
            left: `${selectionBubble.left}px`,
          }}
        >
          <MessageSquare className="size-3" />
          <span>Comment</span>
        </button>
      )}

      {/* Popover form */}
      {selectionBubble && popoverOpen && onAddComment && (
        <InlineCommentPopover
          selectedText={selectionBubble.text}
          position={{
            top: selectionBubble.top + 40,
            left: selectionBubble.left - 150,
          }}
          onSubmit={async (comment, quote) => {
            await onAddComment(comment, quote)
            setPopoverOpen(false)
            setSelectionBubble(null)
          }}
          onClose={() => {
            setPopoverOpen(false)
            setSelectionBubble(null)
          }}
        />
      )}

      <div
        className={cn(
          "prose prose-lg dark:prose-invert max-w-none",
          "font-serif leading-relaxed prose-headings:font-sans prose-headings:font-bold",
          "prose-p:leading-[1.75] prose-li:leading-[1.75]",
          "prose-code:before:content-none prose-code:after:content-none",
          className
        )}
      >
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          components={{
            pre({ children }) {
              return <>{children}</>
            },
            code({ node, className, children, ...props }: any) {
              const match = /language-(\w+)/.exec(className || "")
              const language = match ? match[1] : ""
              const value = String(children).replace(/\n$/, "")
              const isBlock = Boolean(language) || value.includes("\n")

              if (isBlock && language === "mermaid") {
                return <MermaidBlock code={value} />
              }

              if (isBlock) {
                return (
                  <div className="group relative my-8 overflow-hidden rounded-2xl border border-white/10 bg-[#0d1117]/80 shadow-2xl transition-colors hover:border-white/20">
                    {/* Terminal Header */}
                    <div className="flex h-12 items-center justify-between border-b border-white/5 bg-gradient-to-r from-white/[0.08] to-transparent px-4">
                      <div className="flex items-center gap-3">
                        <div className="flex gap-2">
                          <div className="size-3 rounded-full bg-[#ff5f56]/90 shadow-[0_0_10px_rgba(255,95,86,0.3)] transition-transform group-hover:scale-110" />
                          <div className="size-3 rounded-full bg-[#ffbd2e]/90 shadow-[0_0_10px_rgba(255,189,46,0.3)] transition-transform group-hover:scale-110" />
                          <div className="size-3 rounded-full bg-[#27c93f]/90 shadow-[0_0_10px_rgba(39,201,63,0.3)] transition-transform group-hover:scale-110" />
                        </div>
                        {language && (
                          <div className="ml-4 flex items-center gap-2">
                            <span className="h-4 w-px bg-white/10" />
                            <span className="text-[11px] font-black uppercase tracking-[0.2em] text-white/40">
                              {language}
                            </span>
                          </div>
                        )}
                      </div>
                      <CopyButton text={value} />
                    </div>

                    <div className="relative">
                      <SyntaxHighlighter
                        style={atomDark}
                        language={language || "text"}
                        PreTag="div"
                        className="!m-0 !bg-transparent !p-7"
                        codeTagProps={{
                          style: {
                            fontSize: "0.9rem",
                            lineHeight: "1.7",
                            fontFamily: 'var(--font-mono), ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
                            letterSpacing: "-0.01em",
                          },
                        }}
                        {...props}
                      >
                        {value}
                      </SyntaxHighlighter>
                    </div>
                  </div>
                )
              }

              return (
                <code
                  className={cn(
                    "rounded-md bg-white/5 px-1.5 py-0.5 font-mono text-sm text-primary/90 border border-white/10",
                    className
                  )}
                  {...props}
                >
                  {children}
                </code>
              )
            },
          }}
        >
          {content}
        </ReactMarkdown>
      </div>
    </div>
  )
}

export const MarkdownView = memo(MarkdownViewInner)
