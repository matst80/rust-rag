"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Plus, X, Save, ArrowLeft, Eye, Tag, FolderTree, Database } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import dynamic from "next/dynamic";
const EntryCodeMirrorEditor = dynamic(
  () => import("./entry-codemirror-editor").then((m) => m.EntryCodeMirrorEditor),
  {
    ssr: false,
    loading: () => (
      <div className="min-h-[220px] rounded-xl border border-border bg-card flex items-center justify-center p-8">
        <div className="size-6 animate-spin rounded-full border-2 border-border border-t-primary" />
      </div>
    ),
  }
);
import { MarkdownView } from "./markdown-view";
import { Label } from "@/components/ui/label";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import {
  useCreateItem,
  useUpdateItem,
  type Entry,
  type EntryMetadata,
} from "@/lib/api";
import { useSchemas } from "@/lib/api/hooks";
import { useSWRConfig } from "swr";
import Link from "next/link";
import { StructuredDataEditor } from "./structured-data-editor";
import { AiRefineButton } from "../ai/ai-refine-button";
import { WhisperTranscribe } from "./whisper-transcribe";
import { WikiPathTreePicker } from "./wiki-path-tree-picker";

interface EntryFormProps {
  entry?: Entry;
  mode?: "create" | "edit";
  initialData?: Partial<Entry> & { type_name?: string };
  onSuccess?: (entry: Entry) => void;
  onCancel?: () => void;
  minimal?: boolean;
}

export function EntryForm({
  entry,
  mode: providedMode,
  initialData,
  onSuccess,
  onCancel,
  minimal = false,
}: EntryFormProps) {
  const router = useRouter();
  const { mutate } = useSWRConfig();

  const mode = providedMode || (entry?.id ? "edit" : "create");

  const { trigger: createItem, isMutating: isCreating } = useCreateItem();
  const { trigger: updateItem, isMutating: isUpdating } = useUpdateItem(
    entry?.id ?? initialData?.id ?? "",
  );

  const [id, setId] = useState(entry?.id ?? initialData?.id ?? "");
  const [text, setText] = useState(entry?.text ?? initialData?.text ?? "");
  const [sourceId, setSourceId] = useState(
    entry?.source_id ?? initialData?.source_id ?? "knowledge",
  );
  const [path, setPath] = useState(entry?.path ?? initialData?.path ?? "");
  const [metadata, setMetadata] = useState<EntryMetadata>(
    entry?.metadata ?? initialData?.metadata ?? {},
  );
  const [newMetaKey, setNewMetaKey] = useState("");
  const [newMetaValue, setNewMetaValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const [typeName, setTypeName] = useState<string>(
    entry?.type ?? initialData?.type_name ?? "",
  );
  const [data, setData] = useState<any>(entry?.data ?? initialData?.data ?? {});

  // Sync state when entry loads asynchronously
  useEffect(() => {
    if (entry) {
      setId(entry.id);
      setText(entry.text);
      setSourceId(entry.source_id);
      setPath(entry.path ?? "");
      setMetadata(entry.metadata ?? {});
      setTypeName(entry.type ?? "");
      setData(entry.data ?? {});
    }
  }, [entry]);

  const { data: schemas } = useSchemas();
  const isMutating = isCreating || isUpdating;

  const currentSchema = schemas?.find(
    (s) => s.type_name === typeName,
  )?.json_schema;

  const handleAddMetadata = () => {
    if (!newMetaKey.trim()) return;
    setMetadata((prev) => ({
      ...prev,
      [newMetaKey.trim()]: newMetaValue.trim(),
    }));
    setNewMetaKey("");
    setNewMetaValue("");
  };

  const handleRemoveMetadata = (key: string) => {
    setMetadata((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!text.trim() || !sourceId.trim()) {
      setError("Text and source are required");
      return;
    }

    try {
      const trimmedPath = path.trim();
      const typedFields = typeName
        ? { type: typeName, data: data }
        : { type: null, data: null };

      let result: Entry;
      if (mode === "create") {
        result = await createItem({
          ...(id.trim() && { id: id.trim() }),
          text: text.trim(),
          source_id: sourceId.trim(),
          metadata,
          ...(trimmedPath && { path: trimmedPath }),
          ...(typeName && typedFields),
        });
      } else {
        result = await updateItem({
          text: text.trim(),
          source_id: sourceId.trim(),
          metadata,
          path: trimmedPath,
          ...typedFields,
        });
      }
      mutate("items");
      mutate("categories");

      if (onSuccess) {
        onSuccess(result);
      } else {
        router.push("/entries");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save entry");
    }
  };

  return (
    <form
      onSubmit={handleSubmit}
      className={cn("mx-auto w-full", !minimal && "max-w-2xl p-4")}
    >
      {!minimal && (
        <div className="mb-6 flex items-center gap-4">
          <Button variant="ghost" size="icon" asChild>
            <Link href="/entries">
              <ArrowLeft className="size-4" />
            </Link>
          </Button>
          <h1 className="text-2xl font-bold">
            {mode === "create" ? "Create Entry" : "Edit Entry"}
          </h1>
        </div>
      )}

      {error && (
        <div className="mb-4 rounded-md bg-destructive/10 p-3 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="flex flex-col gap-4">
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <Label htmlFor="text" className="flex items-center gap-1.5">
              Content
            </Label>
            <div className="flex items-center gap-2">
              <WhisperTranscribe
                onTranscription={(transcription) => {
                  setText((prev) =>
                    prev ? `${prev}\n${transcription}` : transcription,
                  );
                }}
              />
              <AiRefineButton content={text} onAccept={setText} />
            </div>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 items-start">
            <EntryCodeMirrorEditor
              id={id || "new"}
              value={text}
              onChange={setText}
              className="min-h-[220px]"
            />
            <div className="hidden md:flex flex-col rounded-xl border border-border bg-card overflow-hidden shadow-sm max-h-[650px]">
              <div className="flex items-center gap-1.5 border-b border-border px-3 py-1.5 bg-muted/20 text-[10px] font-mono font-black uppercase tracking-widest text-muted-foreground">
                <Eye className="size-3" />
                Preview
              </div>
              <div className="flex-1 overflow-auto p-4">
                {text.trim() ? (
                  <MarkdownView content={text} className="prose-sm" />
                ) : (
                  <p className="text-sm text-muted-foreground italic">
                    Preview appears here as you type…
                  </p>
                )}
              </div>
            </div>
          </div>
        </div>

        <details className="group rounded-xl border border-border bg-card/50 overflow-hidden">
          <summary className="cursor-pointer select-none list-none flex items-center justify-between px-4 py-3 text-sm font-medium hover:bg-muted/30 transition-colors">
            <span className="flex items-center gap-2">
              <Database className="size-3.5 text-muted-foreground" />
              Details
              <span className="text-xs font-normal text-muted-foreground">
                (id, source, path, type)
              </span>
            </span>
            <span className="text-muted-foreground text-xs group-open:rotate-180 transition-transform">
              ▾
            </span>
          </summary>

          <div className="flex flex-col gap-4 px-4 pb-4 pt-1 border-t border-border">
            <div className="flex flex-col gap-2">
              <Label htmlFor="id">ID {mode === "create" ? "(optional)" : ""}</Label>
              <Input
                id="id"
                value={id}
                onChange={(e) => setId(e.target.value)}
                placeholder={
                  mode === "create"
                    ? "Leave blank to auto-generate"
                    : "doc-knowledge-1"
                }
                disabled={mode === "edit"}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="source">Source</Label>
              <Input
                id="source"
                value={sourceId}
                onChange={(e) => setSourceId(e.target.value)}
                placeholder="knowledge"
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="path" className="flex items-center gap-1.5">
                <FolderTree className="size-3.5" />
                Wiki path (optional)
              </Label>
              <WikiPathTreePicker sourceId={sourceId || "knowledge"} value={path} onChange={setPath} />
              <p className="text-xs text-muted-foreground">
                Pick an existing folder or type a new slash-separated path, e.g.{" "}
                <code>engineering/runbooks/db</code>. Used for tree navigation under
                the source.
              </p>
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="type">Type (optional)</Label>
              <select
                id="type"
                value={typeName}
                onChange={(e) => {
                  setTypeName(e.target.value);
                  // Initialize with empty object if switching to a type
                  if (e.target.value && !data) setData({});
                }}
                className="flex h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm"
              >
                <option value="">— Untyped —</option>
                {schemas?.map((s) => (
                  <option key={s.type_name} value={s.type_name}>
                    {s.type_name}
                    {s.title ? ` — ${s.title}` : ""}
                  </option>
                ))}
              </select>
              <p className="text-xs text-muted-foreground">
                Pick a registered schema to attach structured data. Manage schemas
                at{" "}
                <Link href="/schemas" className="underline">
                  /schemas
                </Link>
                .
              </p>
            </div>
          </div>
        </details>

        {typeName && currentSchema && (
          <div className="flex flex-col gap-2 pt-2 border-t mt-2">
            {/*<Label>Structured Data ({typeName})</Label>*/}
            <StructuredDataEditor
              schema={currentSchema}
              value={data}
              onChange={setData}
              typeName={typeName}
              content={text}
            />
          </div>
        )}
      </div>

      <Card className="mt-4">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Tag className="size-4 text-muted-foreground" />
            Metadata
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {Object.keys(metadata).length > 0 && (
            <div className="flex flex-wrap gap-2">
              {Object.entries(metadata).map(([key, value]) => (
                <Badge key={key} variant="secondary" className="gap-1 pr-1">
                  {key}: {String(value)}
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    className="size-4 hover:bg-destructive hover:text-destructive-foreground"
                    onClick={() => handleRemoveMetadata(key)}
                  >
                    <X className="size-3" />
                  </Button>
                </Badge>
              ))}
            </div>
          )}
          <div className="flex gap-2">
            <Input
              placeholder="Key"
              value={newMetaKey}
              onChange={(e) => setNewMetaKey(e.target.value)}
              className="flex-1"
            />
            <Input
              placeholder="Value"
              value={newMetaValue}
              onChange={(e) => setNewMetaValue(e.target.value)}
              className="flex-1"
            />
            <Button type="button" variant="outline" onClick={handleAddMetadata}>
              <Plus className="size-4" />
            </Button>
          </div>
        </CardContent>
      </Card>

      <div className="mt-6 flex justify-end gap-2 sticky bottom-0 py-3 bg-gradient-to-t from-background via-background/95 to-transparent">
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            if (onCancel) onCancel();
            else router.push("/entries");
          }}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={isMutating}>
          <Save className="size-4" />
          {isMutating ? "Saving..." : "Save Entry"}
        </Button>
      </div>
    </form>
  );
}
