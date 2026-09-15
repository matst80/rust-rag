"use client";

import { useState } from "react";
import Link from "next/link";
import {
  ArrowLeft,
  Pencil,
  Save,
  Copy,
  Check,
  Terminal,
  Clock,
  History,
  Share2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ComboButton } from "@/components/ui/combo-button";
import { EntryTag } from "../ui/entry-tag";
import { WikiPathPicker } from "./wiki-path-picker";
import { ShareModal } from "./share-modal";
import { cn, formatRelativeTime } from "@/lib/utils";
import { toast } from "sonner";

import { Entry } from "@/lib/api";

interface EntryHeaderProps {
  entry: Entry;
  isEditing: boolean;
  isDataValid: boolean;
  isDirty?: boolean;
  onSave: () => Promise<void>;
  onStartEdit: () => void;
  onCancelEdit: () => void;
  onDelete: () => Promise<void>;
}

export function EntryHeader({
  entry,
  isEditing,
  isDataValid,
  isDirty,
  onSave,
  onStartEdit,
  onCancelEdit,
  onDelete,
}: EntryHeaderProps) {
  const [idCopied, setIdCopied] = useState(false);
  const [isShareOpen, setIsShareOpen] = useState(false);

  const handleCopyId = async () => {
    try {
      await navigator.clipboard.writeText(entry.id);
      setIdCopied(true);
      setTimeout(() => setIdCopied(false), 1500);
    } catch {
      toast.error("Copy failed");
    }
  };

  return (
    <div
      className={cn(
        "flex h-12 shrink-0 items-center justify-between border-b px-4 bg-background transition-colors",
        isEditing ? "border-primary/30 bg-primary/[0.02]" : "border-border",
      )}
    >
      <div className="flex items-center gap-3 min-w-0">
        <Button
          variant="ghost"
          size="icon"
          className="size-8 shrink-0"
          onClick={(e) => {
            if (isDirty) {
              e.preventDefault();
              if (window.confirm("Discard unsaved changes?")) {
                onCancelEdit();
                window.location.href = "/entries";
              }
            }
          }}
          asChild
        >
          <Link href="/entries">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div className="flex flex-col min-w-0">
          {/*<h1 className="font-mono text-xs font-black uppercase tracking-[2px] text-foreground leading-none">
            Fragment
          </h1>*/}
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
            <div className="hidden md:flex gap-2">
              <EntryTag label={entry.source_id} icon={false} className="h-6" />

              {entry.type && (
                <div className="h-6 px-2 font-mono text-[10px] uppercase tracking-wider flex items-center gap-1.5 rounded border border-primary/20 bg-primary/5 text-primary shadow-[0_0_10px_rgba(var(--primary-rgb),0.05)]">
                  <Terminal className="size-3" />
                  {entry.type}
                </div>
              )}

              <WikiPathPicker entry={entry} />
            </div>
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
        {isEditing && (
          <span
            className={cn(
              "text-xs font-medium mr-1 hidden sm:inline transition-opacity",
              isDirty ? "text-primary opacity-100" : "text-muted-foreground opacity-70",
            )}
          >
            {isDirty ? "Unsaved changes" : "No changes"}
          </span>
        )}
        {!isEditing && (
          <Button
            variant="ghost"
            size="sm"
            className="font-mono text-[10px] uppercase tracking-[1.5px] h-8 text-muted-foreground hover:text-foreground"
            onClick={() => setIsShareOpen(true)}
            title="Share entry publicly"
          >
            <Share2 className="size-3.5 mr-1.5" />
            Share
          </Button>
        )}
        <Button
          variant={isEditing ? "default" : "outline"}
          size="sm"
          className="h-8"
          onClick={isEditing ? onSave : onStartEdit}
          disabled={isEditing && !isDataValid}
        >
          {isEditing ? (
            <>
              <Save className="size-3.5 mr-1.5" />
              Save
            </>
          ) : (
            <>
              <Pencil className="size-3.5 mr-1.5" />
              Edit
            </>
          )}
        </Button>
        {isEditing && (
          <Button variant="ghost" size="sm" className="h-8" onClick={onCancelEdit}>
            Cancel
          </Button>
        )}
        <ComboButton onConfirm={onDelete} className="size-8" />
      </div>

      <ShareModal
        itemId={entry.id}
        isOpen={isShareOpen}
        onClose={() => setIsShareOpen(false)}
      />
    </div>
  );
}
