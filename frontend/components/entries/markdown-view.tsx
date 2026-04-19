"use client"

import { cn } from "@/lib/utils"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter"
import { atomDark } from "react-syntax-highlighter/dist/esm/styles/prism"
import { useState } from "react"
import { Check, Copy } from "lucide-react"

interface MarkdownViewProps {
  content: string
  className?: string
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <button
      onClick={copy}
      className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/5 px-2.5 py-1.5 text-xs text-white/50 backdrop-blur-sm transition-all hover:bg-white/10 hover:text-white active:scale-95"
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
}

export function MarkdownView({ content, className }: MarkdownViewProps) {
  return (
    <div className={cn("prose prose-sm dark:prose-invert max-w-none", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          pre({ children }) {
            return <>{children}</>
          },
          code({ node, inline, className, children, ...props }: any) {
            const match = /language-(\w+)/.exec(className || "")
            const language = match ? match[1] : ""
            const isBlock = !inline && (language || String(children).includes("\n"))
            const value = String(children).replace(/\n$/, "")

            if (isBlock) {
              return (
                <div className="group relative my-8 overflow-hidden rounded-2xl border border-white/10 bg-[#0d1117]/80 shadow-2xl backdrop-blur-sm transition-all hover:border-white/20">
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
  )
}
