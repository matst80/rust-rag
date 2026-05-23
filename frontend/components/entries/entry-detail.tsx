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

  const [isEditing, setIsEditing] = useState(false);
  const [editedText, setEditedText] = useState("");
  const [editedType, setEditedType] = useState<string>("");
  const [editedData, setEditedData] = useState<Record<string, unknown> | null>(
    null,
  );
  const [isDataValid, setIsDataValid] = useState(true);

  useEffect(() => {
    if (entry) {
      setEditedText(entry.text);
      setEditedType(entry.type ?? "");
      setEditedData(entry.data ?? null);
    }
  }, [entry]);

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
          onCancelEdit={() => setIsEditing(false)}
          onDelete={handleDelete}
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
        onCancelEdit={() => setIsEditing(false)}
        onDelete={handleDelete}
      />
      <ResizablePanelGroup
        direction="horizontal"
        className="flex-1 overflow-hidden"
      >
        <ResizablePanel defaultSize={60} minSize={30}>
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
          />
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={40} minSize={20}>
          <EntryGraphPanel id={id} edges={edges} />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
