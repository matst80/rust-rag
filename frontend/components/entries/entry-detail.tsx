"use client";

import { useState, useEffect, useRef } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { pushRecent } from "@/lib/favorites";
import { entryTitle } from "@/lib/utils";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  useItem,
  useDeleteItem,
  useEdgesForItem,
  useBacklinks,
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
import { BacklinksPanel } from "./backlinks-panel";
import { EntryCodePanel } from "./entry-code-panel";
import { MarkdownView } from "./markdown-view";
import { CommentsPanel } from "./comments-panel";
import { Eye, MessageSquare, GitBranch, Network, Code2, ArrowLeft } from "lucide-react";

interface EntryDetailProps {
  id: string;
}

export function EntryDetail({ id }: EntryDetailProps) {
  const router = useRouter();
  const searchParams = useSearchParams();
  const { mutate } = useSWRConfig();
  const isMobile = useIsMobile();
  const { data: entry, isLoading, error } = useItem(id);

  useEffect(() => { if (searchParams?.get("edit") === "true") setIsEditing(true); }, [searchParams]);

  // Track recently visited entries for the sidebar recents list.
  useEffect(() => {
    if (entry) pushRecent(entry.id, entryTitle(entry));
  }, [entry]);
  const { data: graphStatus } = useGraphStatus();
  const { data: edges } = useEdgesForItem(graphStatus?.enabled ? id : null);
  const { trigger: deleteItem } = useDeleteItem();
  const { trigger: updateItem } = useUpdateItem(id);
  const { trigger: sendMessage } = useSendMessage();

  const [isEditing, setIsEditing] = useState(searchParams?.get("edit") === "true");
  const [saveState, setSaveState] = useState("Saved");
  const saving = useRef(false);
  const initializedId = useRef<string | null>(null);
  const [savedDraft, setSavedDraft] = useState<string | null>(null);
  const [editedText, setEditedText] = useState("");
  const [editedType, setEditedType] = useState<string>("");
  const [editedData, setEditedData] = useState<Record<string, unknown> | null>(
    null,
  );
  const [isDataValid, setIsDataValid] = useState(true);
  const [editPanelTab, setEditPanelTab] = useState<"preview" | "comments">("preview");
  const [sidePanelTab, setSidePanelTab] = useState<
    "related" | "backlinks" | "graph" | "code"
  >(() => {
    const tab = searchParams?.get("tab");
    return tab === "graph" || tab === "code" || tab === "backlinks" ? tab : "related";
  });
  const { data: backlinksResp } = useBacklinks(id);
  const backlinkCount = (backlinksResp?.backlinks.length ?? 0)
    + (edges ?? []).filter((e) => e.edge_type === "manual" && e.source_id !== id).length;

  useEffect(() => {
    if (entry && (!isEditing || initializedId.current !== id)) {
      initializedId.current = id;
      setSavedDraft(null);
      setEditedText(entry.text);
      setEditedType(entry.type ?? "");
      setEditedData(entry.data ?? null);
    }
  }, [entry, isEditing, id]);

  const draft = JSON.stringify([editedText, editedType, editedData]);
  const latestDraft = useRef(draft);
  latestDraft.current = draft;
  const isDirty =
    isEditing &&
    !!entry &&
    draft !== (savedDraft ?? JSON.stringify([entry.text, entry.type ?? "", entry.data ?? null]));

  useEffect(() => {
    if (!isDirty || !isDataValid || saveState === "Save failed") return;
    const timer = setTimeout(() => { void handleSave(false); }, 1200);
    return () => clearTimeout(timer);
  }, [draft, isDirty, isDataValid, saveState]);

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

  const handleSave = async (finish = true) => {
    if (saving.current || !entry || !isDataValid) return;
    saving.current = true;
    const snapshot = draft;
    setSaveState("Saving…");
    try {
      const savedEntry = await updateItem({
        text: editedText,
        source_id: entry?.source_id ?? "knowledge",
        metadata: entry?.metadata ?? {},
        path: entry?.path ?? undefined,
        type: editedType || null,
        data: editedType ? editedData : null,
      });
      setSavedDraft(snapshot);
      setSaveState(`Saved · ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`);
      await mutate(["item", id], savedEntry, false);
      if (finish && latestDraft.current === snapshot) { setIsEditing(false); toast.success("Entry updated"); }
      void mutate(key => Array.isArray(key) && key[0] === "items");
    } catch {
      setSaveState("Save failed");
      toast.error("Failed to update entry — your draft is still here");
    } finally { saving.current = false; }
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

  // ── Mobile layout ───────────────────────────
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
          onSave={() => handleSave()}
          onStartEdit={() => setIsEditing(true)}
          onCancelEdit={handleCancelEdit}
          onDelete={handleDelete}
          saveState={saveState}
          isDirty={isDirty}
        />
        <div className="flex-1 overflow-y-auto">
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
          onSave={() => handleSave()}
          onAddComment={handleAddInlineComment}
        />
        {!isEditing && <div className="space-y-3 p-4">
          <details className="rounded-lg border p-3"><summary className="cursor-pointer">Backlinks · {backlinkCount}</summary><BacklinksPanel id={id} edges={edges} /></details>
          <details className="rounded-lg border p-3"><summary className="cursor-pointer">Graph</summary><div className="h-80"><EntryGraphPanel id={id} edges={edges} /></div></details>
        </div>}
        </div>
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
        onSave={() => handleSave()}
        onStartEdit={() => setIsEditing(true)}
        onCancelEdit={handleCancelEdit}
        onDelete={handleDelete}
        saveState={saveState}
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
            onSave={() => handleSave()}
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
                  onClick={() => setSidePanelTab("backlinks")}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                    sidePanelTab === "backlinks"
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <ArrowLeft className="size-3.5" />
                  Backlinks
                  {backlinkCount > 0 && (
                    <span className="rounded-full bg-primary/15 px-1.5 py-0.5 font-mono text-[10px] font-bold tabular-nums text-primary">
                      {backlinkCount}
                    </span>
                  )}
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
                <button
                  type="button"
                  onClick={() => setSidePanelTab("code")}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
                    sidePanelTab === "code"
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <Code2 className="size-3.5" />
                  Code
                </button>
              </div>
              <div className="flex-1 overflow-hidden">
                {sidePanelTab === "related" ? (
                  <RelatedPanel id={id} edges={edges} />
                ) : sidePanelTab === "backlinks" ? (
                  <BacklinksPanel id={id} edges={edges} />
                ) : sidePanelTab === "code" ? (
                  <EntryCodePanel entry={entry} />
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
