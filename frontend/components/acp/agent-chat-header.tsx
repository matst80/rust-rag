"use client";

import { Menu, Bot, Terminal as TerminalIcon, FolderSearch, Link2, Plus, Trash2, X } from "lucide-react";
import { cn } from "@/lib/utils";

interface AgentChatHeaderProps {
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
  title: string;
  subtitle?: string;
  onBindTelegram?: () => void;
  isTelegramBound?: boolean;
  viewMode: "chat" | "terminal";
  setViewMode: (mode: "chat" | "terminal") => void;
  onCreateTerminal?: () => void;
  onOpenFiles?: () => void;
  onTerminate?: () => void;
  isStandalone?: boolean;
}

export function AgentChatHeader({
  sidebarOpen,
  setSidebarOpen,
  title,
  subtitle,
  onBindTelegram,
  isTelegramBound,
  viewMode,
  setViewMode,
  onCreateTerminal,
  onOpenFiles,
  onTerminate,
  isStandalone = false,
}: AgentChatHeaderProps) {
  return (
    <header className="flex items-center gap-2 border-b border-border px-3 py-2 md:px-6 md:py-3 shrink-0">
      <button
        type="button"
        onClick={() => setSidebarOpen(!sidebarOpen)}
        className="mr-1 flex size-10 md:size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
        aria-label="Toggle sidebar"
        title={sidebarOpen ? "Hide sidebar" : "Show sidebar"}
      >
        <Menu
          className={cn(
            "size-4 transition-transform",
            !sidebarOpen && "rotate-90",
          )}
        />
      </button>
      {isStandalone ? (
        <TerminalIcon className="size-4 shrink-0 text-muted-foreground" />
      ) : (
        <Bot className="size-4 shrink-0 text-muted-foreground" />
      )}
      <div className="flex flex-1 flex-col min-w-0">
        <div className="flex items-center gap-1.5 min-w-0">
          <span className="truncate text-sm font-medium">{title}</span>
        </div>
        {subtitle && (
          <span className="truncate font-mono text-[10px] text-muted-foreground/60">
            {subtitle}
          </span>
        )}
      </div>
      <div className="flex items-center gap-1">
        {!isStandalone && onBindTelegram && (
          <button
            type="button"
            onClick={onBindTelegram}
            className={cn(
              "flex size-10 md:size-8 items-center justify-center rounded-md hover:bg-muted/40 hover:text-foreground",
              isTelegramBound ? "text-emerald-500" : "text-muted-foreground",
            )}
            title={isTelegramBound ? "Telegram thread bound (click to rebind)" : "Bind to Telegram thread"}
          >
            <Link2 className="size-4" />
          </button>
        )}

        {onOpenFiles && (
          <button
            type="button"
            onClick={onOpenFiles}
            className="flex size-10 md:size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            title="Browse remote files"
            aria-label="Browse remote files"
          >
            <FolderSearch className="size-4" />
          </button>
        )}

        {!isStandalone && (
          <button
            type="button"
            onClick={() => setViewMode(viewMode === "chat" ? "terminal" : "chat")}
            className={cn(
              "flex size-10 md:size-8 items-center justify-center rounded-md hover:bg-muted/40 hover:text-foreground",
              viewMode === "terminal" ? "text-primary" : "text-muted-foreground",
            )}
            title="Toggle Terminal"
          >
            <TerminalIcon className="size-4" />
          </button>
        )}

        {onCreateTerminal && (
          <button
            type="button"
            onClick={onCreateTerminal}
            className="flex size-10 md:size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            title="New Terminal"
          >
            <Plus className="size-4" />
          </button>
        )}

        {onTerminate && (
          <button
            type="button"
            onClick={onTerminate}
            className={cn(
              "flex size-10 md:size-8 items-center justify-center rounded-md text-muted-foreground",
              isStandalone ? "hover:bg-red-500/10 hover:text-red-500" : "hover:bg-destructive/10 hover:text-destructive"
            )}
            title={isStandalone ? "Close Terminal" : "Terminate Session"}
          >
            {isStandalone ? <X className="size-4" /> : <Trash2 className="size-4" />}
          </button>
        )}
      </div>
    </header>
  );
}
