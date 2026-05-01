"use client"

import { Children, type ReactNode } from "react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import { cn } from "@/lib/utils"

const MENTION_TOKEN_RE = /(@[\w.\-]+)/g

interface MessageMarkdownProps {
  text: string
  knownUsers: Set<string>
  selfUser?: string
  className?: string
}

function highlightMentions(
  node: ReactNode,
  knownUsers: Set<string>,
  selfUser?: string,
  keyPrefix = "m"
): ReactNode {
  if (typeof node === "string") {
    if (!node.includes("@")) return node
    const parts = node.split(MENTION_TOKEN_RE)
    return parts.map((part, idx) => {
      if (idx % 2 === 1) {
        const handle = part.slice(1).toLowerCase()
        const known = knownUsers.has(handle)
        const isSelf = !!selfUser && handle === selfUser.toLowerCase()
        if (known || isSelf) {
          return (
            <span
              key={`${keyPrefix}-${idx}`}
              className={cn(
                "rounded px-1 font-medium",
                isSelf
                  ? "bg-amber-500/20 text-amber-700 dark:text-amber-300"
                  : "bg-primary/15 text-primary"
              )}
            >
              {part}
            </span>
          )
        }
      }
      return part
    })
  }
  if (Array.isArray(node)) {
    return node.map((child, idx) =>
      highlightMentions(child, knownUsers, selfUser, `${keyPrefix}-${idx}`)
    )
  }
  return node
}

function withMentions(knownUsers: Set<string>, selfUser?: string) {
  return (children: ReactNode): ReactNode => {
    return Children.map(children, (child, idx) => {
      if (typeof child === "string") {
        return highlightMentions(child, knownUsers, selfUser, `c-${idx}`)
      }
      // React element children with their own renderer (already mapped via
      // components below for the structural ones) — pass through.
      return child
    })
  }
}

export function MessageMarkdown({
  text,
  knownUsers,
  selfUser,
  className,
}: MessageMarkdownProps) {
  const wrap = withMentions(knownUsers, selfUser)
  return (
    <div
      className={cn(
        "prose prose-sm dark:prose-invert max-w-none",
        "prose-p:my-1 prose-p:leading-relaxed",
        "prose-ul:my-1 prose-ol:my-1 prose-li:my-0",
        "prose-headings:my-2 prose-headings:font-semibold",
        "prose-pre:my-2 prose-pre:rounded-md prose-pre:bg-muted prose-pre:p-3 prose-pre:text-xs",
        "prose-code:rounded prose-code:bg-muted prose-code:px-1 prose-code:py-0.5 prose-code:text-[0.85em] prose-code:before:content-none prose-code:after:content-none",
        "prose-blockquote:my-1 prose-blockquote:border-l-2 prose-blockquote:pl-3 prose-blockquote:italic",
        "prose-a:text-primary prose-a:underline-offset-2 hover:prose-a:underline",
        "prose-table:my-2 prose-th:px-2 prose-th:py-1 prose-td:px-2 prose-td:py-1",
        className
      )}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          p: ({ children }) => <p>{wrap(children)}</p>,
          li: ({ children }) => <li>{wrap(children)}</li>,
          em: ({ children }) => <em>{wrap(children)}</em>,
          strong: ({ children }) => <strong>{wrap(children)}</strong>,
          h1: ({ children }) => <h1>{wrap(children)}</h1>,
          h2: ({ children }) => <h2>{wrap(children)}</h2>,
          h3: ({ children }) => <h3>{wrap(children)}</h3>,
          h4: ({ children }) => <h4>{wrap(children)}</h4>,
          h5: ({ children }) => <h5>{wrap(children)}</h5>,
          h6: ({ children }) => <h6>{wrap(children)}</h6>,
          td: ({ children }) => <td>{wrap(children)}</td>,
          th: ({ children }) => <th>{wrap(children)}</th>,
          blockquote: ({ children }) => <blockquote>{wrap(children)}</blockquote>,
          a: ({ children, href }) => (
            <a href={href} target="_blank" rel="noopener noreferrer">
              {wrap(children)}
            </a>
          ),
          // Inline code stays mention-free; block code stays raw.
          code: ({ className, children, ...props }: any) => (
            <code className={className} {...props}>
              {children}
            </code>
          ),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  )
}

