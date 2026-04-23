import { useState, useEffect } from "react"
import { ArrowLeft, Pencil, Trash2, GitBranch, Save } from "lucide-react"
import { Button } from "@/components/ui/button"
import { ComboButton } from "@/components/ui/combo-button"
import { Badge } from "@/components/ui/badge"
import { useItem, useDeleteItem, useEdgesForItem, useGraphStatus } from "@/lib/api"
import { useSWRConfig } from "swr"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { MarkdownView } from "./markdown-view"
import { EmbeddedGraph } from "../graph/embedded-graph"
import { Textarea } from "@/components/ui/textarea"
import { useUpdateItem } from "@/lib/api"
import { useIsMobile } from "@/hooks/use-mobile"
import { toast } from "sonner"

interface EntryDetailProps {
  id: string
}

export function EntryDetail({ id }: EntryDetailProps) {
  const { mutate } = useSWRConfig()
  const isMobile = useIsMobile()
  const { data: entry, isLoading, error } = useItem(id)
  const { data: graphStatus } = useGraphStatus()
  const { data: edges } = useEdgesForItem(graphStatus?.enabled ? id : null)
  const { trigger: deleteItem } = useDeleteItem()
  const { trigger: updateItem } = useUpdateItem(id)

  const [isEditing, setIsEditing] = useState(false)
  const [editedText, setEditedText] = useState("")

  useEffect(() => {
    if (entry) setEditedText(entry.text)
  }, [entry])

  const handleDelete = async () => {
    await deleteItem(id)
    mutate("items")
    mutate("categories")
    window.location.href = "/entries/"
  }

  const handleSave = async () => {
    try {
      await updateItem({
        text: editedText,
        source_id: entry?.source_id ?? "knowledge",
        metadata: entry?.metadata ?? {},
      })
      mutate(["items", id])
      setIsEditing(false)
      toast.success("Entry updated")
    } catch {
      toast.error("Failed to update entry")
    }
  }

  if (isLoading) {
    return (
      <div className="flex h-[calc(100vh-3rem)] items-center justify-center gap-3">
        <div className="size-6 animate-spin border-2 border-border border-t-primary" />
        <span className="font-mono text-xs uppercase tracking-widest text-muted-foreground animate-pulse">
          Loading...
        </span>
      </div>
    )
  }

  if (error || !entry) {
    return (
      <div className="flex h-[calc(100vh-3rem)] flex-col items-center justify-center text-center gap-4">
        <p className="font-mono text-xs uppercase tracking-widest text-muted-foreground">Entry not found</p>
        <Button asChild variant="outline" size="sm">
          <a href="/entries/">Back to Entries</a>
        </Button>
      </div>
    )
  }

  // ── Shared header ──────────────────────────────────────
  const header = (
    <div className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4 bg-background">
      <div className="flex items-center gap-3 min-w-0">
        <Button variant="ghost" size="icon" className="size-8 shrink-0" asChild>
          <a href="/entries/">
            <ArrowLeft className="size-4" />
          </a>
        </Button>
        <div className="flex flex-col min-w-0">
          <h1 className="font-mono text-xs font-black uppercase tracking-[2px] text-foreground leading-none">
            Fragment
          </h1>
          <div className="flex items-center gap-2 mt-0.5">
            <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
              {entry.id.substring(0, 12)}…
            </span>
            <span className="font-mono text-[10px] font-bold uppercase tracking-wider px-1.5 py-0.5 border border-border text-muted-foreground">
              {entry.source_id}
            </span>
          </div>
        </div>
      </div>

      <div className="flex items-center gap-2 shrink-0">
        <Button
          variant={isEditing ? "default" : "outline"}
          size="sm"
          className="font-mono text-[10px] uppercase tracking-[1.5px] h-8"
          onClick={isEditing ? handleSave : () => setIsEditing(true)}
        >
          {isEditing ? (
            <><Save className="size-3.5 mr-1.5" />Save</>
          ) : (
            <><Pencil className="size-3.5 mr-1.5" />Edit</>
          )}
        </Button>
        {isEditing && (
          <Button
            variant="ghost"
            size="sm"
            className="font-mono text-[10px] uppercase tracking-[1.5px] h-8"
            onClick={() => setIsEditing(false)}
          >
            Cancel
          </Button>
        )}
        <ComboButton onConfirm={handleDelete} className="size-8" />
      </div>
    </div>
  )

  // ── Main content section ───────────────────────────────
  const contentSection = (
    <div className="flex h-full flex-col overflow-y-auto px-5 md:px-10 py-6 md:py-8">
      <div className="mx-auto w-full max-w-3xl space-y-8">
        {isEditing ? (
          <div className="space-y-3 animate-in fade-in slide-in-from-top-2 duration-300">
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
                Editor
              </span>
              <span className="font-mono text-[10px] px-2 py-0.5 border border-primary/30 text-primary bg-primary/5">
                Drafting
              </span>
            </div>
            <Textarea
              value={editedText}
              onChange={(e) => setEditedText(e.target.value)}
              className="min-h-[60vh] text-sm leading-relaxed p-4 border-border focus-visible:border-primary focus-visible:ring-0 resize-none bg-card font-mono"
              placeholder="Write your content here... (Markdown supported)"
            />
          </div>
        ) : (
          <div className="space-y-8 animate-in fade-in duration-500">
            {/* Content */}
            <div>
              <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground mb-4">
                Content
              </h2>
              <div className="border border-border bg-card p-6">
                <MarkdownView content={entry.text} />
              </div>
            </div>

            {/* Metadata */}
            {Object.keys(entry.metadata).length > 0 && (
              <div>
                <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground mb-4">
                  Properties
                </h2>
                <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                  {Object.entries(entry.metadata).map(([key, value]) => (
                    <div
                      key={key}
                      className="flex flex-col gap-1 border border-border bg-card p-3 hover:border-border/80 transition-colors"
                    >
                      <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                        {key}
                      </span>
                      <span className="text-sm font-medium truncate text-foreground">
                        {String(value)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )

  // ── Graph panel ────────────────────────────────────────
  const graphPanel = (
    <div className="flex h-full flex-col bg-background">
      <div className="flex h-10 shrink-0 items-center justify-between border-b border-border px-4">
        <div className="flex items-center gap-2">
          <GitBranch className="size-3.5 text-primary" />
          <span className="font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground">
            Connections
          </span>
        </div>
        {graphStatus?.enabled && (
          <a
            href={`/visualize/?focus=${encodeURIComponent(id)}`}
            className="font-mono text-[10px] font-bold uppercase tracking-[1px] text-muted-foreground hover:text-primary transition-colors"
          >
            Full View →
          </a>
        )}
      </div>

      <div className="flex-1 relative overflow-hidden">
        {graphStatus?.enabled ? (
          <EmbeddedGraph
            centerId={id}
            onNodeClick={(clickedId) => {
              if (clickedId !== id) window.location.href = `/entries/?id=${encodeURIComponent(clickedId)}`
            }}
          />
        ) : (
          <div className="flex h-full flex-col items-center justify-center p-8 text-center">
            <GitBranch className="size-6 mb-3 text-muted-foreground/30" />
            <p className="font-mono text-xs text-muted-foreground">Graph unavailable</p>
          </div>
        )}
      </div>

      {edges && edges.length > 0 && (
        <div className="h-1/3 shrink-0 border-t border-border bg-card overflow-y-auto p-3 flex flex-col gap-2">
          <h3 className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
            Connected — {edges.length}
          </h3>
          <div className="flex flex-col gap-1.5">
            {edges.map((edge) => {
              const targetId = edge.source_id === id ? edge.target_id : edge.source_id
              return (
                <div
                  key={edge.id}
                  className="flex items-center justify-between border border-border bg-background p-2.5 hover:border-primary/30 transition-colors"
                >
                  <div className="flex flex-col gap-0.5 min-w-0">
                    <span className="font-mono text-[10px] font-bold text-primary uppercase tracking-wider">
                      {edge.relationship}
                    </span>
                    <a
                      href={`/entries/?id=${encodeURIComponent(targetId)}`}
                      className="font-mono text-xs text-muted-foreground hover:text-primary transition-colors truncate"
                    >
                      {targetId.substring(0, 20)}…
                    </a>
                  </div>
                  <span className="font-mono text-[10px] uppercase text-muted-foreground/60 shrink-0 ml-2">
                    {edge.source_id === id ? "out" : "in"}
                  </span>
                </div>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )

  // ── Mobile layout (no graph) ───────────────────────────
  if (isMobile) {
    return (
      <div className="flex flex-col overflow-hidden bg-background" style={{ height: "calc(100vh - 3rem)" }}>
        {header}
        {contentSection}
      </div>
    )
  }

  // ── Desktop layout (resizable split) ──────────────────
  return (
    <div className="flex h-[calc(100vh-3rem)] flex-col overflow-hidden bg-background">
      {header}
      <ResizablePanelGroup direction="horizontal" className="flex-1 overflow-hidden">
        <ResizablePanel defaultSize={60} minSize={30}>
          {contentSection}
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={40} minSize={20}>
          {graphPanel}
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  )
}
