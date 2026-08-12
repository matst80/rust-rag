"use client";

import { useEffect, useRef, memo } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

interface TerminalViewProps {
  terminalId: string;
  onInput: (data: string) => void;
  onResize: (cols: number, rows: number) => void;
  onAttach: (cols: number, rows: number) => void;
}

export const TerminalView = memo(function TerminalView({
  terminalId,
  onInput,
  onResize,
  onAttach,
}: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const suppressResize = useRef(false);

  const onInputRef = useRef(onInput);
  const onResizeRef = useRef(onResize);
  const onAttachRef = useRef(onAttach);

  useEffect(() => {
    onInputRef.current = onInput;
  }, [onInput]);

  useEffect(() => {
    onResizeRef.current = onResize;
  }, [onResize]);

  useEffect(() => {
    onAttachRef.current = onAttach;
  }, [onAttach]);

  useEffect(() => {
    if (!containerRef.current) return;

    let term: Terminal | null = null;
    let fitAddon: FitAddon | null = null;
    let destroyed = false;

    const init = async () => {
      // Ensure fonts are loaded so character measurements are accurate
      if (typeof document !== "undefined" && (document as any).fonts) {
        await (document as any).fonts.ready;
      }
      if (destroyed || !containerRef.current) return;

      term = new Terminal({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: "JetBrains Mono",
        letterSpacing: 0,
        scrollback: 10000,
        theme: {
          background: "#030306", // zinc-950
          foreground: "#fafafa", // zinc-50
          cursor: "#fafafa",
          selectionBackground: "rgba(250, 250, 250, 0.3)",
        },
        allowTransparency: true,
        // Keep Option available for character input on macOS. Without this,
        // xterm can interpret Option+2 (the US Mac @ shortcut) as Meta/ESC.
        macOptionIsMeta: false,
      });

      fitAddon = new FitAddon();
      term.loadAddon(fitAddon);

      term.open(containerRef.current);
      fitAddon.fit();

      // On a US Mac keyboard, @ is produced with Option+2. Browsers expose
      // that chord as an Alt+Digit2 key event, which can otherwise be consumed
      // as a Meta/ESC shortcut before xterm emits onData.
      const isApplePlatform = /Mac|iPhone|iPad/.test(
        navigator.platform || navigator.userAgent,
      );
      term.attachCustomKeyEventHandler((event) => {
        if (
          isApplePlatform &&
          event.type === "keydown" &&
          event.altKey &&
          !event.ctrlKey &&
          !event.metaKey &&
          !event.shiftKey &&
          (event.code === "Digit2" || event.key === "@")
        ) {
          term?.input("@", true);
          return false;
        }
        return true;
      });

      // Keep input as text here. The socket hook batches and base64-encodes
      // it so rapid typing and paste operations do not create one websocket
      // frame per xterm event.
      term.onData((data) => {
        onInputRef.current(data);
      });

      term.onResize(({ cols, rows }) => {
        if (suppressResize.current) return;
        onResizeRef.current(cols, rows);
      });

      termRef.current = term;
      fitAddonRef.current = fitAddon;

      // Initial attach
      const dims = fitAddon.proposeDimensions();
      if (dims) {
        onAttachRef.current(dims.cols, dims.rows);
      }
    };

    init();

    let resizeFrame: number | null = null;
    const handleResize = () => {
      if (resizeFrame !== null) cancelAnimationFrame(resizeFrame);
      resizeFrame = requestAnimationFrame(() => {
        resizeFrame = null;
        fitAddonRef.current?.fit();
      });
    };
    window.addEventListener("resize", handleResize);
    window.visualViewport?.addEventListener("resize", handleResize);
    const resizeObserver = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(handleResize);
    if (resizeObserver && containerRef.current) {
      resizeObserver.observe(containerRef.current);
    }

    return () => {
      destroyed = true;
      window.removeEventListener("resize", handleResize);
      window.visualViewport?.removeEventListener("resize", handleResize);
      resizeObserver?.disconnect();
      if (resizeFrame !== null) cancelAnimationFrame(resizeFrame);
      if (term) term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
    };
  }, [terminalId]);

  // Handle output and snapshots via window events (bypassing React state)
  useEffect(() => {
    const handleOutput = (e: Event) => {
      const customEvent = e as CustomEvent<string>;
      const data = customEvent.detail;
      if (!termRef.current || !data) return;

      const binary = atob(data);
      const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
      const decoded = new TextDecoder().decode(bytes);
      termRef.current.write(decoded);
    };

    const handleSnapshot = (e: Event) => {
      const customEvent = e as CustomEvent<{
        data: string;
        cols: number;
        rows: number;
      }>;
      const { data, cols, rows } = customEvent.detail;
      if (!termRef.current || !data) return;

      const binary = atob(data);
      const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
      const decoded = new TextDecoder().decode(bytes);

      const term = termRef.current;
      suppressResize.current = true;
      term.reset();
      if (cols && rows) {
        term.resize(cols, rows);
      }
      term.write(decoded);
      fitAddonRef.current?.fit();
      suppressResize.current = false;
    };

    const handleResized = (e: Event) => {
      const customEvent = e as CustomEvent<{ cols: number; rows: number }>;
      const { cols, rows } = customEvent.detail;
      if (!termRef.current) return;

      suppressResize.current = true;
      termRef.current.resize(cols, rows);
      fitAddonRef.current?.fit();
      suppressResize.current = false;
    };

    const handleResync = () => {
      if (!termRef.current || !fitAddonRef.current) return;
      fitAddonRef.current.fit();
      const dims = fitAddonRef.current.proposeDimensions();
      if (dims) onAttachRef.current(dims.cols, dims.rows);
    };

    window.addEventListener(`acp:term:output:${terminalId}`, handleOutput);
    window.addEventListener(`acp:term:snapshot:${terminalId}`, handleSnapshot);
    window.addEventListener(`acp:term:resized:${terminalId}`, handleResized);
    window.addEventListener(`acp:term:resync:${terminalId}`, handleResync);

    return () => {
      window.removeEventListener(`acp:term:output:${terminalId}`, handleOutput);
      window.removeEventListener(
        `acp:term:snapshot:${terminalId}`,
        handleSnapshot,
      );
      window.removeEventListener(
        `acp:term:resized:${terminalId}`,
        handleResized,
      );
      window.removeEventListener(`acp:term:resync:${terminalId}`, handleResync);
    };
  }, [terminalId]);

  return (
    <div
      className="w-full h-full bg-[#030306] p-2 overflow-hidden rounded-md border border-border/50 shadow-inner touch-manipulation"
      onClick={() => termRef.current?.focus()}
      onKeyDown={(e) => e.stopPropagation()}
      onKeyUp={(e) => e.stopPropagation()}
      onKeyPress={(e) => e.stopPropagation()}
    >
      <div ref={containerRef} className="w-full h-full" />
    </div>
  );
});
