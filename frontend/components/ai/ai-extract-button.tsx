"use client"

import { useState } from "react"
import { Sparkles, LoaderCircle, AlertTriangle } from "lucide-react"
import { Button } from "@/components/ui/button"
import { getLlmClient, useLlmStatus, formatLoadProgress, isWebGpuAvailable } from "@rust-rag/llm"
import { toast } from "sonner"
import { safeJsonParse } from "@/lib/utils/json"

interface AiExtractButtonProps {
  content: string
  schema: any
  onExtract: (data: any) => void
  onExtractError?: (raw: string) => void
}

export function AiExtractButton({ content, schema, onExtract, onExtractError }: AiExtractButtonProps) {
  const [running, setRunning] = useState(false)
  const status = useLlmStatus()
  
  const run = async () => {
    if (!content.trim()) {
      toast.error("No content to extract from")
      return
    }
    
    setRunning(true)
    const client = getLlmClient()
    try {
      // We use a specific prompt designed for data extraction
      const prompt = `You are a data extraction assistant. 
Extract structured data from the following text that conforms EXACTLY to the provided JSON schema.
Output ONLY the raw JSON object. Do not include markdown code blocks, do not include explanations.

JSON SCHEMA:
${JSON.stringify(schema, null, 2)}

TEXT CONTENT:
${content}

JSON DATA:`

      let result = ""
      await client.generate(prompt, (partial) => {
        result = partial
      })

      const parsed = safeJsonParse(result)
      if (parsed) {
        onExtract(parsed)
        toast.success("Structured data extracted!")
      } else {
        onExtractError?.(result)
        toast.error("Extraction failed: AI returned invalid JSON")
      }
    } catch (err) {
      console.error("Extraction failed", err)
      toast.error(`Extraction failed: ${err instanceof Error ? err.message : "Invalid JSON returned"}`)
    } finally {
      setRunning(false)
    }
  }

  if (!isWebGpuAvailable()) return null

  const isLoading = status.kind === "loading"
  const isBusy = running || isLoading

  return (
    <div className="flex items-center gap-2">
      <Button
        variant="outline"
        size="sm"
        className="h-7 px-2 gap-1.5 text-[10px] uppercase tracking-wider font-mono border-primary/40 text-primary hover:bg-primary/10 transition-all duration-300 shadow-[0_0_10px_rgba(var(--primary-rgb),0.1)]"
        onClick={run}
        disabled={isBusy}
      >
        {isBusy ? (
          <LoaderCircle className="size-3 animate-spin" />
        ) : (
          <Sparkles className="size-3" />
        )}
        {running ? "Extracting..." : isLoading ? "Loading model..." : "Auto-fill"}
      </Button>
      {isLoading && (
        <span className="text-[9px] font-mono text-muted-foreground/60 animate-pulse">
          {formatLoadProgress(status)}
        </span>
      )}
    </div>
  )
}
