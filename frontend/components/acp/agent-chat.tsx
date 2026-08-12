"use client";

import { useEffect, useMemo, useRef, useState, useCallback } from "react";
import { cn } from "@/lib/utils";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { SessionSidebar } from "./session-sidebar";
import { SpawnDialog } from "./spawn-dialog";
import { useAcpSocket } from "./use-acp-socket";
import { buildBlocks } from "./utils";
import { EMPTY_ARRAY } from "./types";
import { AgentChatHeader } from "./agent-chat-header";
import { ChatMessages } from "./chat-messages";
import { AgentChatPrompt } from "./agent-chat-prompt";
import { AgentTerminal } from "./agent-terminal";
import { PendingPermissions } from "./pending-permissions";
import { FilePreview } from "./file-preview";
import { QuickSearch } from "./quick-search";
import { FileBrowser } from "./file-browser";

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
    fileBrowser,
    fileBrowserHost,
    listDirectories,
    findFiles,
    readFile,
    send,
    sendTerminalInput,
    drafts,
    setDraft,
    sidebarOpen,
    setSidebarOpen,
    filePreview,
    setFilePreview,
    readFile,
    listDirectories,
    findFiles,
    suggestions,
  } = useAcpSocket();

  const [terminalFullScreen, setTerminalFullScreen] = useState<
    Record<string, boolean>
  >({});
  const [activeStandaloneTerminalId, setActiveStandaloneTerminalId] = useState<string | null>(null);
  const [fileBrowserOpen, setFileBrowserOpen] = useState(false);

  const [isFileSearchOpen, setIsFileSearchOpen] = useState(false);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "p") {
        e.preventDefault();
        setIsFileSearchOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const active = activeSessionId ? sessions[activeSessionId] : null;
  const draft = activeSessionId ? (drafts[activeSessionId] ?? "") : "";
  const standaloneTerminal = activeStandaloneTerminalId
    ? terminals[activeStandaloneTerminalId]
    : null;
  const standaloneRemote = instances.find((instance) => instance.name === activeInstance)
    ?? (instances.length === 1 ? instances[0] : undefined);
  const standaloneTitle = standaloneRemote?.name || activeInstance || "Standalone terminal";
  const standaloneSubtitle = [standaloneRemote?.host, standaloneTerminal?.cwd]
    .filter(Boolean)
    .join(" · ");
  const fileBrowserHostLabel = standaloneRemote?.name || activeInstance || fileBrowserHost;
  const fileBrowserDirectory = standaloneTerminal?.cwd || active?.project_path || "/";

  const availableCommands = useMemo(() => {
    return active?.available_commands || [];
  }, [active]);

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

  const sendPrompt = useCallback(() => {
    if (
      !activeSessionId ||
      !draft.trim() ||
      conn.status !== "open"
    )
      return;

    const ok = send({
      type: "send_prompt",
      session_id: activeSessionId,
      text: draft.trim(),
    });
    if (ok) {
      setDraft(activeSessionId, "");
    }
  }, [activeSessionId, draft, conn.status, send, setDraft]);

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
      sendTerminalInput(tid, data);
    },
    [sendTerminalInput],
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
  const [spawnDialogTerminalOnly, setSpawnDialogTerminalOnly] = useState(false);

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
				activeStandaloneTerminalId={activeStandaloneTerminalId}
				onSelectSession={(sid) => {
					const terminalIds = sessionTerminals[sid] ?? []
					const selectedTerminalId = activeTerminalId[sid] && terminalIds.includes(activeTerminalId[sid])
						? activeTerminalId[sid]
						: terminalIds[0] ?? null
					setActiveSessionId(sid)
					setActiveTerminalId({ ...activeTerminalId, [sid]: selectedTerminalId })
					setViewMode({ ...viewMode, [sid]: selectedTerminalId ? "terminal" : "chat" })
					setTerminalFullScreen({ ...terminalFullScreen, [sid]: false })
					setActiveStandaloneTerminalId(null)
				}}
				onSpawn={() => {
					setSpawnDialogTerminalOnly(false)
					setIsSpawnDialogOpen(true)
				}}
				onNewTerminal={() => {
					setSpawnDialogTerminalOnly(true)
					setIsSpawnDialogOpen(true)
				}}
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
            <AgentChatHeader
              sidebarOpen={sidebarOpen}
              setSidebarOpen={setSidebarOpen}
              title="Terminal workspace"
              subtitle="Select a terminal or start a new one"
              viewMode="terminal"
              setViewMode={() => {}}
              onOpenFiles={() => setFileBrowserOpen(true)}
            />
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
              Select a terminal to get started
            </div>
          </div>
        ) : activeStandaloneTerminalId ? (
          <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
            <AgentChatHeader
              sidebarOpen={sidebarOpen}
              setSidebarOpen={setSidebarOpen}
              title={standaloneTitle}
              subtitle={standaloneSubtitle || `Terminal ${activeStandaloneTerminalId.slice(0, 8)}`}
              viewMode="terminal"
              setViewMode={() => {}}
              onOpenFiles={() => setFileBrowserOpen(true)}
              onTerminate={() => closeTerminal(activeStandaloneTerminalId)}
              isStandalone
            />
            <div className="flex-1 bg-zinc-950 overflow-hidden relative">
              <AgentTerminal
                terminalId={activeStandaloneTerminalId}
                isFullScreen={false}
                onToggleFullScreen={() => {}}
                onClose={() => closeTerminal(activeStandaloneTerminalId)}
                onInput={(data) => onTerminalInput(activeStandaloneTerminalId, data)}
                onResize={(cols, rows) => onTerminalResize(activeStandaloneTerminalId, cols, rows)}
                onAttach={(cols, rows) => onTerminalAttach(activeStandaloneTerminalId, cols, rows)}
              />
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
            <AgentChatHeader
              sidebarOpen={sidebarOpen}
              setSidebarOpen={setSidebarOpen}
              title={active?.name || active?.folder || active?.project_path || activeSessionId!}
              subtitle={`${active?.agent_command} · ${active?.folder || active?.project_path}`}
              onBindTelegram={bindTelegramThread}
              onOpenFiles={() => setFileBrowserOpen(true)}
              isTelegramBound={active?.thread_id != null && active.thread_id > 0}
              viewMode={(viewMode[activeSessionId!] as "chat" | "terminal") ?? "chat"}
              setViewMode={(mode) => setViewMode({ ...viewMode, [activeSessionId!]: mode })}
              onSearch={() => setIsFileSearchOpen(true)}
              onCreateTerminal={() => activeSessionId && createTerminal(activeSessionId)}
              onTerminate={terminateSession}
            />

            {/* CONTENT AREA */}
            <ResizablePanelGroup
              direction="horizontal"
              className="flex-1 min-h-0"
            >
              <ResizablePanel defaultSize={filePreview ? 60 : 100} minSize={30}>
                <ResizablePanelGroup
                  direction="vertical"
                  className="h-full"
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
                    <ChatMessages
                      blocks={blocks}
                      agentCommand={active?.agent_command}
                      onReadFile={(path) => readFile(activeSessionId, path)}
                    />
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
                        <AgentTerminal
                          terminalId={activeTerminalId[activeSessionId]!}
                          isFullScreen={terminalFullScreen[activeSessionId]}
                          onToggleFullScreen={() => setTerminalFullScreen({
                            ...terminalFullScreen,
                            [activeSessionId]: !terminalFullScreen[activeSessionId],
                          })}
                          onClose={() => closeTerminal(activeTerminalId[activeSessionId]!)}
                          onInput={(data) => onTerminalInput(activeTerminalId[activeSessionId]!, data)}
                          onResize={(cols, rows) => onTerminalResize(activeTerminalId[activeSessionId]!, cols, rows)}
                          onAttach={(cols, rows) => onTerminalAttach(activeTerminalId[activeSessionId]!, cols, rows)}
                          sessionTerminals={sessionTerminals[activeSessionId]}
                          activeTerminalId={activeTerminalId[activeSessionId]}
                          onSelectTerminal={(tid) => setActiveTerminalId({ ...activeTerminalId, [activeSessionId]: tid })}
                          onShowChat={() => setViewMode({ ...viewMode, [activeSessionId]: "chat" })}
                        />
                      </ResizablePanel>
                    )}
                </ResizablePanelGroup>
              </ResizablePanel>

              {filePreview && (
                <>
                  <ResizableHandle withHandle />
                  <ResizablePanel defaultSize={40} minSize={20}>
                    <FilePreview
                      path={filePreview.path}
                      content={filePreview.content}
                      startLine={filePreview.startLine}
                      totalLines={filePreview.totalLines}
                      onClose={() => setFilePreview(null)}
                    />
                  </ResizablePanel>
                </>
              )}
            </ResizablePanelGroup>

            <PendingPermissions pendingPermissions={pendingForActive} />

            <AgentChatPrompt
              draft={draft}
              setDraft={(text) => activeSessionId && setDraft(activeSessionId, text)}
              onSend={sendPrompt}
              onCancel={cancelActive}
              isWorking={active?.status === "Working"}
              connStatus={conn.status}
              availableCommands={availableCommands}
              placeholder={`Message ${active?.name || (activeSessionId ? activeSessionId.slice(0, 8) : "")}…`}
              listDirectories={(q) => listDirectories(activeSessionId, q)}
              findFiles={(q) => findFiles(activeSessionId, q)}
              suggestions={suggestions}
            />
          </div>
        )}
      </section>

      {isFileSearchOpen && (
        <QuickSearch
          onSearch={(q) => {
            if (q.endsWith("/")) {
              listDirectories(activeSessionId, q);
            } else {
              findFiles(activeSessionId, q);
            }
          }}
          suggestions={suggestions}
          onSelect={(path, type) => {
            if (type === "file") {
              readFile(activeSessionId, path);
            } else {
              send({
                type: "create_terminal",
                cwd: path,
                cols: 120,
                rows: 24,
              });
            }
            setIsFileSearchOpen(false);
          }}
          onClose={() => setIsFileSearchOpen(false)}
        />
      )}

      {fileBrowserOpen && (
        <FileBrowser
          hostKey={fileBrowserHost}
          hostLabel={fileBrowserHostLabel}
          initialDirectory={fileBrowserDirectory}
          state={fileBrowser}
          onListDirectories={listDirectories}
          onFindFiles={findFiles}
          onReadFile={readFile}
          onClose={() => setFileBrowserOpen(false)}
        />
      )}

      {isSpawnDialogOpen && (
        <SpawnDialog
          projects={projects}
          defaultTerminalOnly={spawnDialogTerminalOnly}
          onSearchDirectories={(q) => listDirectories(null, q)}
          suggestions={suggestions}
          onSpawn={(path, cmd) => {
            if (cmd) {
              send({
                type: "spawn",
                project_path: path,
                agent_command: cmd,
              });
            } else {
              send({
                type: "create_terminal",
                cwd: path,
                cols: 120,
                rows: 24,
              });
            }
            setIsSpawnDialogOpen(false);
          }}
          onCancel={() => setIsSpawnDialogOpen(false)}
        />
      )}
    </div>
  );
}
