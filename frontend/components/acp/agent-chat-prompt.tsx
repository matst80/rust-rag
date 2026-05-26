"use client";

import { Send, Loader2, StopCircle } from "lucide-react";
import { cn } from "@/lib/utils";
import { WhisperTranscribe } from "@/components/entries/whisper-transcribe";
import { useAutocomplete } from "./use-autocomplete";

interface AgentChatPromptProps {
  draft: string;
  setDraft: (text: string) => void;
  onSend: () => void;
  onCancel?: () => void;
  isWorking?: boolean;
  connStatus: string;
  availableCommands: any[];
  placeholder?: string;
}

export function AgentChatPrompt({
  draft,
  setDraft,
  onSend,
  onCancel,
  isWorking,
  connStatus,
  availableCommands,
  placeholder,
}: AgentChatPromptProps) {
  const {
    acOpen,
    setAcOpen,
    acIndex,
    filteredCommands,
    acListRef,
    handleKeyDown,
  } = useAutocomplete({
    draft,
    availableCommands,
    onSelect: (cmdName) => {
      setDraft(`/${cmdName} `);
      setAcOpen(false);
    },
  });

  return (
    <div className="shrink-0 flex flex-col border-t border-border bg-background relative">
      {/* Autocomplete Menu */}
      {acOpen && filteredCommands.length > 0 && (
        <div className="absolute bottom-full left-0 w-64 bg-popover border border-border rounded-t-lg shadow-xl mb-1 z-50 overflow-hidden">
          <div className="p-2 border-b border-border bg-muted/30">
            <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Available Commands</span>
          </div>
          <ul ref={acListRef} className="max-h-48 overflow-y-auto py-1">
            {filteredCommands.map((cmd, i) => (
              <li key={cmd.name}>
                <button
                  onClick={() => {
                    setDraft(`/${cmd.name} `);
                    setAcOpen(false);
                  }}
                  className={cn(
                    "w-full text-left px-3 py-1.5 text-xs transition-colors flex items-center justify-between",
                    i === acIndex ? "bg-primary text-primary-foreground" : "hover:bg-muted"
                  )}
                >
                  <span className="font-mono">/{cmd.name}</span>
                  <span className={cn("text-[9px] opacity-60 italic", i === acIndex ? "text-primary-foreground" : "")}>
                    {cmd.description?.slice(0, 30)}...
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* PROMPT */}
      <form
        className="border-t border-border"
        onSubmit={(e) => {
          e.preventDefault();
          onSend();
        }}
      >
        <div className="flex items-center gap-3 border border-input bg-background p-1 focus-within:border-primary/50 transition-colors shadow-sm">
          <textarea
            id="agent-message-input"
            name="message"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              const handled = handleKeyDown(e);
              if (!handled && e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                onSend();
              }
            }}
            placeholder={placeholder}
            rows={1}
            className="flex-1 resize-none px-2 bg-transparent text-sm outline-none"
          />
          <div className="flex items-center gap-2 mb-1 shrink-0">
            <div className="flex items-center justify-center">
              <WhisperTranscribe
                onTranscription={(transcription) => {
                  setDraft(draft ? `${draft} ${transcription}` : transcription);
                }}
              />
            </div>
            <button
              type="submit"
              disabled={!draft.trim() || connStatus !== "open"}
              className={cn(
                "flex size-10 items-center justify-center rounded-md transition-all shadow-sm",
                draft.trim() && connStatus === "open"
                  ? "bg-primary text-primary-foreground hover:bg-primary/90 hover:shadow-md"
                  : "bg-muted text-muted-foreground opacity-50 cursor-not-allowed",
              )}
              aria-label="Send"
            >
              {connStatus !== "open" ? (
                <Loader2 className="size-5 animate-spin" />
              ) : (
                <Send className="size-5" />
              )}
            </button>

            {isWorking && onCancel && (
              <button
                type="button"
                onClick={onCancel}
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
  );
}
