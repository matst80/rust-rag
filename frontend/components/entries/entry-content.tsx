"use client";

import Link from "next/link";
import { GitBranch } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import dynamic from "next/dynamic";
const EntryCodeMirrorEditor = dynamic(
  () => import("./entry-codemirror-editor").then((m) => m.EntryCodeMirrorEditor),
  {
    ssr: false,
    loading: () => (
      <div className="min-h-[300px] rounded-xl border border-border bg-card flex items-center justify-center p-8">
        <div className="size-6 animate-spin rounded-full border-2 border-border border-t-primary" />
      </div>
    ),
  }
);
import { AiRefineButton } from "../ai/ai-refine-button";
import { StructuredDataEditor } from "./structured-data-editor";
import { StructuredDataView } from "./structured-data-view";
import { AnalysisPanel } from "./analysis-panel";
import { AttachmentsPanel } from "./attachments-panel";
import { CommentsPanel } from "./comments-panel";
import { MarkdownView } from "./markdown-view";
import { AiAssistPanel } from "../ai/ai-assist-panel";
import { EntryTagList } from "../ui/entry-tag";
import { cn, edgeEndpointTitle, entryTitle } from "@/lib/utils";
import { RELATION_STYLES } from "../graph/relation-item";
import { useSchema, useSchemas } from "@/lib/api/hooks";
import { Entry } from "@/lib/api";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useState } from "react";
import { Eye, Pencil } from "lucide-react";

interface EntryContentProps {
  id: string;
  entry: Entry;
  isEditing: boolean;
  editedText: string;
  setEditedText: (text: string) => void;
  editedType: string;
  setEditedType: (type: string) => void;
  editedData: Record<string, unknown> | null;
  setEditedData: (data: Record<string, unknown> | null) => void;
  isDataValid: boolean;
  setIsDataValid: (valid: boolean) => void;
  edges: any[] | undefined;
  isMobile: boolean;
  onSave?: () => void;
  onAddComment?: (comment: string, quote: string) => Promise<void>;
}

export function EntryContent({
  id,
  entry,
  isEditing,
  editedText,
  setEditedText,
  editedType,
  setEditedType,
  editedData,
  setEditedData,
  isDataValid,
  setIsDataValid,
  edges,
  isMobile,
  onSave,
  onAddComment,
}: EntryContentProps) {
  const { data: schemas } = useSchemas();
  const { data: schema } = useSchema(editedType);
  const [mobilePreview, setMobilePreview] = useState(false);

  return (
    <div className="flex h-full flex-col overflow-y-auto px-5 md:px-10 py-8 md:py-12">
      <div className="mx-auto w-full max-w-3xl space-y-16">
        {isEditing ? (
          <div className="space-y-6 animate-in fade-in slide-in-from-top-2 duration-300">
            <div className="space-y-2">
              <div className="flex items-center justify-between gap-3">
                <Label className="text-sm font-semibold">Content</Label>
                <div className="flex items-center gap-2">
                  {isMobile && (
                    <button
                      type="button"
                      onClick={() => setMobilePreview((v) => !v)}
                      className="flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
                    >
                      {mobilePreview ? (
                        <>
                          <Pencil className="size-3.5" /> Edit
                        </>
                      ) : (
                        <>
                          <Eye className="size-3.5" /> Preview
                        </>
                      )}
                    </button>
                  )}
                  <AiRefineButton content={editedText} onAccept={setEditedText} />
                </div>
              </div>
              <p className="text-xs text-muted-foreground">
                Markdown supported. Select text for AI refine or an inline comment.
              </p>

              {isMobile && mobilePreview ? (
                <div className="rounded-xl border border-border bg-card p-4">
                  <MarkdownView content={editedText || "*Empty note*"} />
                </div>
              ) : (
                <EntryCodeMirrorEditor
                  id={id}
                  value={editedText}
                  onChange={setEditedText}
                  onSave={onSave}
                  enableCollab={true}
                  onAddComment={onAddComment}
                />
              )}
            </div>

            <div className="space-y-3 border-t border-border pt-6">
              <div className="space-y-1.5">
                <Label className="text-sm font-semibold">Content type</Label>
                <p className="text-xs text-muted-foreground">
                  Optional — attach a schema for structured fields alongside the text.
                </p>
              </div>
              <Select
                value={editedType || "__none__"}
                onValueChange={(value) => {
                  const next = value === "__none__" ? "" : value;
                  setEditedType(next);
                  if (next && !editedData) setEditedData({});
                }}
              >
                <SelectTrigger className="w-full max-w-xs">
                  <SelectValue placeholder="— Untyped —" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__none__">— Untyped —</SelectItem>
                  {schemas?.map((s) => (
                    <SelectItem key={s.type_name} value={s.type_name}>
                      {s.type_name} {s.title ? `(${s.title})` : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              {editedType && schema && (
                <div className="space-y-3 pt-2 animate-in fade-in duration-300">
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
            {/* Title */}
            <h1 className="font-serif text-3xl md:text-4xl font-bold tracking-tight text-foreground">
              {entryTitle(entry, 200)}
            </h1>

            {/* Image preview */}
            {entry.metadata.source_type === "image" &&
              !!entry.metadata.source_file && (
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
              <div className="flex items-center justify-end mb-4 gap-3 flex-wrap">
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

              <MarkdownView content={entry.text} onAddComment={onAddComment} />
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

            {entry.type === "cms_page" && (
              <div className="space-y-4 animate-in fade-in slide-in-from-bottom-4 duration-700 delay-300">
                <div className="flex items-center justify-between gap-3">
                  <span className="font-mono text-xs font-black uppercase tracking-[3px] text-primary/80">
                    Rendered Page
                  </span>
                  <Link
                    href={`/cms/${encodeURIComponent(id)}`}
                    target="_blank"
                    className="font-mono text-[10px] font-black uppercase tracking-[2px] text-primary hover:text-primary/80"
                  >
                    Open HTML
                  </Link>
                </div>
                <div className="overflow-hidden rounded-2xl border border-border bg-card shadow-sm">
                  <iframe
                    title={`Rendered CMS page ${id}`}
                    src={`/cms/${encodeURIComponent(id)}`}
                    className="h-[560px] w-full bg-background"
                  />
                </div>
              </div>
            )}

            {/* Analysis */}
            <AnalysisPanel entry={entry} edges={edges} />

            {/* Connected Connections (on mobile only, since right panel is hidden) */}
            {isMobile && edges && edges.length > 0 && (
              <div className="space-y-4 animate-in fade-in slide-in-from-bottom-4 duration-700">
                <div className="flex items-center gap-2">
                  <GitBranch className="size-3.5 text-primary/60" />
                  <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    Connections ({edges.length})
                  </h2>
                </div>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  {edges.map((edge) => {
                    const targetId =
                      edge.source_id === id ? edge.target_id : edge.source_id;
                    const isManual = edge.edge_type === "manual";

                    return (
                      <Link
                        key={edge.id}
                        href={`/entries/${encodeURIComponent(targetId)}`}
                        className={cn(
                          "group relative flex flex-col gap-1 border p-4 hover:border-primary/40 hover:bg-primary/5 transition-all duration-300 rounded-xl bg-card",
                          isManual ? "border-primary/20" : "border-border",
                        )}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-xs font-semibold text-foreground group-hover:text-primary transition-colors truncate">
                            {edgeEndpointTitle(
                              targetId,
                              edge.source_id === id ? edge.target_title : edge.source_title,
                            )}
                          </span>
                          <Badge
                            variant="outline"
                            className={cn(
                              "h-4 px-1.5 font-mono text-[8px] uppercase tracking-tighter leading-none border-primary/20 text-primary/80 bg-primary/5",
                              RELATION_STYLES[
                                edge.relationship.toLowerCase()
                              ] || "",
                            )}
                          >
                            {edge.relationship.replace(/_/g, " ")}
                          </Badge>
                        </div>
                        <div className="flex items-center justify-between text-[9px] font-mono text-muted-foreground/60">
                          <span>
                            {isManual
                              ? "Manual Connection"
                              : "Similarity Connection"}
                          </span>
                          {edge.edge_type === "similarity" && (
                            <span className="opacity-80">
                              w = {edge.weight.toFixed(2)}
                            </span>
                          )}
                        </div>
                      </Link>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Attachments */}
            <AttachmentsPanel itemId={id} />

            {/* Comments & Discussion */}
            <CommentsPanel itemId={id} entryTitle={entry.id} />

            {/* Metadata */}
            {(() => {
              const visibleMetadata = Object.entries(entry.metadata).filter(
                ([key, value]) =>
                  key !== "source_type" &&
                  key !== "source_file" &&
                  key !== "projection" &&
                  typeof value !== "object",
              );
              if (visibleMetadata.length === 0) return null;
              return (
                <div>
                  <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground mb-4">
                    Properties
                  </h2>
                  <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                    {visibleMetadata.map(([key, value]) => (
                      <div
                        key={key}
                        className="flex flex-col gap-1 border border-border bg-card p-3 hover:border-border/80 transition-colors"
                      >
                        <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                          {key}
                        </span>
                        <div className="text-sm font-medium truncate text-foreground">
                          {key === "tags" && typeof value === "string" ? (
                            <EntryTagList
                              tags={value
                                .split(",")
                                .map((t) => t.trim())
                                .filter(Boolean)}
                            />
                          ) : (
                            String(value)
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              );
            })()}
          </div>
        )}
      </div>
    </div>
  );
}
