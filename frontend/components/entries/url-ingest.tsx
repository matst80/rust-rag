"use client"

import { useState } from "react"
import { useRouter } from "next/navigation"
import { ArrowLeft, Globe, Sparkles, Zap, ChevronRight, CheckCircle, Info } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { api } from "@/lib/api"
import { toast } from "sonner"
import { cn } from "@/lib/utils"

export function UrlIngest() {
  const router = useRouter()
  const [url, setUrl] = useState("")
  const [sourceId, setSourceId] = useState("web")
  const [useCdp, setUseCdp] = useState(false)
  const [llmClean, setLlmClean] = useState(true)
  const [ingesting, setIngesting] = useState(false)

  const handleIngest = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!url) return

    setIngesting(true)
    try {
      const result = await api.items.ingestUrl({
        url,
        source_id: sourceId,
        use_cdp: useCdp,
        llm_clean: llmClean,
      })

      toast.success("URL ingested and indexed")
      router.push(`/entries/${encodeURIComponent(result.id)}`)
    } catch (err) {
      console.error("Ingestion error:", err)
      toast.error(err instanceof Error ? err.message : "Ingestion failed")
    } finally {
      setIngesting(false)
    }
  }

  return (
    <div className="mx-auto max-w-2xl px-4 py-8 space-y-8">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div>
          <h1 className="font-mono text-xs font-black uppercase tracking-[2px] text-foreground">
            Ingest URL
          </h1>
          <p className="font-mono text-[10px] text-muted-foreground mt-0.5">
            Fetch, clean, and index web content into your knowledge base
          </p>
        </div>
      </div>

      <form onSubmit={handleIngest} className="space-y-6">
        {/* URL Input */}
        <div className="space-y-2">
          <label className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground flex items-center gap-2">
            <Globe className="size-3" /> Target URL
          </label>
          <div className="relative group">
            <Input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/article"
              className="font-mono text-sm pl-10 h-12 border-2 focus:border-primary/50 transition-all"
              required
              type="url"
            />
            <Globe className="absolute left-3.5 top-1/2 -translate-y-1/2 size-4 text-muted-foreground/40 group-focus-within:text-primary/50 transition-colors" />
          </div>
        </div>

        {/* Source ID */}
        <div className="space-y-2">
          <label className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
            Source / Category
          </label>
          <Input
            value={sourceId}
            onChange={(e) => setSourceId(e.target.value)}
            placeholder="web"
            className="font-mono text-sm"
          />
        </div>

        {/* Options Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* CDP Toggle */}
          <button
            type="button"
            onClick={() => setUseCdp(!useCdp)}
            className={cn(
              "flex flex-col gap-2 p-4 border text-left transition-all hover:bg-accent/50",
              useCdp ? "border-primary/50 bg-primary/[0.03]" : "border-border bg-card"
            )}
          >
            <div className="flex items-center justify-between">
              <Zap className={cn("size-4", useCdp ? "text-primary" : "text-muted-foreground")} />
              <div className={cn("size-2 rounded-full", useCdp ? "bg-primary animate-pulse" : "bg-muted-foreground/20")} />
            </div>
            <div>
              <p className="font-mono text-[10px] font-bold uppercase tracking-widest text-foreground">
                JavaScript Rendering
              </p>
              <p className="font-mono text-[9px] text-muted-foreground mt-1 leading-relaxed">
                Use a remote CDP instance to handle dynamic content and SPA pages.
              </p>
            </div>
          </button>

          {/* LLM Clean Toggle */}
          <button
            type="button"
            onClick={() => setLlmClean(!llmClean)}
            className={cn(
              "flex flex-col gap-2 p-4 border text-left transition-all hover:bg-accent/50",
              llmClean ? "border-primary/50 bg-primary/[0.03]" : "border-border bg-card"
            )}
          >
            <div className="flex items-center justify-between">
              <Sparkles className={cn("size-4", llmClean ? "text-primary" : "text-muted-foreground")} />
              <div className={cn("size-2 rounded-full", llmClean ? "bg-primary animate-pulse" : "bg-muted-foreground/20")} />
            </div>
            <div>
              <p className="font-mono text-[10px] font-bold uppercase tracking-widest text-foreground">
                LLM Extraction
              </p>
              <p className="font-mono text-[9px] text-muted-foreground mt-1 leading-relaxed">
                Automatically remove ads, headers, and nav to extract only the core content.
              </p>
            </div>
          </button>
        </div>

        {/* Info Box */}
        <div className="flex gap-3 p-3 bg-muted/30 border border-border text-[11px] text-muted-foreground leading-relaxed">
          <Info className="size-4 shrink-0 text-muted-foreground/60" />
          <p>
            Content will be converted to Markdown before being indexed. 
            Large pages will be automatically chunked for optimal vector retrieval.
          </p>
        </div>

        {/* Submit Button */}
        <Button
          type="submit"
          disabled={!url || ingesting}
          className="w-full h-12 font-mono text-xs uppercase tracking-[2px] group"
        >
          {ingesting ? (
            <>
              <div className="size-3.5 mr-2 animate-spin border border-current border-t-transparent rounded-full" />
              Ingesting content…
            </>
          ) : (
            <>
              Ingest & Index
              <ChevronRight className="size-3.5 ml-1.5 transition-transform group-hover:translate-x-0.5" />
            </>
          )}
        </Button>
      </form>
    </div>
  )
}
