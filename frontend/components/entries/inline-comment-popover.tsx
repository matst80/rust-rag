"use client";

import React, { useState } from "react";
import { MessageSquare, Send, X, CornerDownRight, Quote } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

interface InlineCommentPopoverProps {
  selectedText: string;
  position: { top: number; left: number };
  onSubmit: (comment: string, quote: string) => Promise<void>;
  onClose: () => void;
}

export function InlineCommentPopover({
  selectedText,
  position,
  onSubmit,
  onClose,
}: InlineCommentPopoverProps) {
  const [comment, setComment] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!comment.trim() || isSubmitting) return;

    try {
      setIsSubmitting(true);
      await onSubmit(comment.trim(), selectedText);
      onClose();
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div
      className="fixed z-50 animate-in fade-in zoom-in-95 duration-150 rounded-xl border border-border bg-popover/95 backdrop-blur-md p-3 shadow-2xl w-80 max-w-[calc(100vw-2rem)]"
      style={{
        top: `${Math.max(10, Math.min(position.top, window.innerHeight - 260))}px`,
        left: `${Math.max(10, Math.min(position.left, window.innerWidth - 330))}px`,
      }}
    >
      <div className="flex items-center justify-between pb-2 border-b border-border/50 mb-2">
        <div className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <MessageSquare className="size-3.5 text-primary" />
          <span>Comment on selection</span>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="size-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
        >
          <X className="size-3.5" />
        </button>
      </div>

      <div className="mb-2 p-2 rounded bg-muted/40 border border-border/40 text-[11px] font-mono text-muted-foreground line-clamp-2 italic flex gap-1.5 items-start">
        <Quote className="size-3 shrink-0 opacity-50 mt-0.5" />
        <span className="truncate">{selectedText}</span>
      </div>

      <form onSubmit={handleSubmit} className="space-y-2">
        <Textarea
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          placeholder="Add an inline comment or question…"
          className="min-h-[70px] text-xs resize-none"
          autoFocus
        />
        <div className="flex items-center justify-end gap-1.5">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 text-xs"
            onClick={onClose}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            size="sm"
            disabled={!comment.trim() || isSubmitting}
            className="h-7 text-xs font-mono gap-1"
          >
            <Send className="size-3" />
            Comment
          </Button>
        </div>
      </form>
    </div>
  );
}
