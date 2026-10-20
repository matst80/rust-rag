"use client"

import Link from "next/link"
import { cn, entryTitle } from "@/lib/utils"
import { useItem, useSearch } from "@/lib/api"
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

export type WikiSegment =
  | { kind: "md"; text: string }
  | { kind: "wiki"; target: string; label: string; raw: string }

const WIKILINK_RE = /\[\[([^\]|\]]+)(?:\|([^\]]+))?\]\]/g

/**
 * Split Obsidian-style wikilinks (`[[entry-id]]` or `[[title|Label]]`) out
 * of markdown so they can resolve against the store. Fenced code blocks
 * and inline code spans stay plain text so examples don't linkify.
 *
 * Consecutive markdown pieces are merged back into a single segment so
 * each paragraph renders through one <ReactMarkdown> instance. Without
 * this, every inline `code` span becomes its own <p> (or, worse, trips
 * the block-code heuristic) and sentences render as stacked blocks.
 */
export function splitWikilinks(markdown: string): WikiSegment[] {
  const raw: WikiSegment[] = []
  const pushMd = (text: string) => {
    if (!text) return
    const last = raw[raw.length - 1]
    if (last?.kind === "md") last.text += text
    else raw.push({ kind: "md", text })
  }
  const fenceSplit = markdown.split(/(```[\s\S]*?(?:```|$))/g)
  fenceSplit.forEach((chunk, i) => {
    if (i % 2 === 1) {
      pushMd(chunk) // fenced block — untouched
      return
    }
    chunk.split(/(`[^`\n]*`)/g).forEach((span, j) => {
      if (j % 2 === 1 || !span) {
        if (span) pushMd(span)
        return // inline code — untouched
      }
      let last = 0
      WIKILINK_RE.lastIndex = 0
      let m: RegExpExecArray | null
      while ((m = WIKILINK_RE.exec(span)) !== null) {
        if (m.index > last) pushMd(span.slice(last, m.index))
        const target = m[1].trim()
        const label = (m[2] ?? target).trim().replace(/\]/g, "")
        if (target) raw.push({ kind: "wiki", target, label, raw: m[0] })
        else pushMd(m[0])
        last = m.index + m[0].length
      }
      if (last < span.length) pushMd(span.slice(last))
    })
  })
  return raw.filter((s) => (s.kind === "md" ? s.text.length > 0 : true))
}

export function stripWikilinks(markdown: string): string {
  return markdown.replace(
    WIKILINK_RE,
    (_m: string, target: string, label?: string) => ((label ?? target) as string).trim().replace(/\]/g, "")
  )
}

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
        <ReactMarkdown remarkPlugins={[remarkGfm, remarkWikilinks, remarkCallouts]} components={markdownComponents}>
          {content ?? ""}
        </ReactMarkdown>
      </div>
    </div>
  )
}

/**
 * Resolve one `[[target]]` against the store: exact id first, then a
 * title search for fuzzy `[[Some Title]]` links. Unresolvable targets
 * render as muted brackets (Obsidian-style dangling link).
 */
function Wikilink({ target, label }: { target: string; label: string }) {
  const { data: exact, error } = useItem(target)
  // Title search only fires once the exact lookup 404s — no wasted fetch.
  const { data: results } = useSearch(!exact && error ? target : "", undefined, undefined, true, 3)
  const fuzzy = !exact && !error ? null : (results?.results ?? []).find((r) => r.id !== target) ?? null

  if (exact) {
    return (
      <Link
        href={`/entries/${encodeURIComponent(exact.id)}`}
        prefetch={false}
        className="text-primary underline decoration-primary/40 underline-offset-2 hover:decoration-primary"
        title={exact.id}
      >
        {label === target ? entryTitle(exact) : label}
      </Link>
    )
  }
  if (fuzzy) {
    return (
      <Link
        href={`/entries/${encodeURIComponent(fuzzy.id)}`}
        prefetch={false}
        className="text-primary/80 underline decoration-dotted decoration-primary/40 underline-offset-2 hover:decoration-primary"
        title={`Resolved by title search → ${fuzzy.id}`}
      >
        {label}
      </Link>
    )
  }
  return (
    <span className="font-mono text-[0.9em] text-muted-foreground/70" title="No entry found for this link">
      [[{label}]]
    </span>
  )
}

function hastCodeText(nodes: any[] | undefined): string {
  if (!nodes) return ""
  let out = ""
  for (const n of nodes) {
    if (n?.type === "text") out += String(n.value ?? "")
    else if (Array.isArray(n?.children)) out += hastCodeText(n.children)
  }
  return out
}

function hastCodeMeta(node: any): { language: string; value: string } | null {
  const codeNode = node?.children?.find((c: any) => c?.tagName === "code") ?? node?.children?.[0]
  if (!codeNode || codeNode.tagName !== "code") return null
  const cls = codeNode.properties?.className
  const flat: string[] = Array.isArray(cls) ? cls.flatMap((c: any) => String(c).split(/\s+/)) : []
  const langCls = flat.find((c) => c.startsWith("language-"))
  const language = langCls ? langCls.slice("language-".length) : ""
  return { language, value: hastCodeText(codeNode.children).replace(/\n$/, "") }
}

function CodeBlockChrome({ language, value }: { language: string; value: string }) {
  if (language === "mermaid") return <MermaidBlock code={value} />
  return (
    <div className="group relative my-8 overflow-hidden rounded-2xl border border-white/10 bg-[#0d1117]/80 shadow-2xl transition-colors hover:border-white/20">
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
              <span className="text-[11px] font-black uppercase tracking-[0.2em] text-white/40">{language}</span>
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
        >
          {value}
        </SyntaxHighlighter>
      </div>
    </div>
  )
}

function remarkWikilinks() {
  return (tree: any) => {
    const visit = (node: any) => {
      if (!node.children || node.type === "link" || node.type === "linkReference") return;
      node.children = node.children.flatMap((child: any) => {
        if (child.type !== "text") { visit(child); return [child]; }
        const parts: any[] = [];
        const pattern = /\[\[([^\]|\]]+)(?:\|([^\]]+))?\]\]/g;
        let last = 0; let match;
        while ((match = pattern.exec(child.value))) {
          if (match.index > last) parts.push({ type: "text", value: child.value.slice(last, match.index) });
          const target = match[1].trim();
          parts.push({ type: "link", url: `#entry-wiki=${encodeURIComponent(target)}`, children: [{ type: "text", value: (match[2] ?? target).trim() }] });
          last = match.index + match[0].length;
        }
        if (!last) return [child];
        if (last < child.value.length) parts.push({ type: "text", value: child.value.slice(last) });
        return parts;
      });
    };
    visit(tree);
  };
}

// Transform the markdown tree so formatting, links and nested lists survive.
function remarkCallouts() {
  return (tree: any) => {
    const visit = (node: any) => {
      if (node.type === "blockquote") {
        const paragraph = node.children?.[0];
        const first = paragraph?.children?.[0];
        const match = first?.type === "text" && /^\[!(NOTE|TIP|WARNING|IMPORTANT|CAUTION|TOGGLE)\]([^\n]*)(?:\n|$)/i.exec(first.value);
        if (match) {
          const toggle = match[1].toUpperCase() === "TOGGLE";
          first.value = first.value.slice(match[0].length);
          node.data = { hName: toggle ? "details" : "aside", hProperties: { className: toggle ? "my-4 rounded-lg border p-4" : "my-4 rounded-lg border-l-4 border-primary bg-primary/5 px-4 py-3" } };
          node.children.unshift({ type: "paragraph", data: { hName: toggle ? "summary" : "strong", hProperties: { className: "cursor-pointer font-semibold" } }, children: [{ type: "text", value: match[2].trim() || match[1] }] });
        }
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

const markdownComponents = {
            pre({ node, children }: any) {
              const meta = hastCodeMeta(node)
              if (meta) return <CodeBlockChrome language={meta.language} value={meta.value} />
              return <pre>{children}</pre>
            },
            a({ href, children }: any) {
              if (href?.startsWith("#entry-wiki=")) return <Wikilink target={decodeURIComponent(href.slice(12))} label={String(children)} />;
              // Internal entry links navigate client-side instead of a full reload.
              if (href?.startsWith("/entries/")) {
                return (
                  <Link href={href} prefetch={false}>
                    {children}
                  </Link>
                )
              }
              return <a href={href}>{children}</a>
            },
            code({ node: _node, className, children, ...props }: any) {
              // Fenced blocks render via `pre` above, so anything reaching
              // `code` is inline `like this` by construction. The language
              // fallback covers a fenced block that slips past `pre`.
              const match = /language-([\w-]+)/.exec(className || "")
              const language = match ? match[1] : ""
              const value = String(children).replace(/\n$/, "")
              if (language) {
                return <CodeBlockChrome language={language} value={value} />
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
          }

export const MarkdownView = memo(MarkdownViewInner)
