"use client";

import React, { useMemo, useEffect, useRef, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView, keymap } from "@codemirror/view";
import { useTheme } from "next-themes";
import * as Y from "yjs";
import { yCollab } from "y-codemirror.next";
import {
  Bold,
  Italic,
  Heading1,
  Heading2,
  List,
  ListOrdered,
  Code,
  Quote,
  CheckSquare,
  Link as LinkIcon,
  Users,
  Wifi,
  WifiOff,
  MessageSquare,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { InlineCommentPopover } from "./inline-comment-popover";

// Preset colors for peer cursors
const USER_COLORS = [
  "#3b82f6", // blue
  "#10b981", // emerald
  "#f59e0b", // amber
  "#ec4899", // pink
  "#8b5cf6", // violet
  "#06b6d4", // cyan
];

interface EntryCodeMirrorEditorProps {
  id?: string;
  value: string;
  onChange: (value: string) => void;
  onSave?: () => void;
  enableCollab?: boolean;
  onAddComment?: (comment: string, quote: string) => Promise<void>;
  className?: string;
}

export function EntryCodeMirrorEditor({
  id,
  value,
  onChange,
  onSave,
  enableCollab = true,
  onAddComment,
  className,
}: EntryCodeMirrorEditorProps) {
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";
  const editorRef = useRef<EditorView | null>(null);

  const [collabConnected, setCollabConnected] = useState(false);
  const [peerCount, setPeerCount] = useState(1);

  // Inline comment popover state
  const [inlinePopover, setInlinePopover] = useState<{
    text: string;
    pos: { top: number; left: number };
  } | null>(null);

  // Stats calculation
  const stats = useMemo(() => {
    const text = value || "";
    const words = text.trim() ? text.trim().split(/\s+/).length : 0;
    const chars = text.length;
    const lines = text.split("\n").length;
    return { words, chars, lines };
  }, [value]);

  // Yjs document and WebSocket provider setup
  const yjsSetup = useMemo(() => {
    if (!enableCollab || !id) return null;
    const doc = new Y.Doc();
    const ytext = doc.getText("content");
    // Initial content seed
    if (value && ytext.length === 0) {
      ytext.insert(0, value);
    }

    // Set local awareness user
    const color = USER_COLORS[Math.floor(Math.random() * USER_COLORS.length)];
    const username = `User-${Math.random().toString(36).substring(2, 6)}`;

    return { doc, ytext, username, color };
  }, [enableCollab, id]);

  // Connect WebSocket relay
  useEffect(() => {
    if (!yjsSetup || !id || typeof window === "undefined") return;

    let socket: WebSocket | null = null;
    let isCleanedUp = false;

    try {
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${protocol}//${window.location.host}/api/collab/ws?room=${encodeURIComponent(id)}`;
      socket = new WebSocket(wsUrl);
      socket.binaryType = "arraybuffer";

      socket.onopen = () => {
        if (isCleanedUp) return;
        setCollabConnected(true);

        // Send local Yjs sync step 1
        const stateVector = Y.encodeStateVector(yjsSetup.doc);
        socket?.send(stateVector);
      };

      socket.onmessage = (event) => {
        if (isCleanedUp) return;
        if (event.data instanceof ArrayBuffer) {
          const update = new Uint8Array(event.data);
          Y.applyUpdate(yjsSetup.doc, update, "remote");
        }
      };

      socket.onclose = () => {
        if (!isCleanedUp) setCollabConnected(false);
      };

      socket.onerror = () => {
        if (!isCleanedUp) setCollabConnected(false);
      };

      const docUpdateHandler = (update: Uint8Array, origin: any) => {
        if (origin !== "remote" && socket && socket.readyState === WebSocket.OPEN) {
          socket.send(update);
        }
      };

      yjsSetup.doc.on("update", docUpdateHandler);

      return () => {
        isCleanedUp = true;
        yjsSetup.doc.off("update", docUpdateHandler);
        if (socket) {
          socket.close();
        }
        setCollabConnected(false);
      };
    } catch {
      setCollabConnected(false);
    }
  }, [yjsSetup, id]);

  // Sync Y.Text back to form state
  useEffect(() => {
    if (!yjsSetup) return;
    const observer = () => {
      onChange(yjsSetup.ytext.toString());
    };
    yjsSetup.ytext.observe(observer);
    return () => {
      yjsSetup.ytext.unobserve(observer);
      yjsSetup.doc.destroy();
    };
  }, [yjsSetup, onChange]);

  // Extensions
  const extensions = useMemo(() => {
    const extList = [
      markdown(),
      EditorView.lineWrapping,
      keymap.of([
        {
          key: "Mod-s",
          run: () => {
            if (onSave) {
              onSave();
              return true;
            }
            return false;
          },
        },
      ]),
    ];

    if (yjsSetup) {
      extList.push(yCollab(yjsSetup.ytext, null));
    }

    return extList;
  }, [yjsSetup, onSave]);

  // Toolbar action helpers
  const applyFormat = (prefix: string, suffix: string = prefix, defaultPlaceholder: string = "") => {
    const view = editorRef.current;
    if (!view) return;

    const { state } = view;
    const { from, to } = state.selection.main;
    const selected = state.sliceDoc(from, to) || defaultPlaceholder;
    const insert = `${prefix}${selected}${suffix}`;

    view.dispatch({
      changes: { from, to, insert },
      selection: {
        anchor: from + prefix.length,
        head: from + prefix.length + selected.length,
      },
    });
    view.focus();
  };

  const applyLinePrefix = (prefix: string) => {
    const view = editorRef.current;
    if (!view) return;

    const { state } = view;
    const { from } = state.selection.main;
    const line = state.doc.lineAt(from);

    view.dispatch({
      changes: { from: line.from, to: line.from, insert: prefix },
      selection: { anchor: from + prefix.length },
    });
    view.focus();
  };

  const handleOpenCommentOnSelection = () => {
    const view = editorRef.current;
    if (!view) return;
    const { state } = view;
    const { from, to } = state.selection.main;
    const selected = state.sliceDoc(from, to).trim();
    if (!selected) return;

    const coords = view.coordsAtPos(to) || view.coordsAtPos(from);
    if (coords) {
      setInlinePopover({
        text: selected,
        pos: { top: coords.bottom + 8, left: coords.left },
      });
    }
  };

  return (
    <div
      className={cn(
        "rounded-xl border border-border bg-card overflow-hidden shadow-sm flex flex-col transition-all",
        className
      )}
    >
      {/* Editor Action Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-1.5 border-b border-border px-3 py-1.5 bg-muted/20 text-muted-foreground">
        <div className="flex items-center gap-1 flex-wrap">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyFormat("**", "**", "bold text")}
            title="Bold (**text**)"
          >
            <Bold className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyFormat("*", "*", "italic text")}
            title="Italic (*text*)"
          >
            <Italic className="size-3.5" />
          </Button>
          <div className="w-px h-4 bg-border mx-1" />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyLinePrefix("# ")}
            title="Heading 1 (# )"
          >
            <Heading1 className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyLinePrefix("## ")}
            title="Heading 2 (## )"
          >
            <Heading2 className="size-3.5" />
          </Button>
          <div className="w-px h-4 bg-border mx-1" />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyFormat("`", "`", "code")}
            title="Inline Code (`code`)"
          >
            <Code className="size-3.5" />
          </Button>
          <div className="w-px h-4 bg-border mx-1" />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyLinePrefix("- ")}
            title="Bullet list (- )"
          >
            <List className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyLinePrefix("1. ")}
            title="Numbered list (1. )"
          >
            <ListOrdered className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyLinePrefix("- [ ] ")}
            title="Task item (- [ ] )"
          >
            <CheckSquare className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyLinePrefix("> ")}
            title="Blockquote (> )"
          >
            <Quote className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7 text-xs hover:text-foreground"
            onClick={() => applyFormat("[", "](url)", "link text")}
            title="Hyperlink [title](url)"
          >
            <LinkIcon className="size-3.5" />
          </Button>

          {onAddComment && (
            <>
              <div className="w-px h-4 bg-border mx-1" />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 px-2 text-[11px] font-mono gap-1 text-muted-foreground hover:text-primary hover:bg-primary/10"
                onClick={handleOpenCommentOnSelection}
                title="Add inline comment on selected text"
              >
                <MessageSquare className="size-3" />
                <span>Comment Selection</span>
              </Button>
            </>
          )}
        </div>

        {enableCollab && id && (
          <div className="flex items-center gap-2">
            <div
              className={cn(
                "flex items-center gap-1.5 px-2 py-0.5 rounded text-[10px] font-mono uppercase tracking-wider transition-colors",
                collabConnected
                  ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
                  : "bg-muted/40 text-muted-foreground border border-border"
              )}
              title={collabConnected ? "Real-time sync connected" : "Local draft"}
            >
              {collabConnected ? (
                <>
                  <Wifi className="size-3 text-emerald-500 animate-pulse" />
                  <span>Live Sync</span>
                </>
              ) : (
                <>
                  <WifiOff className="size-3 opacity-60" />
                  <span>Draft</span>
                </>
              )}
            </div>
          </div>
        )}
      </div>

      {/* CodeMirror Workspace */}
      <div className="min-h-[350px] max-h-[650px] overflow-auto relative">
        <CodeMirror
          value={value}
          height="100%"
          theme={isDark ? oneDark : "light"}
          extensions={extensions}
          onCreateEditor={(view) => {
            editorRef.current = view;
          }}
          onChange={(val) => {
            if (!enableCollab) {
              onChange(val);
            }
          }}
          basicSetup={{
            lineNumbers: true,
            highlightActiveLine: true,
            highlightActiveLineGutter: true,
            foldGutter: false,
            syntaxHighlighting: true,
          }}
          className="text-sm font-mono leading-relaxed [&_.cm-editor]:bg-transparent [&_.cm-scroller]:font-mono"
        />
      </div>

      {/* Editor Status Footer */}
      <div className="flex items-center justify-between border-t border-border px-3 py-1.5 bg-muted/10 text-[11px] font-mono text-muted-foreground select-none">
        <div className="flex items-center gap-3">
          <span>{stats.words} words</span>
          <span>•</span>
          <span>{stats.chars} characters</span>
          <span>•</span>
          <span>{stats.lines} lines</span>
        </div>

        <div className="flex items-center gap-2 text-[10px] opacity-70">
          <span>Markdown</span>
          <span>•</span>
          <span>⌘S to save</span>
        </div>
      </div>

      {/* Inline Comment Popover */}
      {inlinePopover && onAddComment && (
        <InlineCommentPopover
          selectedText={inlinePopover.text}
          position={inlinePopover.pos}
          onSubmit={async (comment, quote) => {
            await onAddComment(comment, quote);
            setInlinePopover(null);
          }}
          onClose={() => setInlinePopover(null)}
        />
      )}
    </div>
  );
}
