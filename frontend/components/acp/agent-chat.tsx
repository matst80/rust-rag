"use client";

import { useMemo, useRef, useLayoutEffect, useState, useCallback } from "react";
import {
  Bot,
  Circle,
  CheckCircle2,
  Hash,
  Link2,
  Loader2,
  Maximize2,
  Menu,
  MessageSquare,
  Minimize2,
  Plus,
  Send,
  StopCircle,
  Terminal as TerminalIcon,
  Trash2,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { WhisperTranscribe } from "@/components/entries/whisper-transcribe";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { TerminalView } from "./terminal-view";
import { BlockView } from "./block-view";
import { SessionSidebar } from "./session-sidebar";
import { SpawnDialog } from "./spawn-dialog";
import { useAcpSocket } from "./use-acp-socket";
import { buildBlocks } from "./utils";
import { EMPTY_ARRAY } from "./types";

const NEAR_BOTTOM_PX = 80;

export function AgentChat() {
  const {
    conn,
    sessions,
    eventsBySession,
    activeSessionId,
    setActiveSessionId,
    terminals,
    sessionTerminals,
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

  const [terminalFullScreen, setTerminalFullScreen] = useState<
    Record<string, boolean>
  >({});
  const [selectedCommand, setSelectedCommand] = useState<
    Record<string, string | null>
  >({});
  const [activeStandaloneTerminalId, setActiveStandaloneTerminalId] = useState<string | null>(null);

  const active = activeSessionId ? sessions[activeSessionId] : null;
  const draft = activeSessionId ? (drafts[activeSessionId] ?? "") : "";
  const activeSelectedCommand = activeSessionId
    ? (selectedCommand[activeSessionId] ?? null)
    : null;

  const activeEvents = useMemo(() => {
    if (!activeSessionId) return EMPTY_ARRAY;
    return eventsBySession[activeSessionId] ?? EMPTY_ARRAY;
  }, [activeSessionId, eventsBySession]);

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

  const sendPrompt = useCallback(() => {
    if (
      !activeSessionId ||
      (!draft.trim() && !selectedCommand[activeSessionId]) ||
      conn.status !== "open"
    )
      return;

    let text = draft.trim();
    const cmd = selectedCommand[activeSessionId];
    if (cmd) {
      text = `/${cmd}${text ? ` ${text}` : ""}`;
    }

    const ok = send({
      type: "send_prompt",
      session_id: activeSessionId,
      text,
    });
    if (ok) {
      setDraft(activeSessionId, "");
      setSelectedCommand({ ...selectedCommand, [activeSessionId]: null });
    }
  }, [activeSessionId, draft, selectedCommand, conn.status, send, setDraft]);

  const toggleCommand = useCallback(
    (name: string) => {
      if (!activeSessionId) return;
      const current = selectedCommand[activeSessionId];
      setSelectedCommand({
        ...selectedCommand,
        [activeSessionId]: current === name ? null : name,
      });
    },
    [activeSessionId, selectedCommand],
  );

  const terminateSession = useCallback(() => {
    if (!activeSessionId) return;
    if (!window.confirm("Are you sure you want to terminate this session?"))
      return;
    send({
      type: "end_session",
      session_id: activeSessionId,
    });
  }, [activeSessionId, send]);

  const cancelActive = useCallback(() => {
    if (!activeSessionId) return;
    send({
      type: "cancel",
      session_id: activeSessionId,
    });
  }, [activeSessionId, send]);

  const bindTelegramThread = useCallback(() => {
    if (!activeSessionId) return;
    const raw = window.prompt(
      "Telegram thread_id (leave blank to auto-create a new forum topic):",
      "",
    );
    if (raw === null) return;
    const trimmed = raw.trim();
    const payload: Record<string, unknown> = {
      type: "bind_telegram_thread",
      session_id: activeSessionId,
    };
    if (trimmed === "") {
      payload.thread_id = null;
    } else {
      const n = Number(trimmed);
      if (!Number.isInteger(n) || n <= 0) {
        window.alert("thread_id must be a positive integer or blank");
        return;
      }
      payload.thread_id = n;
    }
    send(payload);
  }, [activeSessionId, send]);

  const createTerminal = useCallback(
    (sid: string) => {
      const s = sessions[sid];
      if (!s) return;
      send({
        type: "create_terminal",
        session_id: sid,
        cwd: s.project_path || "/",
        cols: 120,
        rows: 24,
      });
    },
    [sessions, send],
  );

  const closeTerminal = useCallback(
    (tid: string) => {
      send({
        type: "close_terminal",
        terminal_id: tid,
      });
    },
    [send],
  );

  const onTerminalInput = useCallback(
    (tid: string, data: string) => {
      send({
        type: "terminal_input",
        terminal_id: tid,
        data,
      });
    },
    [send],
  );

  const onTerminalResize = useCallback(
    (tid: string, cols: number, rows: number) => {
      send({
        type: "terminal_resize",
        terminal_id: tid,
        cols,
        rows,
      });
    },
    [send],
  );

  const onTerminalAttach = useCallback(
    (tid: string, cols: number, rows: number) => {
      send({
        type: "attach_terminal",
        terminal_id: tid,
        cols,
        rows,
      });
    },
    [send],
  );

  const [isSpawnDialogOpen, setIsSpawnDialogOpen] = useState(false);

  return (
    <div className="relative flex h-full overflow-hidden">
      {/* Mobile Overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 z-40 bg-background/80 backdrop-blur-sm md:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

			<SessionSidebar
				sessions={sessions}
				activeSessionId={activeSessionId}
				onSelectSession={(sid) => {
					setActiveSessionId(sid)
					setViewMode({ ...viewMode, [sid]: "chat" })
					setTerminalFullScreen({ ...terminalFullScreen, [sid]: false })
					setActiveStandaloneTerminalId(null)
				}}
				onSpawn={() => setIsSpawnDialogOpen(true)}
				onClose={() => setSidebarOpen(false)}
				onCreateTerminal={(sid) => {
					createTerminal(sid)
					setTerminalFullScreen({ ...terminalFullScreen, [sid]: true })
				}}
				onSelectTerminal={(sid, tid) => {
					if (sid) {
						setActiveSessionId(sid)
						setActiveTerminalId({ ...activeTerminalId, [sid]: tid })
						setViewMode({ ...viewMode, [sid]: "terminal" })
						setTerminalFullScreen({ ...terminalFullScreen, [sid]: true })
						setActiveStandaloneTerminalId(null)
					} else {
						setActiveSessionId(null)
						setActiveStandaloneTerminalId(tid)
					}
				}}
				terminals={terminals}
				sessionTerminals={sessionTerminals}
				activeTerminalId={activeTerminalId}
				conn={conn}
				workers={workers}
				instances={instances}
				activeInstance={activeInstance}
				onSelectInstance={selectInstance}
				sidebarOpen={sidebarOpen}
			/>
      {/* Main Content Area */}
      <section className="flex min-w-0 flex-1 flex-col h-full overflow-hidden">
        {!activeSessionId && !activeStandaloneTerminalId ? (
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
        ) : activeStandaloneTerminalId ? (
          <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
            {/* HEADER FOR STANDALONE TERMINAL */}
            <header className="flex items-center gap-2 border-b border-border px-3 py-2 md:px-6 md:py-3 shrink-0">
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
              <TerminalIcon className="size-4 shrink-0 text-muted-foreground" />
              <div className="flex flex-1 flex-col min-w-0">
                <div className="flex items-center gap-1.5 min-w-0">
                  <span className="truncate text-sm font-medium">
                    Standalone Terminal {activeStandaloneTerminalId.slice(0, 8)}
                  </span>
                </div>
                <span className="truncate font-mono text-[10px] text-muted-foreground/60">
                  {terminals[activeStandaloneTerminalId]?.cwd}
                </span>
              </div>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => closeTerminal(activeStandaloneTerminalId)}
                  className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-red-500/10 hover:text-red-500"
                  title="Close Terminal"
                >
                  <X className="size-4" />
                </button>
              </div>
            </header>
            <div className="flex-1 bg-zinc-950 overflow-hidden relative">
              <TerminalView
                key={activeStandaloneTerminalId}
                terminalId={activeStandaloneTerminalId}
                onInput={(data) =>
                  onTerminalInput(activeStandaloneTerminalId, data)
                }
                onResize={(cols, rows) =>
                  onTerminalResize(activeStandaloneTerminalId, cols, rows)
                }
                onAttach={(cols, rows) =>
                  onTerminalAttach(activeStandaloneTerminalId, cols, rows)
                }
              />
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
            {/* HEADER */}
            <header className="flex items-center gap-2 border-b border-border px-3 py-2 md:px-6 md:py-3 shrink-0">
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
                  onClick={bindTelegramThread}
                  className={cn(
                    "flex size-8 items-center justify-center rounded-md hover:bg-muted/40 hover:text-foreground",
                    active?.thread_id != null && active.thread_id > 0
                      ? "text-emerald-500"
                      : "text-muted-foreground",
                  )}
                  title={
                    active?.thread_id != null && active.thread_id > 0
                      ? `Bound to Telegram thread ${active.thread_id} (click to rebind)`
                      : "Bind to Telegram thread"
                  }
                  aria-label="Bind Telegram thread"
                >
                  <Link2 className="size-4" />
                </button>

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
                  title="Toggle Terminal"
                >
                  <TerminalIcon className="size-4" />
                </button>

                <button
                  type="button"
                  onClick={() => createTerminal(activeSessionId)}
                  className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
                  title="New Terminal"
                >
                  <Plus className="size-4" />
                </button>

                <button
                  type="button"
                  onClick={terminateSession}
                  className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                  title="Terminate Session"
                >
                  <Trash2 className="size-4" />
                </button>
              </div>
            </header>

            {/* CONTENT AREA */}
            <ResizablePanelGroup
              direction="vertical"
              className="flex-1 min-h-0"
            >
              {/* CHAT THREAD (Top section) */}
              <ResizablePanel
                defaultSize={60}
                minSize={20}
                className={cn(
                  "flex flex-col",
                  activeSessionId &&
                    terminalFullScreen[activeSessionId] &&
                    "hidden",
                )}
              >
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
              </ResizablePanel>

              {/* TERMINAL HANDLE */}
              {activeSessionId &&
                viewMode[activeSessionId] === "terminal" &&
                activeTerminalId[activeSessionId] &&
                !terminalFullScreen[activeSessionId] && (
                  <ResizableHandle withHandle />
                )}

              {/* TERMINAL SECTION (Resizable Middle section) */}
              {activeSessionId &&
                viewMode[activeSessionId] === "terminal" &&
                activeTerminalId[activeSessionId] && (
                  <ResizablePanel
                    defaultSize={terminalFullScreen[activeSessionId] ? 100 : 40}
                    minSize={10}
                    className="flex flex-col"
                  >
                    <div className="flex flex-col h-full bg-muted/5">
                      {/* TERMINAL TABS */}
                      {sessionTerminals[activeSessionId] &&
                        sessionTerminals[activeSessionId].length > 0 && (
                          <div className="flex items-center gap-1 border-b border-border bg-muted/20 px-4 py-1.5 overflow-x-auto no-scrollbar shrink-0">
                            <button
                              onClick={() =>
                                setViewMode({
                                  ...viewMode,
                                  [activeSessionId]: "chat",
                                })
                              }
                              className={cn(
                                "flex items-center gap-1.5 rounded px-2 py-1 text-[10px] font-bold uppercase tracking-wider transition-colors",
                                (viewMode[activeSessionId] ?? "chat") === "chat"
                                  ? "bg-background text-primary shadow-sm"
                                  : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                              )}
                            >
                              <MessageSquare className="size-3" />
                              Chat Only
                            </button>
                            {sessionTerminals[activeSessionId].map(
                              (tid, idx) => (
                                <div
                                  key={tid}
                                  className="flex items-center gap-0.5"
                                >
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
                                      viewMode[activeSessionId] ===
                                        "terminal" &&
                                        activeTerminalId[activeSessionId] ===
                                          tid
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
                              ),
                            )}

                            <div className="flex-1" />

                            <button
                              onClick={() =>
                                setTerminalFullScreen({
                                  ...terminalFullScreen,
                                  [activeSessionId]:
                                    !terminalFullScreen[activeSessionId],
                                })
                              }
                              className="flex items-center gap-1.5 rounded px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground hover:bg-muted/40 hover:text-foreground transition-colors"
                              title={
                                terminalFullScreen[activeSessionId]
                                  ? "Exit Full Screen"
                                  : "Full Screen"
                              }
                            >
                              {terminalFullScreen[activeSessionId] ? (
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
                          key={activeTerminalId[activeSessionId]!}
                          terminalId={activeTerminalId[activeSessionId]!}
                          onInput={(data) =>
                            onTerminalInput(
                              activeTerminalId[activeSessionId]!,
                              data,
                            )
                          }
                          onResize={(cols, rows) =>
                            onTerminalResize(
                              activeTerminalId[activeSessionId]!,
                              cols,
                              rows,
                            )
                          }
                          onAttach={(cols, rows) =>
                            onTerminalAttach(
                              activeTerminalId[activeSessionId]!,
                              cols,
                              rows,
                            )
                          }
                        />
                      </div>
                    </div>
                  </ResizablePanel>
                )}
            </ResizablePanelGroup>

            {/* PENDING PERMISSIONS */}
            {pendingForActive.length > 0 && (
              <div className="border-t border-border bg-amber-500/5 p-4 shrink-0">
                <div className="text-xs font-bold text-amber-600 uppercase tracking-wider mb-2 flex items-center gap-2">
                  <Circle className="size-2 fill-amber-500 animate-pulse" />
                  Pending Permissions
                </div>
                <div className="space-y-2 max-h-40 overflow-y-auto">
                  {pendingForActive.map((p) => (
                    <div
                      key={p.localSeq}
                      className="text-sm p-3 bg-card border border-border rounded-md shadow-sm font-mono text-[10px]"
                    >
                      {JSON.stringify(p.payload, null, 2)}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* BOTTOM COMMANDS & PROMPT */}
            <div className="shrink-0 flex flex-col border-t border-border bg-background">
              {active?.available_commands &&
                active.available_commands.length > 0 && (
                  <div className="flex items-center gap-1 px-4 py-2 bg-muted/5 overflow-x-auto no-scrollbar scroll-smooth">
                    <span className="text-[9px] font-bold uppercase tracking-[0.15em] text-muted-foreground/60 whitespace-nowrap mr-2">
                      Commands
                    </span>
                    {active.available_commands.map((cmd) => (
                      <button
                        key={cmd.name}
                        type="button"
                        onClick={() => toggleCommand(cmd.name)}
                        className={cn(
                          "inline-flex items-center gap-1 rounded-full border px-2 py-1 text-[11px] font-medium transition-all shadow-sm active:scale-95 whitespace-nowrap",
                          activeSelectedCommand === cmd.name
                            ? "border-primary bg-primary text-primary-foreground shadow-md ring-1 ring-primary/20"
                            : "border-border/50 bg-background text-muted-foreground hover:border-primary/50 hover:bg-primary/5 hover:text-primary",
                        )}
                        title={cmd.description}
                      >
                        {activeSelectedCommand === cmd.name ? (
                          <CheckCircle2 className="size-3" />
                        ) : (
                          <Plus className="size-3 opacity-50" />
                        )}
                        {cmd.name}
                      </button>
                    ))}
                  </div>
                )}

              {/* PROMPT */}
              <form
                className="border-t border-border"
                onSubmit={(e) => {
                  e.preventDefault();
                  sendPrompt();
                }}
              >
                <div className="flex items-center gap-3 border border-input bg-background p-1 focus-within:border-primary/50 transition-colors shadow-sm">
                  <textarea
                    id="agent-message-input"
                    name="message"
                    value={draft}
                    onChange={(e) => setDraft(activeSessionId, e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && !e.shiftKey) {
                        e.preventDefault();
                        sendPrompt();
                      }
                    }}
                    placeholder={
                      activeSelectedCommand
                        ? `Command: /${activeSelectedCommand}...`
                        : `Message ${active?.name || activeSessionId.slice(0, 8)}…`
                    }
                    rows={1}
                    className="flex-1 resize-none px-2 bg-transparent text-sm outline-none"
                  />
                  <div className="flex items-center gap-2 mb-1 shrink-0">
                    <div className="flex items-center justify-center">
                      <WhisperTranscribe
                        onTranscription={(transcription) => {
                          setDraft(
                            activeSessionId,
                            draft ? `${draft} ${transcription}` : transcription,
                          );
                        }}
                      />
                    </div>
                    <button
                      type="submit"
                      disabled={
                        (!draft.trim() && !activeSelectedCommand) ||
                        conn.status !== "open"
                      }
                      className={cn(
                        "flex size-10 items-center justify-center rounded-md transition-all shadow-sm",
                        (draft.trim() || activeSelectedCommand) &&
                          conn.status === "open"
                          ? "bg-primary text-primary-foreground hover:bg-primary/90 hover:shadow-md"
                          : "bg-muted text-muted-foreground opacity-50 cursor-not-allowed",
                      )}
                      aria-label="Send"
                    >
                      {conn.status !== "open" ? (
                        <Loader2 className="size-5 animate-spin" />
                      ) : (
                        <Send className="size-5" />
                      )}
                    </button>

                    {active?.status === "Working" && (
                      <button
                        type="button"
                        onClick={cancelActive}
                        className="flex size-10 items-center justify-center rounded-md bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors shadow-sm"
                        title="Stop/Cancel Agent"
                      >
                        <StopCircle className="size-5" />
                      </button>
                    )}
                  </div>
                </div>
              </form>
            </div>
          </div>
        )}
      </section>

      {isSpawnDialogOpen && (
        <SpawnDialog
          projects={projects}
          onSpawn={(path, cmd) => {
            send({
              type: "spawn",
              project_path: path,
              agent_command: cmd || undefined,
            });
            setIsSpawnDialogOpen(false);
          }}
          onCancel={() => setIsSpawnDialogOpen(false)}
        />
      )}
    </div>
  );
}
