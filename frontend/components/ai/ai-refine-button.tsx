"use client"

import { useState } from "react"
import { Sparkles, LoaderCircle, Check, X, Wand2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { getLlmClient, useLlmStatus, formatLoadProgress } from "@rust-rag/llm"
import { toast } from "sonner"

interface AiRefineButtonProps {
  content: string
  onAccept: (newContent: string) => void
}

export function AiRefineButton({ content, onAccept }: AiRefineButtonProps) {
  const [isInputOpen, setIsInputOpen] = useState(false)
  const [instruction, setInstruction] = useState("")
  const [running, setRunning] = useState(false)
  const [refinedText, setRefinedText] = useState<string | null>(null)
  const status = useLlmStatus()
  
  const run = async () => {
    if (!instruction.trim()) {
      toast.error("Please provide a refinement instruction")
      return
    }
    
    setRunning(true)
    const client = getLlmClient()
    try {
      const prompt = `You are a helpful writing assistant. 
Refine the following text based on this instruction: "${instruction}"
Keep the original markdown formatting where appropriate.
Output ONLY the refined text. No explanations.

ORIGINAL TEXT:
${content}

REFINED TEXT:`

      let result = ""
      await client.generate(prompt, (partial) => {
        result = partial
        setRefinedText(partial)
      })

      setRefinedText(result.trim())
    } catch (err) {
      toast.error(`Refinement failed: ${err instanceof Error ? err.message : String(err)}`)
      setRefinedText(null)
    } finally {
      setRunning(false)
    }
  }

  const handleAccept = () => {
    if (refinedText) {
      onAccept(refinedText)
      reset()
      toast.success("Refinement applied")
    }
  }

  const reset = () => {
    setIsInputOpen(false)
    setRefinedText(null)
    setInstruction("")
  }

  const isLoading = status.kind === "loading"
  const isBusy = running || isLoading

  return (
    <div className="flex flex-col gap-3">
      {!isInputOpen && !refinedText ? (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-2 gap-1.5 text-[10px] uppercase tracking-[2px] font-black text-primary hover:bg-primary/10 w-fit transition-all duration-300 hover:tracking-[3px]"
          onClick={() => setIsInputOpen(true)}
        >
          <Wand2 className="size-3" />
          Refine with AI
        </Button>
      ) : (
        <div className="relative overflow-hidden rounded-lg border border-primary/30 bg-black/40 backdrop-blur-md p-4 shadow-[0_0_20px_rgba(var(--primary-rgb),0.05)] animate-in fade-in zoom-in-95 duration-300">
          {/* Subtle background glow */}
          <div className="absolute -right-20 -top-20 size-40 bg-primary/5 blur-[50px] pointer-events-none" />
          
          <div className="relative z-10 space-y-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Sparkles className="size-3.5 text-primary animate-pulse" />
                <span className="font-mono text-[10px] font-black uppercase tracking-[3px] text-primary">
                  {refinedText ? "Suggested Refinement" : "AI Assistant"}
                </span>
              </div>
              
              <div className="flex items-center gap-2">
                {refinedText ? (
                  <>
                    <Button 
                      variant="ghost" 
                      size="xs" 
                      className="h-7 px-3 text-[9px] uppercase tracking-wider font-bold text-muted-foreground hover:text-foreground" 
                      onClick={reset}
                    >
                      <X className="size-3 mr-1.5" /> Discard
                    </Button>
                    <Button 
                      size="xs" 
                      className="h-7 px-4 text-[9px] uppercase tracking-[1.5px] font-black bg-primary text-primary-foreground hover:bg-primary/90 shadow-[0_0_15px_rgba(var(--primary-rgb),0.3)]" 
                      onClick={handleAccept}
                    >
                      <Check className="size-3.5 mr-1.5" /> Accept
                    </Button>
                  </>
                ) : (
                  <Button variant="ghost" size="icon" className="size-6 text-muted-foreground hover:text-foreground" onClick={reset}>
                    <X className="size-4" />
                  </Button>
                )}
              </div>
            </div>

            {!refinedText ? (
              <div className="space-y-3">
                <div className="flex gap-2">
                  <div className="relative flex-1">
                    <Input
                      placeholder="e.g. 'Summarize into a list', 'Make it more professional'..."
                      value={instruction}
                      onChange={(e) => setInstruction(e.target.value)}
                      className="h-9 text-xs bg-black/40 border-primary/20 focus-visible:ring-primary/40 pl-8 font-mono italic"
                      autoFocus
                      onKeyDown={(e) => e.key === "Enter" && run()}
                    />
                    <Wand2 className="absolute left-2.5 top-2.5 size-3.5 text-primary/40" />
                  </div>
                  <Button 
                    size="sm" 
                    className="h-9 px-4 gap-2 text-[10px] uppercase font-black tracking-widest bg-primary/10 text-primary hover:bg-primary/20 border border-primary/30" 
                    onClick={run}
                    disabled={isBusy}
                  >
                    {isBusy ? <LoaderCircle className="size-3.5 animate-spin" /> : "Run"}
                  </Button>
                </div>
                {isLoading && (
                  <div className="flex items-center gap-3 px-1">
                    <div className="flex-1 h-[2px] bg-primary/10 overflow-hidden">
                      <div className="h-full bg-primary animate-progress-indefinite" style={{ width: '30%' }} />
                    </div>
                    <span className="text-[9px] font-mono text-primary/60 tabular-nums uppercase tracking-tighter">
                      {formatLoadProgress(status)}
                    </span>
                  </div>
                )}
              </div>
            ) : (
              <div className="group relative">
                <div className="absolute -inset-0.5 bg-gradient-to-r from-primary/20 to-transparent opacity-50 blur rounded" />
                <div className="relative max-h-[300px] overflow-y-auto rounded border border-primary/20 bg-black/60 p-4 text-[13px] leading-relaxed font-mono text-primary/90 shadow-inner custom-scrollbar">
                  {refinedText}
                  {running && (
                    <span className="inline-block w-2 h-4 ml-1 bg-primary animate-pulse align-middle" />
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      <style jsx global>{`
        @keyframes progress-indefinite {
          0% { transform: translateX(-100%); }
          100% { transform: translateX(400%); }
        }
        .animate-progress-indefinite {
          animation: progress-indefinite 2s infinite linear;
        }
        .custom-scrollbar::-webkit-scrollbar { width: 4px; }
        .custom-scrollbar::-webkit-scrollbar-track { background: transparent; }
        .custom-scrollbar::-webkit-scrollbar-thumb { background: rgba(var(--primary-rgb), 0.2); border-radius: 10px; }
        .custom-scrollbar::-webkit-scrollbar-thumb:hover { background: rgba(var(--primary-rgb), 0.4); }
      `}</style>
    </div>
  )
}
