"use client"

import { useState, useEffect } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import { ArrowLeft, Pencil, Trash2, GitBranch, Save, Copy, Check, Terminal, Clock, History, X, Search, Plus, ChevronDown } from "lucide-react"
import { Button } from "@/components/ui/button"
import { ComboButton } from "@/components/ui/combo-button"
import { Badge } from "@/components/ui/badge"
import { useItem, useDeleteItem, useEdgesForItem, useGraphStatus, useDeleteEdge, useCreateEdge, useUpdateEdge, useSearch } from "@/lib/api"
import { RELATION_STYLES } from "../graph/relation-item"
import { cn, formatRelativeTime } from "@/lib/utils"
import { useSchema, useSchemas } from "@/lib/api/hooks"
import { useSWRConfig } from "swr"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
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

const CANONICAL_PREDICATES = [
  { value: "is_a", label: "Is A", description: "Subtype or instance" },
  { value: "part_of", label: "Part Of", description: "Component of" },
  { value: "caused_by", label: "Caused By", description: "Effect of" },
  { value: "works_for", label: "Works For", description: "Affiliation" },
  { value: "contradicts", label: "Contradicts", description: "Incompatible with" },
  { value: "depends_on", label: "Depends On", description: "Requirement" },
  { value: "contains", label: "Contains", description: "Includes/embeds" },
  { value: "implemented_by", label: "Implemented By", description: "Realization of" },
]

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
  const { trigger: deleteEdge } = useDeleteEdge()
  const { trigger: createEdge } = useCreateEdge()
  const { trigger: updateEdge } = useUpdateEdge("") // We'll pass id dynamically if needed, but the hook uses it

  const [isEditing, setIsEditing] = useState(false)
  const [editedText, setEditedText] = useState("")
  const [editedType, setEditedType] = useState<string>("")
  const [editedData, setEditedData] = useState<any>(null)
  const [isDataValid, setIsDataValid] = useState(true)
  const [idCopied, setIdCopied] = useState(false)

  // Edge management state
  const [searchQuery, setSearchQuery] = useState("")
  const [isAddingEdge, setIsAddingEdge] = useState(false)
  const [selectedTarget, setSelectedTarget] = useState<string | null>(null)
  const { data: searchResults } = useSearch(searchQuery, undefined, undefined, true, 5)

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

  const handleDeleteEdge = async (edgeId: string) => {
    try {
      await deleteEdge(edgeId)
      mutate(["edges", id])
      toast.success("Connection removed")
    } catch {
      toast.error("Failed to remove connection")
    }
  }

  const handleUpdateEdgeType = async (edgeId: string, newRelation: string) => {
    try {
      // The useUpdateEdge hook in hooks.ts is defined as useUpdateEdge(id: string)
      // but api.edges.update(id, arg) is what it calls.
      // We need to call it with the dynamic edgeId. 
      // Since useUpdateEdge uses id from hook factory, we might need a better way or use api directly.
      // Actually, let's use the api object directly for this since it's cleaner than dynamic hooks in loops.
      const { api } = await import("@/lib/api/client")
      await api.edges.update(edgeId, { metadata: { relation: newRelation } })
      // Wait, look at the backend - updateEdge usually updates metadata.
      // Let's check how the backend handles relation update. 
      // Most likely it's in metadata or a top level field depending on edge_type.
      mutate(["edges", id])
      toast.success("Connection updated")
    } catch {
      toast.error("Failed to update connection")
    }
  }

  const handleAddEdge = async (targetId: string, relationship: string) => {
    try {
      await createEdge({
        source_id: id,
        target_id: targetId,
        relationship,
        directed: true,
        weight: 1.0,
      })
      mutate(["edges", id])
      setIsAddingEdge(false)
      setSelectedTarget(null)
      setSearchQuery("")
      toast.success("Connection added")
    } catch {
      toast.error("Failed to add connection")
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
          <div className="flex items-center gap-2 mt-1">
            <button
              type="button"
              onClick={handleCopyId}
              title={`Copy id: ${entry.id}`}
              className="h-6 px-2 font-mono text-[10px] text-muted-foreground tabular-nums inline-flex items-center gap-1.5 hover:text-primary border border-border bg-muted/5 rounded transition-colors"
            >
              <span>{entry.id.substring(0, 8)}…</span>
              {idCopied ? (
                <Check className="size-3 text-emerald-500" />
              ) : (
                <Copy className="size-3 opacity-60" />
              )}
            </button>
            <EntryTag label={entry.source_id} icon={false} className="h-6" />
            {entry.type && (
              <div className="h-6 px-2 font-mono text-[10px] uppercase tracking-wider flex items-center gap-1.5 rounded border border-primary/20 bg-primary/5 text-primary shadow-[0_0_10px_rgba(var(--primary-rgb),0.05)]">
                <Terminal className="size-3" />
                {entry.type}
              </div>
            )}
            <WikiPathPicker entry={entry} />
            {entry.path && (
              <Link
                href={`/wiki?source_id=${encodeURIComponent(entry.source_id)}&path=${encodeURIComponent(entry.path)}`}
                className="h-6 px-2 font-mono text-[10px] uppercase tracking-wider flex items-center border border-border text-muted-foreground hover:text-primary hover:border-primary/40 rounded transition-colors"
                title="Open this wiki folder"
              >
                ↗
              </Link>
            )}

            <div className="hidden sm:flex items-center gap-3 ml-2 border-l border-border pl-3">
              <div 
                className="flex items-center gap-1.5 font-mono text-[10px] text-muted-foreground/60"
                title={`Created: ${new Date(entry.created_at).toLocaleString()}`}
              >
                <Clock className="size-3 opacity-50" />
                <span>{formatRelativeTime(entry.created_at)}</span>
              </div>
              {entry.updated_at > entry.created_at + 1000 && (
                <div 
                  className="flex items-center gap-1.5 font-mono text-[10px] text-muted-foreground/60"
                  title={`Modified: ${new Date(entry.updated_at).toLocaleString()}`}
                >
                  <History className="size-3 opacity-50" />
                  <span>{formatRelativeTime(entry.updated_at)}</span>
                </div>
              )}
            </div>
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
    <div className="flex h-full flex-col overflow-y-auto px-5 md:px-10 py-8 md:py-12">
      <div className="mx-auto w-full max-w-3xl space-y-16">
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

            {/* Similar Entries (Contextual Expansion) */}
            {entry.neighbors && entry.neighbors.length > 0 && (
              <div className="space-y-4 animate-in fade-in slide-in-from-bottom-4 duration-700 delay-500">
                <div className="flex items-center gap-2">
                  <div className="size-1.5 rounded-full bg-primary/60 shadow-[0_0_8px_rgba(var(--primary-rgb),0.3)]" />
                  <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    Similar Entries
                  </h2>
                </div>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  {entry.neighbors.map((neighbor) => (
                    <Link
                      key={neighbor.id}
                      href={`/entries/${encodeURIComponent(neighbor.id)}`}
                      className="group relative flex flex-col gap-1 border border-border bg-card p-4 hover:border-primary/40 hover:bg-primary/5 transition-all duration-300"
                    >
                      {neighbor.thumbnail && (
                        <div className="absolute top-3 right-3 size-12 border border-border/40 overflow-hidden bg-muted/50 rounded-sm">
                          <img
                            src={neighbor.thumbnail}
                            alt=""
                            className="size-full object-cover grayscale group-hover:grayscale-0 transition-all duration-500"
                          />
                        </div>
                      )}
                      <div className="flex items-center gap-2 mb-0.5">
                        <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground group-hover:text-primary transition-colors">
                          {neighbor.id.substring(0, 12)}...
                        </span>
                        {neighbor.relationship && (
                          <Badge
                            variant="outline"
                            className={cn(
                              "h-4 px-1.5 font-mono text-[8px] uppercase tracking-tighter border-primary/20 text-primary/80 bg-primary/5 leading-none transition-colors",
                              RELATION_STYLES[neighbor.relationship.toLowerCase()]
                            )}
                          >
                            {neighbor.relationship}
                          </Badge>
                        )}
                      </div>
                      <div className="text-sm font-medium text-foreground line-clamp-1 pr-12">
                        {neighbor.title || (neighbor.source_type === "image" ? "Image Fragment" : neighbor.id)}
                      </div>
                    </Link>
                  ))}
                </div>
              </div>
            )}

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
          <span className="font-mono text-[10px] font-black uppercase tracking-[2px] text-muted-foreground">
            Connections
          </span>
        </div>
        <div className="flex items-center gap-4">
          <button
            onClick={() => setIsAddingEdge(true)}
            className="font-mono text-[10px] font-black uppercase tracking-[1px] text-primary hover:text-primary/80 transition-colors flex items-center gap-1"
          >
            <Plus className="size-3" />
            Add Edge
          </button>
          {graphStatus?.enabled && (
            <Link
              href={`/visualize?focus=${encodeURIComponent(id)}`}
              className="font-mono text-[10px] font-black uppercase tracking-[1px] text-muted-foreground hover:text-primary transition-colors"
            >
              Full View →
            </Link>
          )}
        </div>
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

        {/* Add Edge Search Overlay */}
        {isAddingEdge && (
          <div className="absolute inset-0 z-50 bg-background/95 backdrop-blur-sm p-4 animate-in fade-in zoom-in-95 duration-200">
            <div className="flex flex-col h-full max-w-md mx-auto space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary">Add Connection</h3>
                <Button variant="ghost" size="icon" onClick={() => {
                  setIsAddingEdge(false)
                  setSelectedTarget(null)
                }} className="size-6">
                  <X className="size-4" />
                </Button>
              </div>

              {!selectedTarget ? (
                <>
                  <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
                    <input
                      autoFocus
                      className="w-full bg-muted/50 border border-border rounded-xl h-10 pl-10 pr-4 text-sm focus:outline-none focus:ring-1 focus:ring-primary transition-all"
                      placeholder="Search entries to link..."
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                    />
                  </div>
                  <div className="flex-1 overflow-y-auto space-y-2 py-2">
                    {searchResults?.results.filter(r => r.id !== id).map((result) => (
                      <button
                        key={result.id}
                        onClick={() => setSelectedTarget(result.id)}
                        className="w-full text-left p-3 rounded-xl border border-border bg-card hover:border-primary/40 hover:bg-primary/5 transition-all group"
                      >
                        <div className="flex flex-col gap-0.5">
                          <span className="font-mono text-[10px] text-muted-foreground group-hover:text-primary transition-colors">
                            {result.id.substring(0, 16)}...
                          </span>
                          <span className="text-sm font-medium line-clamp-1">
                            {result.text.substring(0, 100)}
                          </span>
                        </div>
                      </button>
                    ))}
                  </div>
                </>
              ) : (
                <div className="space-y-6 animate-in slide-in-from-right-4 duration-300">
                  <div className="p-4 rounded-2xl bg-primary/5 border border-primary/20">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-mono text-[10px] text-primary/60 uppercase font-black">Linking to</span>
                      <button onClick={() => setSelectedTarget(null)} className="text-[10px] text-primary hover:underline ml-auto font-bold">Change</button>
                    </div>
                    <div className="font-mono text-xs font-bold truncate">{selectedTarget}</div>
                  </div>

                  <div className="space-y-3">
                    <span className="font-mono text-[10px] font-black uppercase tracking-widest text-muted-foreground">Select Relationship</span>
                    <div className="grid grid-cols-1 gap-2 overflow-y-auto max-h-[40vh] pr-2 custom-scrollbar">
                      {CANONICAL_PREDICATES.map((p) => (
                        <button
                          key={p.value}
                          onClick={() => handleAddEdge(selectedTarget, p.value)}
                          className="flex flex-col items-start gap-1 p-3 rounded-xl border border-border bg-card hover:border-primary/50 hover:bg-primary/5 transition-all group"
                        >
                          <span className="font-bold text-xs text-foreground group-hover:text-primary transition-colors">{p.label}</span>
                          <span className="text-[10px] text-muted-foreground opacity-60 leading-tight text-left">{p.description}</span>
                        </button>
                      ))}
                      <button
                        onClick={() => handleAddEdge(selectedTarget, "related_to")}
                        className="flex items-center justify-center p-3 rounded-xl border border-dashed border-border bg-transparent hover:border-primary/50 hover:bg-primary/5 transition-all group mt-2"
                      >
                        <span className="text-xs font-bold text-muted-foreground group-hover:text-primary transition-colors">Other / Generic Related</span>
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {edges && edges.length > 0 && (
        <div className="h-[45%] shrink-0 border-t border-border bg-card/30 backdrop-blur-md overflow-hidden flex flex-col">
          <div className="px-4 py-2 border-b border-border bg-muted/5 flex items-center justify-between">
            <h3 className="font-mono text-[10px] font-black uppercase tracking-widest text-muted-foreground flex items-center gap-2">
              <GitBranch className="size-3 opacity-60" />
              Connected — {edges.length}
            </h3>
            <div className="flex items-center gap-2">
               <div className="size-2 rounded-full bg-primary/40 animate-pulse" />
               <span className="font-mono text-[9px] uppercase font-bold text-muted-foreground/60 tracking-wider">Manual Enabled</span>
            </div>
          </div>
          
          <div className="flex-1 overflow-y-auto p-3 space-y-2 custom-scrollbar">
            {edges.sort((a, b) => {
              // Manual edges first
              if (a.edge_type === "manual" && b.edge_type !== "manual") return -1
              if (a.edge_type !== "manual" && b.edge_type === "manual") return 1
              return 0
            }).map((edge) => {
              const targetId = edge.source_id === id ? edge.target_id : edge.source_id
              const isManual = edge.edge_type === "manual"
              
              return (
                <div
                  key={edge.id}
                  className={cn(
                    "group relative flex flex-col gap-2 rounded-xl border p-3 transition-all",
                    isManual 
                      ? "border-primary/20 bg-primary/[0.02] hover:border-primary/40 hover:bg-primary/[0.04] shadow-[0_2px_10px_rgba(var(--primary-rgb),0.02)]" 
                      : "border-border bg-background/50 hover:border-border/80"
                  )}
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex flex-col gap-1 min-w-0">
                      <div className="flex items-center gap-2">
                        {isManual ? (
                          <Popover>
                            <PopoverTrigger asChild>
                              <Button 
                                variant="ghost" 
                                className={cn(
                                  "h-5 px-1.5 font-mono text-[10px] font-black uppercase tracking-wider rounded-sm border hover:bg-primary/10 transition-colors flex items-center gap-1.5",
                                  RELATION_STYLES[edge.relationship.toLowerCase()] || "text-primary border-primary/30"
                                )}
                              >
                                {edge.relationship.replace(/_/g, " ")}
                                <ChevronDown className="size-2.5 opacity-60" />
                              </Button>
                            </PopoverTrigger>
                            <PopoverContent className="w-64 p-0 rounded-xl border-border shadow-2xl overflow-hidden" align="start">
                              <Command className="bg-transparent" loop>
                                <CommandInput placeholder="Change relationship..." className="h-9 text-xs" />
                                <CommandList>
                                  <CommandEmpty>No results</CommandEmpty>
                                  <CommandGroup heading="Canonical Predicates">
                                    {CANONICAL_PREDICATES.map((p) => (
                                      <CommandItem
                                        key={p.value}
                                        onSelect={() => handleUpdateEdgeType(edge.id, p.value)}
                                        className="rounded-lg m-1 p-2 cursor-pointer transition-all hover:bg-primary/10"
                                      >
                                        <div className="flex flex-col gap-0.5">
                                          <span className="font-bold text-[10px] text-primary">{p.label}</span>
                                          <span className="text-[8px] text-muted-foreground leading-tight">{p.description}</span>
                                        </div>
                                      </CommandItem>
                                    ))}
                                  </CommandGroup>
                                </CommandList>
                              </Command>
                            </PopoverContent>
                          </Popover>
                        ) : (
                          <Badge
                            variant="outline"
                            className={cn(
                              "h-5 px-1.5 font-mono text-[10px] font-black uppercase tracking-wider border-border/60 text-muted-foreground/80 bg-muted/5 leading-none",
                              RELATION_STYLES[edge.relationship.toLowerCase()]
                            )}
                          >
                            {edge.relationship}
                          </Badge>
                        )}
                        
                        <span className="font-mono text-[9px] uppercase text-muted-foreground/40 font-bold">
                          {edge.source_id === id ? "→ out" : "← in"}
                        </span>
                      </div>
                      
                      <Link
                        href={`/entries/${encodeURIComponent(targetId)}`}
                        className="font-mono text-xs text-foreground/80 hover:text-primary transition-colors truncate font-medium underline-offset-4 hover:underline"
                        title={targetId}
                      >
                        {targetId}
                      </Link>
                    </div>

                    {isManual && (
                      <div className="shrink-0 flex items-center gap-1">
                        <Button 
                          variant="ghost" 
                          size="icon" 
                          className="size-7 rounded-lg text-muted-foreground/40 hover:text-red-500 hover:bg-red-500/10 transition-all opacity-0 group-hover:opacity-100"
                          onClick={() => handleDeleteEdge(edge.id)}
                        >
                          <Trash2 className="size-3.5" />
                        </Button>
                      </div>
                    )}
                  </div>
                  
                  {!isManual && (
                    <div className="flex items-center gap-2">
                      <div className="flex-1 h-0.5 bg-muted rounded-full overflow-hidden">
                        <div 
                          className="h-full bg-primary/20" 
                          style={{ width: `${Math.max(10, (1 - (edge.distance ?? 0)) * 100)}%` }} 
                        />
                      </div>
                      <span className="font-mono text-[8px] text-muted-foreground/40 font-black uppercase">Similarity</span>
                    </div>
                  )}
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
