"use client"

import React, { useState } from "react"
import {
  MessageSquare,
  Send,
  Trash2,
  Bot,
  User,
  Clock,
  CornerDownRight,
  Sparkles,
  Quote,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import {
  useChannelMessages,
  useSendMessage,
  useDeleteMessage,
} from "@/lib/api"
import { formatRelativeTime } from "@/lib/utils"
import { toast } from "sonner"
import type { Message } from "@/lib/api/types"

interface CommentsPanelProps {
  itemId: string
  entryTitle?: string
  compact?: boolean
  className?: string
}

export function CommentsPanel({
  itemId,
  entryTitle,
  compact = false,
  className = "",
}: CommentsPanelProps) {
  // Dedicated channel for node discussions
  const channel = `entry:${itemId}`
  const { data: messagesResp, mutate } = useChannelMessages(channel)
  const { trigger: sendMessage, isMutating: isSending } = useSendMessage()
  const { trigger: deleteMessage } = useDeleteMessage()

  const [commentText, setCommentText] = useState("")
  const [replyToId, setReplyToId] = useState<string | null>(null)

  const messages = messagesResp?.messages ?? []

  const handlePostComment = async (e?: React.FormEvent) => {
    e?.preventDefault()
    const text = commentText.trim()
    if (!text || isSending) return

    try {
      await sendMessage({
        channel,
        text,
        kind: "text",
        metadata: {
          item_id: itemId,
          entry_title: entryTitle,
          ...(replyToId && { reply_to: replyToId }),
        },
      })
      setCommentText("")
      setReplyToId(null)
      mutate()
      toast.success("Comment connected to node")
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to post comment")
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deleteMessage(id)
      mutate()
      toast.success("Comment deleted")
    } catch {
      toast.error("Failed to delete comment")
    }
  }

  return (
    <div className={`space-y-4 animate-in fade-in duration-300 ${className}`}>
      {!compact && (
        <div className="flex items-center justify-between pt-4 border-t border-border">
          <div className="flex items-center gap-2">
            <MessageSquare className="size-4 text-primary" />
            <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
              Node Discussions & Comments ({messages.length})
            </h2>
          </div>
          <span className="font-mono text-[10px] text-muted-foreground/60">
            channel: entry:{itemId.slice(0, 8)}…
          </span>
        </div>
      )}

      {/* Comments list */}
      <div className="space-y-2.5">
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center p-6 rounded-xl border border-dashed border-border text-center bg-muted/10">
            <MessageSquare className="size-6 text-muted-foreground/30 mb-2" />
            <p className="font-mono text-xs text-muted-foreground">
              No comments connected to this node yet.
            </p>
            <p className="text-[11px] text-muted-foreground/60 mt-1">
              Add discussion notes, review questions, or instructions for agents.
            </p>
          </div>
        ) : (
          messages.map((m: Message) => {
            const isAgent = m.sender_kind === "agent"
            const parentId = m.metadata?.reply_to as string | undefined
            return (
              <div
                key={m.id}
                className="group relative flex flex-col p-3 rounded-lg border border-border bg-card transition-colors hover:border-primary/40 space-y-1.5"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <Avatar className="size-5 text-[9px] font-mono">
                      <AvatarFallback
                        className={
                          isAgent
                            ? "bg-primary/10 text-primary"
                            : "bg-muted text-foreground"
                        }
                      >
                        {isAgent ? (
                          <Bot className="size-3" />
                        ) : (
                          <User className="size-3" />
                        )}
                      </AvatarFallback>
                    </Avatar>
                    <span className="font-mono text-xs font-semibold text-foreground truncate">
                      {m.sender}
                    </span>
                    {isAgent && (
                      <span className="font-mono text-[8px] uppercase px-1.5 py-0.2 rounded border border-primary/20 bg-primary/5 text-primary shrink-0">
                        Agent
                      </span>
                    )}
                    <span className="flex items-center gap-1 font-mono text-[9px] text-muted-foreground/50 shrink-0">
                      <Clock className="size-2.5" />
                      {formatRelativeTime(m.created_at)}
                    </span>
                  </div>

                  <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-6 text-muted-foreground hover:text-foreground"
                      onClick={() => {
                        setReplyToId(m.id)
                        setCommentText((prev) =>
                          prev ? `${prev} ` : `@${m.sender} `
                        )
                      }}
                      title="Reply"
                    >
                      <CornerDownRight className="size-3" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-6 text-muted-foreground hover:text-destructive"
                      onClick={() => handleDelete(m.id)}
                      title="Delete comment"
                    >
                      <Trash2 className="size-3" />
                    </Button>
                  </div>
                </div>

                {parentId && (
                  <div className="flex items-center gap-1.5 text-[9px] font-mono text-muted-foreground/60 pl-2 border-l-2 border-primary/30">
                    <CornerDownRight className="size-2" />
                    <span>replying to thread</span>
                  </div>
                )}

                {typeof m.metadata?.quote === "string" && m.metadata.quote && (
                  <div className="flex items-start gap-1.5 text-[11px] font-mono text-muted-foreground bg-muted/40 border-l-2 border-amber-500/70 px-2 py-1 rounded-r italic my-1 ml-7">
                    <Quote className="size-3 shrink-0 opacity-50 mt-0.5" />
                    <span className="line-clamp-2">{m.metadata.quote}</span>
                  </div>
                )}

                <div className="text-xs font-sans text-foreground/90 whitespace-pre-wrap leading-relaxed pl-7">
                  {m.text}
                </div>
              </div>
            )
          })
        )}
      </div>

      {/* New comment form */}
      <form onSubmit={handlePostComment} className="space-y-2 pt-1">
        {replyToId && (
          <div className="flex items-center justify-between text-xs font-mono text-primary bg-primary/5 px-2.5 py-1 rounded border border-primary/20">
            <span className="flex items-center gap-1.5 text-[11px]">
              <CornerDownRight className="size-3" />
              Replying to #{replyToId.slice(0, 8)}…
            </span>
            <button
              type="button"
              onClick={() => setReplyToId(null)}
              className="text-muted-foreground hover:text-foreground text-[9px] uppercase font-bold"
            >
              Cancel
            </button>
          </div>
        )}

        <Textarea
          value={commentText}
          onChange={(e) => setCommentText(e.target.value)}
          placeholder="Leave a comment or note for this node..."
          rows={compact ? 2 : 3}
          className="text-xs bg-card border-border focus-visible:border-primary focus-visible:ring-0 resize-none font-sans"
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault()
              handlePostComment()
            }
          }}
        />

        <div className="flex items-center justify-between">
          <span className="text-[9px] font-mono text-muted-foreground/50">
            ⌘+Enter to submit
          </span>
          <Button
            type="submit"
            size="sm"
            disabled={!commentText.trim() || isSending}
            className="h-7 px-2.5 gap-1.5 font-mono text-[9px] font-bold uppercase tracking-wider"
          >
            <Send className="size-3" />
            <span>Comment</span>
          </Button>
        </div>
      </form>
    </div>
  )
}
