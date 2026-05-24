"use client";

import { useMemo, useRef, useLayoutEffect } from "react";
import {
  Bot,
  Circle,
  Hash,
  Loader2,
  Menu,
  MessageSquare,
  Plus,
  Send,
  Terminal as TerminalIcon,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { WhisperTranscribe } from "@/components/entries/whisper-transcribe";
import { TerminalView } from "./terminal-view";
import { BlockView } from "./block-view";
import { SessionSidebar } from "./session-sidebar";
import { SpawnDialog } from "./spawn-dialog";
import { useAcpSocket } from "./use-acp-socket";
import { buildBlocks } from "./utils";
import { Block, EMPTY_ARRAY } from "./types";

const NEAR_BOTTOM_PX = 80;

export function AgentChat() {
  const {
    conn,
    sessions,
    eventsBySession,
    activeSessionId,
    setActiveSessionId,
    sessionTerminals,
    terminalEvents,
    activeTerminalId,
    setActiveTerminalId,
    viewMode,
    setViewMode,
    pendingPermissions,
    instances,
    activeInstance,
    selectInstance,
    workers,
    projects,
    send,
    drafts,
    setDraft,
    sidebarOpen,
    setSidebarOpen,
  } = useAcpSocket();

  const active = activeSessionId ? sessions[activeSessionId] : null;
  const draft = activeSessionId ? (drafts[activeSessionId] ?? "") : "";

  const activeEvents = useMemo(() => {
    if (!activeSessionId) return EMPTY_ARRAY;
    return eventsBySession[activeSessionId] ?? EMPTY_ARRAY;
  }, [activeSessionId, eventsBySession[activeSessionId || ""]]);

  const prevBlocksRef = useRef<any[]>([]);
  const blocks = useMemo(() => {
    const next = buildBlocks(activeEvents, prevBlocksRef.current);
    prevBlocksRef.current = next;
    return next;
  }, [activeEvents]);

  const pendingForActive = useMemo(() => {
    if (!activeSessionId) return EMPTY_ARRAY;
    return Object.values(pendingPermissions).filter(
      (p) =>
        (p.payload["acp_session_id"] || p.payload["session_id"]) ===
        activeSessionId,
    );
  }, [activeSessionId, pendingPermissions]);

  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const wasNearBottomRef = useRef(true);

  useLayoutEffect(() => {
    const el = scrollContainerRef.current;
    if (!el || !wasNearBottomRef.current) return;
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [blocks]);

  const onScroll = () => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
    wasNearBottomRef.current = dist < NEAR_BOTTOM_PX;
  };

  const sendPrompt = () => {
    if (!activeSessionId || !draft.trim() || conn.status !== "open") return;
    const ok = send({
      type: "send_prompt",
      session_id: activeSessionId,
      text: draft.trim(),
    });
    if (ok) {
      setDraft(activeSessionId, "");
    }
  };

  const executeCommand = (name: string) => {
    if (!activeSessionId) return;
    send({
      type: "send_prompt",
      session_id: activeSessionId,
      text: `/${name}`,
    });
  };

  const createTerminal = (sid: string) => {
    const s = sessions[sid];
    if (!s) return;
    send({
      type: "create_terminal",
      session_id: sid,
      cwd: s.project_path || "/",
      cols: 120,
      rows: 40,
    });
  };

  const closeTerminal = (tid: string) => {
    send({
      type: "close_terminal",
      terminal_id: tid,
    });
  };

  const onTerminalInput = (tid: string, data: string) => {
    send({
      type: "terminal_input",
      terminal_id: tid,
      data,
    });
  };

  const onTerminalResize = (tid: string, cols: number, rows: number) => {
    send({
      type: "terminal_resize",
      terminal_id: tid,
      cols,
      rows,
    });
  };

  return (
    <div className="relative flex h-[calc(100dvh - 49px)] overflow-hidden">
      {/* Mobile Overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 z-30 bg-background/80 backdrop-blur-sm md:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      <SessionSidebar
        sessions={sessions}
        activeSessionId={activeSessionId}
        onSelectSession={setActiveSessionId}
        onSpawn={() => setDraft("spawn", "open")} // Temp hack to show spawn dialog
        onClose={() => setSidebarOpen(false)}
        conn={conn}
        workers={workers}
        instances={instances}
        activeInstance={activeInstance}
        onSelectInstance={selectInstance}
        sidebarOpen={sidebarOpen}
      />

      {/* Thread */}
      <section className="flex min-w-0 flex-1 flex-col">
        {!activeSessionId ? (
          <div className="flex-1 flex flex-col">
            <header className="flex items-center gap-2 border-b border-border px-3 py-2 md:px-6 md:py-3">
              <button
                type="button"
                onClick={() => setSidebarOpen(!sidebarOpen)}
                className="mr-1 flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
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
              <span className="text-sm font-medium text-muted-foreground">
                ACP Agent Sessions
              </span>
            </header>
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
              Select or spawn a session
            </div>
          </div>
        ) : (
          <>
            <header className="flex items-center gap-2 border-b border-border px-3 py-2 md:px-6 md:py-3">
              <button
                type="button"
                onClick={() => setSidebarOpen(!sidebarOpen)}
                className="mr-1 flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
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
              <Bot className="size-4 shrink-0 text-muted-foreground" />
              <div className="flex flex-1 flex-col min-w-0">
                <div className="flex items-center gap-1.5 min-w-0">
                  <span className="truncate text-sm font-medium">
                    {active?.name || active?.project_path || activeSessionId}
                  </span>
                </div>
                <span className="truncate font-mono text-[10px] text-muted-foreground/60">
                  {active?.agent_command} · {active?.project_path}
                </span>
              </div>

              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => {
                    const current = viewMode[activeSessionId] ?? "chat";
                    setViewMode({
                      ...viewMode,
                      [activeSessionId]:
                        current === "chat" ? "terminal" : "chat",
                    });
                  }}
                  className={cn(
                    "flex size-8 items-center justify-center rounded-md hover:bg-muted/40 hover:text-foreground",
                    viewMode[activeSessionId] === "terminal"
                      ? "text-primary"
                      : "text-muted-foreground",
                  )}
                  title="Switch View"
                >
                  {viewMode[activeSessionId] === "terminal" ? (
                    <MessageSquare className="size-4" />
                  ) : (
                    <TerminalIcon className="size-4" />
                  )}
                </button>

                <button
                  type="button"
                  onClick={() => createTerminal(activeSessionId)}
                  className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
                  title="New Terminal"
                >
                  <Plus className="size-4" />
                </button>
              </div>
            </header>

            {activeSessionId &&
              sessionTerminals[activeSessionId] &&
              sessionTerminals[activeSessionId].length > 0 && (
                <div className="flex items-center gap-1 border-b border-border bg-muted/20 px-4 py-1.5 overflow-x-auto no-scrollbar">
                  <button
                    onClick={() =>
                      setViewMode({ ...viewMode, [activeSessionId]: "chat" })
                    }
                    className={cn(
                      "flex items-center gap-1.5 rounded px-2 py-1 text-[10px] font-bold uppercase tracking-wider transition-colors",
                      (viewMode[activeSessionId] ?? "chat") === "chat"
                        ? "bg-background text-primary shadow-sm"
                        : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                    )}
                  >
                    <MessageSquare className="size-3" />
                    Chat
                  </button>
                  {sessionTerminals[activeSessionId].map((tid, idx) => (
                    <div key={tid} className="flex items-center gap-0.5">
                      <button
                        onClick={() => {
                          setActiveTerminalId({
                            ...activeTerminalId,
                            [activeSessionId]: tid,
                          });
                          setViewMode({
                            ...viewMode,
                            [activeSessionId]: "terminal",
                          });
                        }}
                        className={cn(
                          "flex items-center gap-1.5 rounded px-2 py-1 text-[10px] font-bold uppercase tracking-wider transition-colors",
                          viewMode[activeSessionId] === "terminal" &&
                            activeTerminalId[activeSessionId] === tid
                            ? "bg-background text-primary shadow-sm"
                            : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                        )}
                      >
                        <TerminalIcon className="size-3" />
                        Term {idx + 1}
                      </button>
                      <button
                        onClick={() => closeTerminal(tid)}
                        className="flex size-5 items-center justify-center rounded text-muted-foreground/40 hover:bg-red-500/10 hover:text-red-500"
                      >
                        <X className="size-2.5" />
                      </button>
                    </div>
                  ))}
                </div>
              )}

            {viewMode[activeSessionId] === "terminal" &&
            activeTerminalId[activeSessionId] ? (
              <div className="flex-1 p-4 bg-zinc-950 overflow-hidden">
                <TerminalView
                  terminalId={activeTerminalId[activeSessionId]!}
                  onInput={(data) =>
                    onTerminalInput(activeTerminalId[activeSessionId]!, data)
                  }
                  onResize={(cols, rows) =>
                    onTerminalResize(
                      activeTerminalId[activeSessionId]!,
                      cols,
                      rows,
                    )
                  }
                  output={
                    terminalEvents[activeTerminalId[activeSessionId]!]?.output
                  }
                  snapshot={
                    terminalEvents[activeTerminalId[activeSessionId]!]?.snapshot
                  }
                />
              </div>
            ) : (
              <>
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
                      sessionAgent={active?.agent_command}
                    />
                  ))}
                  <div ref={messagesEndRef} />
                </div>

                {pendingForActive.length > 0 && (
                  <div className="border-t border-border bg-amber-500/5 p-4">
                    {/* Permission requests UI - kept minimal for now */}
                    <div className="text-xs font-bold text-amber-600 uppercase tracking-wider mb-2 flex items-center gap-2">
                      <Circle className="size-2 fill-amber-500 animate-pulse" />
                      Pending Permissions
                    </div>
                    <div className="space-y-2">
                      {pendingForActive.map((p) => (
                        <div
                          key={p.localSeq}
                          className="text-sm p-3 bg-card border border-border rounded-md shadow-sm"
                        >
                          {JSON.stringify(p.payload)}
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {active?.available_commands &&
                  active.available_commands.length > 0 && (
                    <div className="flex items-center gap-2 px-4 py-2.5 border-t border-border/40 bg-muted/5 overflow-x-auto no-scrollbar scroll-smooth">
                      <span className="text-[9px] font-bold uppercase tracking-[0.15em] text-muted-foreground/60 whitespace-nowrap mr-2">
                        Commands
                      </span>
                      {active.available_commands.map((cmd) => (
                        <button
                          key={cmd.name}
                          type="button"
                          onClick={() => executeCommand(cmd.name)}
                          className="inline-flex items-center gap-1.5 rounded-full border border-border/50 bg-background px-3 py-1 text-[11px] font-medium transition-colors hover:border-primary/50 hover:bg-primary/5 hover:text-primary text-muted-foreground whitespace-nowrap shadow-sm active:scale-95"
                          title={cmd.description}
                        >
                          <Plus className="size-3 opacity-50" />
                          {cmd.name}
                        </button>
                      ))}
                    </div>
                  )}

                <form
                  className="border-t border-border p-2 md:p-4"
                  onSubmit={(e) => {
                    e.preventDefault();
                    sendPrompt();
                  }}
                >
                  <div className="flex items-end gap-2 rounded-lg border border-input bg-background p-2 focus-within:border-primary/50 transition-colors">
                    <textarea
                      id="agent-message-input"
                      name="message"
                      value={draft}
                      onChange={(e) =>
                        setDraft(activeSessionId, e.target.value)
                      }
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && !e.shiftKey) {
                          e.preventDefault();
                          sendPrompt();
                        }
                      }}
                      placeholder={`Message ${active?.name || activeSessionId.slice(0, 8)}…`}
                      rows={1}
                      className="flex-1 resize-none bg-transparent px-2 py-1.5 text-sm outline-none"
                    />
                    <div className="flex items-center gap-1.5 mb-0.5">
                      <WhisperTranscribe
                        onTranscription={(transcription) => {
                          setDraft(
                            activeSessionId,
                            draft ? `${draft} ${transcription}` : transcription,
                          );
                        }}
                      />
                      <button
                        type="submit"
                        disabled={!draft.trim() || conn.status !== "open"}
                        className={cn(
                          "flex size-9 items-center justify-center rounded-md transition-colors",
                          draft.trim() && conn.status === "open"
                            ? "bg-primary text-primary-foreground hover:bg-primary/90"
                            : "bg-muted text-muted-foreground",
                        )}
                        aria-label="Send"
                      >
                        {conn.status !== "open" ? (
                          <Loader2 className="size-4 animate-spin" />
                        ) : (
                          <Send className="size-4" />
                        )}
                      </button>
                    </div>
                  </div>
                </form>
              </>
            )}
          </>
        )}
      </section>

      {drafts["spawn"] === "open" && (
        <SpawnDialog
          projects={projects}
          onSpawn={(path, cmd) => {
            send({
              type: "spawn",
              project_path: path,
              agent_command: cmd || undefined,
            });
            setDraft("spawn", "");
          }}
          onCancel={() => setDraft("spawn", "")}
        />
      )}
    </div>
  );
}
