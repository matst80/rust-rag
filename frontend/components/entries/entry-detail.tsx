"use client"

import { useState, useEffect } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import { ArrowLeft, Pencil, Trash2, GitBranch, Save, Copy, Check } from "lucide-react"
import { Button } from "@/components/ui/button"
import { ComboButton } from "@/components/ui/combo-button"
import { Badge } from "@/components/ui/badge"
import { useItem, useDeleteItem, useEdgesForItem, useGraphStatus } from "@/lib/api"
import { useSchema, useSchemas } from "@/lib/api/hooks"
import { useSWRConfig } from "swr"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { MarkdownView } from "./markdown-view"
import { EmbeddedGraph } from "../graph/embedded-graph"
import { AttachmentsPanel } from "./attachments-panel"
import { WikiPathPicker } from "./wiki-path-picker"
import { AiAssistPanel } from "../ai/ai-assist-panel"
import { AnalysisPanel } from "./analysis-panel"
import { Textarea } from "@/components/ui/textarea"
import { useUpdateItem } from "@/lib/api"
import { useIsMobile } from "@/hooks/use-mobile"
import { toast } from "sonner"
import { StructuredDataEditor } from "./structured-data-editor"
import { StructuredDataView } from "./structured-data-view"
import { EntryTag, EntryTagList } from "../ui/entry-tag"
import { AiRefineButton } from "../ai/ai-refine-button"

interface EntryDetailProps {
  id: string
}

export function EntryDetail({ id }: EntryDetailProps) {
  const router = useRouter()
  const { mutate } = useSWRConfig()
  const isMobile = useIsMobile()
  const { data: entry, isLoading, error } = useItem(id)
  const { data: graphStatus } = useGraphStatus()
  const { data: edges } = useEdgesForItem(graphStatus?.enabled ? id : null)
  const { trigger: deleteItem } = useDeleteItem()
  const { trigger: updateItem } = useUpdateItem(id)

  const [isEditing, setIsEditing] = useState(false)
  const [editedText, setEditedText] = useState("")
  const [editedType, setEditedType] = useState<string>("")
  const [editedData, setEditedData] = useState<any>(null)
  const [isDataValid, setIsDataValid] = useState(true)
  const [idCopied, setIdCopied] = useState(false)

  const { data: schemas } = useSchemas()
  const { data: schema } = useSchema(editedType)

  const handleCopyId = async () => {
    try {
      await navigator.clipboard.writeText(entry?.id ?? "")
      setIdCopied(true)
      setTimeout(() => setIdCopied(false), 1500)
    } catch {
      toast.error("Copy failed")
    }
  }

  useEffect(() => {
    if (entry) {
      setEditedText(entry.text)
      setEditedType(entry.type ?? "")
      setEditedData(entry.data)
    }
  }, [entry])

  const handleDelete = async () => {
    await deleteItem(id)
    mutate("items")
    mutate("categories")
    router.push("/entries")
  }

  const handleSave = async () => {
    try {
      await updateItem({
        text: editedText,
        source_id: entry?.source_id ?? "knowledge",
        metadata: entry?.metadata ?? {},
        path: entry?.path ?? undefined,
        type: editedType || null,
        data: editedType ? editedData : null,
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
          <Link href="/entries">Back to Entries</Link>
        </Button>
      </div>
    )
  }

  // ── Shared header ──────────────────────────────────────
  const header = (
    <div className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4 bg-background">
      <div className="flex items-center gap-3 min-w-0">
        <Button variant="ghost" size="icon" className="size-8 shrink-0" asChild>
          <Link href="/entries">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div className="flex flex-col min-w-0">
          <h1 className="font-mono text-xs font-black uppercase tracking-[2px] text-foreground leading-none">
            Fragment
          </h1>
          <div className="flex items-center gap-2 mt-0.5">
            <button
              type="button"
              onClick={handleCopyId}
              title={`Copy id: ${entry.id}`}
              className="font-mono text-[10px] text-muted-foreground tabular-nums inline-flex items-center gap-1 hover:text-primary transition-colors"
            >
              <span>{entry.id.substring(0, 12)}…</span>
              {idCopied ? (
                <Check className="size-3 text-emerald-500" />
              ) : (
                <Copy className="size-3 opacity-60" />
              )}
            </button>
            <EntryTag label={entry.source_id} icon={false} />
            <WikiPathPicker entry={entry} />
            {entry.path && (
              <Link
                href={`/wiki?source_id=${encodeURIComponent(entry.source_id)}&path=${encodeURIComponent(entry.path)}`}
                className="font-mono text-[10px] uppercase tracking-wider px-1.5 py-0.5 border border-border text-muted-foreground hover:text-primary hover:border-primary/40 transition-colors"
                title="Open this wiki folder"
              >
                ↗
              </Link>
            )}
          </div>
        </div>
      </div>

      <div className="flex items-center gap-2 shrink-0">
        <Button
          variant={isEditing ? "default" : "outline"}
          size="sm"
          className="font-mono text-[10px] uppercase tracking-[1.5px] h-8"
          onClick={isEditing ? handleSave : () => setIsEditing(true)}
          disabled={isEditing && !isDataValid}
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
              <AiRefineButton content={editedText} onAccept={setEditedText} />
            </div>
            <Textarea
              value={editedText}
              onChange={(e) => setEditedText(e.target.value)}
              className="min-h-[30vh] text-sm leading-relaxed p-4 border-border focus-visible:border-primary focus-visible:ring-0 resize-none bg-card font-mono"
              placeholder="Write your content here... (Markdown supported)"
            />

            <div className="pt-4 border-t space-y-3">
              <div className="flex flex-col gap-1.5">
                <span className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
                  Type
                </span>
                <select
                  value={editedType}
                  onChange={(e) => {
                    setEditedType(e.target.value)
                    if (e.target.value && !editedData) setEditedData({})
                  }}
                  className="flex h-9 w-full max-w-xs rounded-md border border-border bg-background px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                >
                  <option value="">— Untyped —</option>
                  {schemas?.map((s) => (
                    <option key={s.type_name} value={s.type_name}>
                      {s.type_name} {s.title ? `(${s.title})` : ""}
                    </option>
                  ))}
                </select>
              </div>

              {editedType && schema && (
                <div className="space-y-3 animate-in fade-in duration-300">
                  <span className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    Structured Data ({editedType})
                  </span>
                  <StructuredDataEditor
                    schema={schema.json_schema}
                    value={editedData}
                    onChange={setEditedData}
                    onValidityChange={setIsDataValid}
                    typeName={editedType}
                    content={editedText}
                  />
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="space-y-8 animate-in fade-in duration-500">
            {/* Image preview */}
            {entry.metadata.source_type === "image" && entry.metadata.source_file && (
              <div>
                <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground mb-4">
                  Image
                </h2>
                <div className="border border-border bg-card overflow-hidden">
                  <img
                    src={String(entry.metadata.source_file)}
                    alt={String(entry.metadata.original_filename ?? "image")}
                    className="max-w-full h-auto"
                  />
                </div>
              </div>
            )}

            {/* Content */}
            <div>
              <div className="flex items-center justify-between mb-4 gap-3 flex-wrap">
                <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
                  Content
                </h2>
                <AiAssistPanel
                  label="Explain this"
                  buildPrompt={() =>
                    `Summarize the following note in 3-5 sentences (markdown). Identify what it is, why it exists, and the single most useful takeaway. If it's a runbook or list, surface the key steps as bullets.

Title: ${entry.id}
Source: ${entry.source_id}

---
${(entry.text ?? "").slice(0, 6000)}`
                  }
                />
              </div>
              <div className="border border-border bg-card p-6">
                <MarkdownView content={entry.text} />
              </div>
            </div>

            {/* Typed data */}
            {entry.type && entry.data && (
              <div className="space-y-4 animate-in fade-in slide-in-from-bottom-4 duration-700 delay-300">
                <div className="flex items-center gap-2">
                  <div className="size-1.5 rounded-full bg-primary shadow-[0_0_8px_rgba(var(--primary-rgb),0.8)]" />
                  <span className="font-mono text-xs font-black uppercase tracking-[3px] text-primary/80">
                    Data ({entry.type})
                  </span>
                </div>
                <StructuredDataView type={entry.type} data={entry.data} />
              </div>
            )}

            {/* Analysis */}
            <AnalysisPanel entry={entry} />

            {/* Attachments */}
            <AttachmentsPanel itemId={id} />

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
                      <div className="text-sm font-medium truncate text-foreground">
                        {key === "tags" && typeof value === "string" ? (
                          <EntryTagList tags={value.split(",").map(t => t.trim()).filter(Boolean)} />
                        ) : (
                          String(value)
                        )}
                      </div>
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
          <Link
            href={`/visualize?focus=${encodeURIComponent(id)}`}
            className="font-mono text-[10px] font-bold uppercase tracking-[1px] text-muted-foreground hover:text-primary transition-colors"
          >
            Full View →
          </Link>
        )}
      </div>

      <div className="flex-1 relative overflow-hidden">
        {graphStatus?.enabled ? (
          <EmbeddedGraph
            centerId={id}
            onNodeClick={(clickedId) => {
              if (clickedId !== id) router.push(`/entries/${encodeURIComponent(clickedId)}`)
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
                    <Link
                      href={`/entries/${encodeURIComponent(targetId)}`}
                      className="font-mono text-xs text-muted-foreground hover:text-primary transition-colors truncate"
                    >
                      {targetId.substring(0, 20)}…
                    </Link>
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
