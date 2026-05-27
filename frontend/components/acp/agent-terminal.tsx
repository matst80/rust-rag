"use client";

import { MessageSquare, Terminal as TerminalIcon, X, Minimize2, Maximize2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { TerminalView } from "./terminal-view";

interface AgentTerminalProps {
  terminalId: string;
  isFullScreen: boolean;
  onToggleFullScreen: () => void;
  onClose: () => void;
  onInput: (data: string) => void;
  onResize: (cols: number, rows: number) => void;
  onAttach: (cols: number, rows: number) => void;
  sessionTerminals?: string[];
  activeTerminalId?: string;
  onSelectTerminal?: (tid: string) => void;
  onShowChat?: () => void;
}

export function AgentTerminal({
  terminalId,
  isFullScreen,
  onToggleFullScreen,
  onClose,
  onInput,
  onResize,
  onAttach,
  sessionTerminals,
  activeTerminalId,
  onSelectTerminal,
  onShowChat,
}: AgentTerminalProps) {
  return (
    <div className="flex flex-col h-full bg-muted/5">
      {/* TERMINAL TABS */}
      {sessionTerminals && sessionTerminals.length > 0 && (
        <div className="flex items-center gap-1 border-b border-border bg-muted/20 px-4 py-1.5 overflow-x-auto no-scrollbar shrink-0">
          {onShowChat && (
            <button
              onClick={onShowChat}
              className="flex items-center gap-1.5 rounded px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground hover:bg-muted/40 hover:text-foreground transition-colors"
            >
              <MessageSquare className="size-3" />
              Chat Only
            </button>
          )}
          {sessionTerminals.map((tid, idx) => (
            <div key={tid} className="flex items-center gap-0.5">
              <button
                onClick={() => onSelectTerminal?.(tid)}
                className={cn(
                  "flex items-center gap-1.5 rounded px-2 py-1 text-[10px] font-bold uppercase tracking-wider transition-colors",
                  activeTerminalId === tid
                    ? "bg-background text-primary shadow-sm"
                    : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                )}
              >
                <TerminalIcon className="size-3" />
                Term {idx + 1}
              </button>
              <button
                onClick={onClose}
                className="flex size-5 items-center justify-center rounded text-muted-foreground/40 hover:bg-red-500/10 hover:text-red-500"
              >
                <X className="size-2.5" />
              </button>
            </div>
          ))}

          <div className="flex-1" />

          <button
            onClick={onToggleFullScreen}
            className="flex items-center gap-1.5 rounded px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground hover:bg-muted/40 hover:text-foreground transition-colors"
            title={isFullScreen ? "Exit Full Screen" : "Full Screen"}
          >
            {isFullScreen ? (
              <>
                <Minimize2 className="size-3" />
                Minimize
              </>
            ) : (
              <>
                <Maximize2 className="size-3" />
                Maximize
              </>
            )}
          </button>
        </div>
      )}

      {/* TERMINAL WINDOW */}
      <div className="flex-1 overflow-hidden relative">
        <TerminalView
          key={terminalId}
          terminalId={terminalId}
          onInput={onInput}
          onResize={onResize}
          onAttach={onAttach}
        />
      </div>
    </div>
  );
}
