"use client"

import { useState, useEffect } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
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
import { toast } from "sonner"
import { cn } from "@/lib/utils"

interface EntryDetailProps {
  id: string
}

export function EntryDetail({ id }: EntryDetailProps) {
  const router = useRouter()
  const { mutate } = useSWRConfig()
  const { data: entry, isLoading, error } = useItem(id)
  const { data: graphStatus } = useGraphStatus()
  const { data: edges } = useEdgesForItem(graphStatus?.enabled ? id : null)
  const { trigger: deleteItem } = useDeleteItem()
  const { trigger: updateItem } = useUpdateItem(id)

  const [isEditing, setIsEditing] = useState(false)
  const [editedText, setEditedText] = useState("")

  useEffect(() => {
    if (entry) {
      setEditedText(entry.text)
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
      })
      mutate(["items", id])
      setIsEditing(false)
      toast.success("Entry updated successfully")
    } catch (err) {
      toast.error("Failed to update entry")
    }
  }

  if (isLoading) {
    return (
      <div className="flex h-[calc(100vh-3.5rem)] items-center justify-center">
        <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  if (error || !entry) {
    return (
      <div className="flex h-[calc(100vh-3.5rem)] flex-col items-center justify-center text-center">
        <h3 className="mb-2 text-lg font-medium">Entry not found</h3>
        <p className="mb-4 text-sm text-muted-foreground">The entry you're looking for doesn't exist.</p>
        <Button asChild>
          <Link href="/entries">Back to Entries</Link>
        </Button>
      </div>
    )
  }

  const getSourceVariant = (source: string) => {
    const s = source.toLowerCase()
    if (s.includes('manual')) return 'warning'
    if (s.includes('auto')) return 'info'
    if (s.includes('imported')) return 'purple'
    return 'outline'
  }

  return (
    <div className="flex h-[calc(100vh-3.5rem)] flex-col overflow-hidden bg-background">
      {/* Header bar */}
      <div className="flex h-16 shrink-0 items-center justify-between border-b px-6 bg-background/50 backdrop-blur-sm">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="icon" asChild className="rounded-full">
            <Link href="/entries">
              <ArrowLeft className="size-4" />
            </Link>
          </Button>
          <div className="flex flex-col">
            <h1 className="text-lg font-black tracking-tighter uppercase text-foreground/80 leading-none">Intelligence Fragment</h1>
            <div className="flex items-center gap-2 mt-1">
              <span className="text-[10px] font-bold text-muted-foreground/40 tabular-nums uppercase tracking-widest">ID: {entry.id.substring(0, 8)}...</span>
              <Badge variant={getSourceVariant(entry.source_id)} className="px-1.5 py-0 text-[8px] uppercase font-black tracking-[0.15em]">
                {entry.source_id}
              </Badge>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button 
            variant={isEditing ? "default" : "outline"} 
            size="sm" 
            onClick={isEditing ? handleSave : () => setIsEditing(true)}
            className="shadow-sm transition-all"
          >
            {isEditing ? (
              <><Save className="size-4 mr-2" /> Save Changes</>
            ) : (
              <><Pencil className="size-4 mr-2" /> Edit Mode</>
            )}
          </Button>
          {isEditing && (
            <Button variant="ghost" size="sm" onClick={() => setIsEditing(false)}>
              Cancel
            </Button>
          )}
          <ComboButton 
            onConfirm={handleDelete}
            className="text-muted-foreground"
          />
        </div>
      </div>

      <ResizablePanelGroup direction="horizontal" className="flex-1">
        <ResizablePanel defaultSize={60} minSize={30}>
          <div className="flex h-full flex-col overflow-y-auto px-10 py-8 scrollbar-thin">
            <div className="mx-auto w-full max-w-3xl">
              {isEditing ? (
                <div className="space-y-4 animate-in fade-in slide-in-from-top-2 duration-300">
                  <div className="flex items-center justify-between">
                    <h2 className="text-sm font-bold uppercase tracking-widest text-muted-foreground/60">Content Editor</h2>
                    <Badge variant="outline" className="bg-primary/5 text-primary border-primary/20">Drafting</Badge>
                  </div>
                  <Textarea
                    value={editedText}
                    onChange={(e) => setEditedText(e.target.value)}
                    className="min-h-[60vh] text-base leading-relaxed p-6 rounded-2xl border-muted/50 focus-visible:ring-primary/20 resize-none shadow-inner bg-muted/5 font-mono"
                    placeholder="Write your content here... (Markdown supported)"
                  />
                </div>
              ) : (
                <div className="space-y-8 animate-in fade-in duration-500">
                  <div className="flex flex-col gap-6">
                    <div>
                      <h2 className="text-sm font-bold uppercase tracking-widest text-muted-foreground/60 mb-4">Content</h2>
                      <div className="bg-card rounded-2xl border border-muted/40 p-8 shadow-sm">
                        <MarkdownView content={entry.text} />
                      </div>
                    </div>

                    {Object.keys(entry.metadata).length > 0 && (
                      <div>
                        <h2 className="text-sm font-bold uppercase tracking-widest text-muted-foreground/60 mb-4">Properties</h2>
                        <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
                          {Object.entries(entry.metadata).map(([key, value]) => (
                            <div key={key} className="flex flex-col gap-1 rounded-xl bg-muted/20 p-3.5 border border-muted/40 transition-colors hover:bg-muted/30">
                              <span className="text-[10px] font-bold uppercase text-muted-foreground/50 tracking-widest">{key}</span>
                              <span className="text-sm font-semibold truncate text-foreground/90">{String(value)}</span>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={40} minSize={20}>
          <div className="flex h-full flex-col bg-muted/5">
            <div className="flex h-12 shrink-0 items-center justify-between border-b px-4 bg-background/30 backdrop-blur-sm">
              <div className="flex items-center gap-2">
                <GitBranch className="size-4 text-primary" />
                <span className="text-xs font-bold uppercase tracking-widest text-muted-foreground">Neural Context</span>
              </div>
              {graphStatus?.enabled && (
                <Button variant="ghost" size="sm" asChild className="h-8 text-[10px] font-bold uppercase">
                  <Link href={`/visualize?focus=${encodeURIComponent(id)}`}>
                    Full View
                  </Link>
                </Button>
              )}
            </div>
            
            <div className="flex-1 relative">
              {graphStatus?.enabled ? (
                <EmbeddedGraph 
                  centerId={id} 
                  onNodeClick={(clickedId) => {
                    if (clickedId !== id) {
                      router.push(`/entries/${encodeURIComponent(clickedId)}`)
                    }
                  }} 
                />
              ) : (
                <div className="flex h-full flex-col items-center justify-center p-8 text-center text-muted-foreground">
                  <GitBranch className="size-8 mb-4 opacity-20" />
                  <p className="text-sm font-medium">Graph context unavailable</p>
                  <p className="text-xs mt-1">Enable graph support to visualize relationships.</p>
                </div>
              )}
            </div>

            {edges && edges.length > 0 && (
              <div className="h-1/3 shrink-0 border-t bg-card/50 overflow-y-auto p-4 flex flex-col gap-3">
                <h3 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/70">Connected Nodes</h3>
                <div className="flex flex-col gap-2">
                  {edges.map((edge) => (
                    <div
                      key={edge.id}
                      className="group flex items-center justify-between rounded-xl border border-muted/30 bg-background/50 p-3 transition-all hover:border-primary/30 hover:shadow-sm"
                    >
                      <div className="flex flex-col gap-1">
                        <Badge variant="indigo" className="w-fit text-[9px] font-bold uppercase py-0 px-1.5 opacity-80">
                          {edge.relationship}
                        </Badge>
                        <Link
                          href={`/entries/${encodeURIComponent(
                            edge.source_id === id ? edge.target_id : edge.source_id
                          )}`}
                          className="text-xs font-bold hover:text-primary transition-colors truncate max-w-[150px]"
                        >
                          {edge.source_id === id ? edge.target_id : edge.source_id}
                        </Link>
                      </div>
                      <span className="text-[9px] uppercase font-bold text-muted-foreground/40">
                        {edge.source_id === id ? "out" : "in"}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  )
}
