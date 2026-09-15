"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  useItem,
  useDeleteItem,
  useEdgesForItem,
  useGraphStatus,
  useUpdateItem,
  useSendMessage,
} from "@/lib/api";
import { useSWRConfig } from "swr";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { useIsMobile } from "@/hooks/use-mobile";
import { toast } from "sonner";
import { EntryHeader } from "./entry-header";
import { EntryContent } from "./entry-content";
import { EntryGraphPanel } from "./entry-graph-panel";
import { RelatedPanel } from "./related-panel";
import { MarkdownView } from "./markdown-view";
import { CommentsPanel } from "./comments-panel";
import { Eye, MessageSquare, GitBranch, Network } from "lucide-react";

interface EntryDetailProps {
  id: string;
}

export function EntryDetail({ id }: EntryDetailProps) {
  const router = useRouter();
  const { mutate } = useSWRConfig();
  const isMobile = useIsMobile();
  const { data: entry, isLoading, error } = useItem(id);
  const { data: graphStatus } = useGraphStatus();
  const { data: edges } = useEdgesForItem(graphStatus?.enabled ? id : null);
  const { trigger: deleteItem } = useDeleteItem();
  const { trigger: updateItem } = useUpdateItem(id);
  const { trigger: sendMessage } = useSendMessage();

  const [isEditing, setIsEditing] = useState(false);
  const [editedText, setEditedText] = useState("");
  const [editedType, setEditedType] = useState<string>("");
  const [editedData, setEditedData] = useState<Record<string, unknown> | null>(
    null,
  );
  const [isDataValid, setIsDataValid] = useState(true);
  const [editPanelTab, setEditPanelTab] = useState<"preview" | "comments">("preview");
  const [sidePanelTab, setSidePanelTab] = useState<"related" | "graph">("related");

  useEffect(() => {
    if (entry) {
      setEditedText(entry.text);
      setEditedType(entry.type ?? "");
      setEditedData(entry.data ?? null);
    }
  }, [entry]);

  const isDirty =
    isEditing &&
    !!entry &&
    (editedText !== entry.text ||
      editedType !== (entry.type ?? "") ||
      JSON.stringify(editedData) !== JSON.stringify(entry.data ?? null));

  // Warn on tab close / reload with unsaved edits.
  useEffect(() => {
    if (!isDirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [isDirty]);

  const handleCancelEdit = () => {
    if (isDirty && !window.confirm("Discard unsaved changes?")) return;
    if (entry) {
      setEditedText(entry.text);
      setEditedType(entry.type ?? "");
      setEditedData(entry.data ?? null);
    }
    setIsEditing(false);
  };

  const handleAddInlineComment = async (comment: string, quote: string) => {
    try {
      await sendMessage({
        channel: `entry:${id}`,
        text: comment,
        kind: "text",
        metadata: {
          item_id: id,
          entry_title: entry?.id,
          quote,
        },
      });
      mutate(`entry:${id}`);
      mutate(["messages", `entry:${id}`]);
      toast.success("Inline comment posted");
      if (isEditing) {
        setEditPanelTab("comments");
      }
    } catch {
      toast.error("Failed to post inline comment");
    }
  };

  // Global Cmd+S / Ctrl+S shortcut to trigger save when editing
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        if (isEditing && isDataValid) {
          e.preventDefault();
          handleSave();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isEditing, isDataValid, editedText, editedType, editedData]);

  const handleDelete = async () => {
    await deleteItem(id);
    mutate("items");
    mutate("categories");
    router.push("/entries");
  };

  const handleSave = async () => {
    try {
      await updateItem({
        text: editedText,
        source_id: entry?.source_id ?? "knowledge",
        metadata: entry?.metadata ?? {},
        path: entry?.path ?? undefined,
        type: editedType || null,
        data: editedType ? editedData : null,
      });
      mutate(["items", id]);
      setIsEditing(false);
      toast.success("Entry updated");
    } catch {
      toast.error("Failed to update entry");
    }
  };

  if (isLoading) {
    return (
      <div className="flex h-[calc(100vh-3rem)] items-center justify-center gap-3">
        <div className="size-6 animate-spin border-2 border-border border-t-primary" />
        <span className="font-mono text-xs uppercase tracking-widest text-muted-foreground animate-pulse">
          Loading...
        </span>
      </div>
    );
  }

  if (error || !entry) {
    return (
      <div className="flex h-[calc(100vh-3rem)] flex-col items-center justify-center text-center gap-4">
        <p className="font-mono text-xs uppercase tracking-widest text-muted-foreground">
          Entry not found
        </p>
        <Button asChild variant="outline" size="sm">
          <Link href="/entries">Back to Entries</Link>
        </Button>
      </div>
    );
  }

  // ── Mobile layout (no graph) ───────────────────────────
  if (isMobile) {
    return (
      <div
        className="flex flex-col overflow-hidden bg-background"
        style={{ height: "calc(100vh - 3rem)" }}
      >
        <EntryHeader
          entry={entry}
          isEditing={isEditing}
          isDataValid={isDataValid}
          onSave={handleSave}
          onStartEdit={() => setIsEditing(true)}
          onCancelEdit={handleCancelEdit}
          onDelete={handleDelete}
          isDirty={isDirty}
        />
        <EntryContent
          id={id}
          entry={entry}
          isEditing={isEditing}
          editedText={editedText}
          setEditedText={setEditedText}
          editedType={editedType}
          setEditedType={setEditedType}
          editedData={editedData}
          setEditedData={setEditedData}
          isDataValid={isDataValid}
          setIsDataValid={setIsDataValid}
          edges={edges}
          isMobile={isMobile}
          onSave={handleSave}
          onAddComment={handleAddInlineComment}
        />
      </div>
    );
  }

  // ── Desktop layout (resizable split) ──────────────────
  return (
    <div className="flex h-[calc(100vh-3rem)] flex-col overflow-hidden bg-background">
      <EntryHeader
        entry={entry}
        isEditing={isEditing}
        isDataValid={isDataValid}
        onSave={handleSave}
        onStartEdit={() => setIsEditing(true)}
        onCancelEdit={handleCancelEdit}
        onDelete={handleDelete}
        isDirty={isDirty}
      />
      <ResizablePanelGroup
        direction="horizontal"
        className="flex-1 overflow-hidden"
      >
        <ResizablePanel defaultSize={50} minSize={30}>
          <EntryContent
            id={id}
            entry={entry}
            isEditing={isEditing}
            editedText={editedText}
            setEditedText={setEditedText}
            editedType={editedType}
            setEditedType={setEditedType}
            editedData={editedData}
            setEditedData={setEditedData}
            isDataValid={isDataValid}
            setIsDataValid={setIsDataValid}
            edges={edges}
            isMobile={isMobile}
            onSave={handleSave}
            onAddComment={handleAddInlineComment}
          />
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={50} minSize={25}>
          {isEditing ? (
            <div className="flex h-full flex-col overflow-hidden bg-muted/10 border-l border-border">
              <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2 bg-muted/20">
                <button
                  type="button"
                  onClick={() => setEditPanelTab("preview")}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                    editPanelTab === "preview"
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <Eye className="size-3.5" />
                  Preview
                </button>
                <button
                  type="button"
                  onClick={() => setEditPanelTab("comments")}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                    editPanelTab === "comments"
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <MessageSquare className="size-3.5" />
                  Comments
                </button>
              </div>
              <div className="flex-1 overflow-y-auto px-6 py-6">
                {editPanelTab === "preview" ? (
                  <MarkdownView
                    content={editedText || "*Empty note*"}
                    onAddComment={handleAddInlineComment}
                  />
                ) : (
                  <CommentsPanel itemId={id} entryTitle={entry?.id} compact />
                )}
              </div>
            </div>
          ) : (
            <div className="flex h-full flex-col overflow-hidden bg-background border-l border-border">
              <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2 bg-muted/20">
                <button
                  type="button"
                  onClick={() => setSidePanelTab("related")}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                    sidePanelTab === "related"
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <Network className="size-3.5" />
                  Related
                </button>
                <button
                  type="button"
                  onClick={() => setSidePanelTab("graph")}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                    sidePanelTab === "graph"
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <GitBranch className="size-3.5" />
                  Graph
                </button>
              </div>
              <div className="flex-1 overflow-hidden">
                {sidePanelTab === "related" ? (
                  <RelatedPanel id={id} edges={edges} />
                ) : (
                  <EntryGraphPanel id={id} edges={edges} />
                )}
              </div>
            </div>
          )}
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
