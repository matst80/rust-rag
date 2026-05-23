"use client";

import Link from "next/link";
import { GitBranch } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
import { AiRefineButton } from "../ai/ai-refine-button";
import { StructuredDataEditor } from "./structured-data-editor";
import { StructuredDataView } from "./structured-data-view";
import { AnalysisPanel } from "./analysis-panel";
import { AttachmentsPanel } from "./attachments-panel";
import { MarkdownView } from "./markdown-view";
import { AiAssistPanel } from "../ai/ai-assist-panel";
import { EntryTagList } from "../ui/entry-tag";
import { cn } from "@/lib/utils";
import { RELATION_STYLES } from "../graph/relation-item";
import { useSchema, useSchemas } from "@/lib/api/hooks";
import { Entry } from "@/lib/api";

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
}: EntryContentProps) {
  const { data: schemas } = useSchemas();
  const { data: schema } = useSchema(editedType);

  return (
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
                    setEditedType(e.target.value);
                    if (e.target.value && !editedData) setEditedData({});
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

              <MarkdownView content={entry.text} />
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
                          <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground group-hover:text-primary transition-colors truncate">
                            {targetId.substring(0, 20)}...
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
