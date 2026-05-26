"use client";

import { useRef, useLayoutEffect, useCallback } from "react";
import { BlockView } from "./block-view";

const NEAR_BOTTOM_PX = 80;

interface ChatMessagesProps {
  blocks: any[];
  agentCommand?: string;
}

export function ChatMessages({ blocks, agentCommand }: ChatMessagesProps) {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const wasNearBottomRef = useRef(true);

  useLayoutEffect(() => {
    const el = scrollContainerRef.current;
    if (!el || !wasNearBottomRef.current) return;
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [blocks]);

  const onScroll = useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
    wasNearBottomRef.current = dist < NEAR_BOTTOM_PX;
  }, []);

  return (
    <div
      ref={scrollContainerRef}
      onScroll={onScroll}
      className="flex-1 overflow-y-auto px-3 py-3 md:px-6 md:py-4 no-scrollbar"
    >
      {blocks.length === 0 && (
        <div className="text-xs text-muted-foreground italic">
          No events yet
        </div>
      )}
      {blocks.map((b) => (
        <BlockView
          key={b.key}
          block={b}
          sessionAgent={agentCommand}
        />
      ))}
      <div ref={messagesEndRef} />
    </div>
  );
}
