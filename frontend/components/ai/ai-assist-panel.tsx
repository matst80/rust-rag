"use client"

import { useEffect, useRef, useState } from "react"
import { Sparkles, X, LoaderCircle, AlertTriangle } from "lucide-react"
import { Button } from "@/components/ui/button"
import { MarkdownView } from "@/components/entries/markdown-view"
import { getLlmClient, formatLoadProgress } from "@/lib/ai/llm-client"
import { useLlmStatus } from "@/lib/ai/use-llm-status"

interface AiAssistPanelProps {
  /** Short button label. */
  label: string
  /** Built lazily on click — keeps token cost off the render path. */
  buildPrompt: () => string
  /** Optional extra hint shown next to the button (e.g. result count). */
  hint?: string
}

export function AiAssistPanel({ label, buildPrompt, hint }: AiAssistPanelProps) {
  const status = useLlmStatus()
  const [open, setOpen] = useState(false)
  const [text, setText] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [running, setRunning] = useState(false)
  const abortRef = useRef<AbortController | null>(null)
  const supported =
    typeof navigator !== "undefined" &&
    typeof (navigator as unknown as { gpu?: unknown }).gpu !== "undefined"

  useEffect(
    () => () => {
      abortRef.current?.abort()
    },
    []
  )

  const run = async () => {
    const client = getLlmClient()
    setOpen(true)
    setText("")
    setError(null)
    setRunning(true)
    const controller = new AbortController()
    abortRef.current = controller
    try {
      const prompt = buildPrompt()
      await client.generate(
        prompt,
        (partial) => setText(partial),
        controller.signal
      )
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      if (msg !== "aborted") setError(msg)
    } finally {
      setRunning(false)
    }
  }

  const cancel = () => {
    abortRef.current?.abort()
    setRunning(false)
  }

  if (!supported) {
    return (
      <div
        className="flex items-center gap-2 px-2 py-1.5 border border-dashed border-border text-[10px] font-mono uppercase tracking-widest text-muted-foreground/70"
        title="WebGPU not available in this browser"
      >
        <AlertTriangle className="size-3" />
        AI assist needs WebGPU
      </div>
    )
  }

  const isLoading = status.kind === "loading"
  const isError = status.kind === "error"
  const isBusy = running || isLoading

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2 flex-wrap">
        <Button
          variant="outline"
          size="sm"
          className="font-mono text-[10px] uppercase tracking-[1.5px] h-8 gap-1.5 border-primary/30 text-primary hover:bg-primary/10"
          onClick={isBusy ? cancel : run}
          disabled={isLoading && !isBusy}
        >
          {isBusy ? (
            <>
              <LoaderCircle className="size-3.5 animate-spin" />
              {running ? "Cancel" : "Loading…"}
            </>
          ) : (
            <>
              <Sparkles className="size-3.5" />
              {label}
            </>
          )}
        </Button>

        {isLoading && (
          <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
            {formatLoadProgress(status)}
          </span>
        )}
        {!isLoading && !running && hint && (
          <span className="font-mono text-[10px] text-muted-foreground/60 uppercase tracking-widest">
            {hint}
          </span>
        )}
        {isError && (
          <span className="font-mono text-[10px] text-destructive">
            {status.message}
          </span>
        )}
      </div>

      {open && (text || running || error) && (
        <div className="relative border border-primary/20 bg-primary/[0.02] p-5">
          <div className="absolute top-2 right-2 flex items-center gap-1">
            {running && (
              <div className="flex items-center gap-1.5 px-2 py-0.5 text-[10px] font-mono uppercase tracking-widest text-primary/70">
                <div className="size-1.5 bg-primary animate-pulse" />
                Generating
              </div>
            )}
            <Button
              variant="ghost"
              size="icon"
              className="size-6"
              onClick={() => {
                cancel()
                setOpen(false)
              }}
            >
              <X className="size-3.5" />
            </Button>
          </div>

          <div className="flex items-center gap-2 mb-3">
            <Sparkles className="size-3.5 text-primary" />
            <span className="font-mono text-[10px] font-bold uppercase tracking-[2px] text-primary/80">
              On-device summary
            </span>
            <span className="font-mono text-[9px] uppercase tracking-widest text-muted-foreground/50">
              gemma-4 · webgpu
            </span>
          </div>

          {error ? (
            <p className="text-sm text-destructive">{error}</p>
          ) : text ? (
            <MarkdownView content={text} />
          ) : (
            <p className="text-sm text-muted-foreground/60 italic">Thinking…</p>
          )}
        </div>
      )}
    </div>
  )
}
