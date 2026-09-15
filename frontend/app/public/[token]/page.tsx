"use client";

import { use, useState } from "react";
import Link from "next/link";
import { usePublicEntry } from "@/lib/api";
import { MarkdownView } from "@/components/entries/markdown-view";
import { formatRelativeTime } from "@/lib/utils";
import {
  Globe,
  Clock,
  History,
  Copy,
  Check,
  FileText,
  AlertCircle,
  Loader2,
  Terminal,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";

export default function PublicSharePage({
  params,
}: {
  params: Promise<{ token: string }>;
}) {
  const { token } = use(params);
  const { data: entry, error, isLoading } = usePublicEntry(token);
  const [copied, setCopied] = useState(false);

  const title =
    (entry?.metadata?.title as string | undefined) ||
    entry?.path?.split("/").pop() ||
    "Untitled Document";

  const handleCopyContent = async () => {
    if (!entry?.text) return;
    try {
      await navigator.clipboard.writeText(entry.text);
      setCopied(true);
      toast.success("Document content copied to clipboard");
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Failed to copy content");
    }
  };

  if (isLoading) {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center bg-background text-foreground">
        <Loader2 className="size-8 animate-spin text-muted-foreground mb-4" />
        <p className="font-mono text-xs text-muted-foreground uppercase tracking-wider">
          Loading shared document…
        </p>
      </div>
    );
  }

  if (error || !entry) {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center bg-background text-foreground px-4 text-center">
        <div className="size-12 rounded-full bg-destructive/10 text-destructive flex items-center justify-center mb-4">
          <AlertCircle className="size-6" />
        </div>
        <h1 className="text-xl font-bold mb-2">Document Unavailable</h1>
        <p className="text-sm text-muted-foreground max-w-md mb-6">
          This share link may have expired, been revoked, or the document no longer exists.
        </p>
        <Button variant="outline" asChild className="font-mono text-xs">
          <Link href="/">Return to Home</Link>
        </Button>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background text-foreground flex flex-col">
      {/* Top minimal public header */}
      <header className="h-12 border-b border-border px-4 md:px-8 flex items-center justify-between shrink-0 bg-background/80 backdrop-blur sticky top-0 z-10">
        <div className="flex items-center gap-2 text-xs font-mono text-muted-foreground">
          <Globe className="size-3.5 text-primary" />
          <span className="uppercase tracking-wider font-semibold text-foreground">
            Shared Document
          </span>
          <span className="opacity-40">/</span>
          <span className="truncate max-w-[200px] md:max-w-xs">{title}</span>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={handleCopyContent}
            className="h-8 font-mono text-[11px] gap-1.5"
            title="Copy markdown text"
          >
            {copied ? (
              <Check className="size-3.5 text-emerald-500" />
            ) : (
              <Copy className="size-3.5" />
            )}
            <span>{copied ? "Copied" : "Copy Markdown"}</span>
          </Button>
        </div>
      </header>

      {/* Main document container */}
      <main className="flex-1 max-w-4xl w-full mx-auto px-4 md:px-8 py-8 md:py-12">
        <article className="space-y-6">
          {/* Metadata banner */}
          <div className="border-b border-border pb-6 space-y-3">
            <h1 className="text-2xl md:text-3xl font-bold tracking-tight text-foreground">
              {title}
            </h1>

            <div className="flex flex-wrap items-center gap-3 text-xs font-mono text-muted-foreground">
              <div className="flex items-center gap-1.5 bg-muted/40 px-2 py-0.5 rounded border border-border">
                <FileText className="size-3 opacity-60" />
                <span>{entry.source_id}</span>
              </div>

              {entry.type && (
                <div className="flex items-center gap-1.5 bg-primary/5 text-primary border border-primary/20 px-2 py-0.5 rounded">
                  <Terminal className="size-3" />
                  <span>{entry.type}</span>
                </div>
              )}

              {entry.path && (
                <span className="opacity-75">path: {entry.path}</span>
              )}

              <div
                className="flex items-center gap-1.5 opacity-60 ml-auto"
                title={`Created: ${new Date(entry.created_at).toLocaleString()}`}
              >
                <Clock className="size-3" />
                <span>{formatRelativeTime(entry.created_at)}</span>
              </div>

              {entry.updated_at > entry.created_at + 1000 && (
                <div
                  className="flex items-center gap-1.5 opacity-60"
                  title={`Modified: ${new Date(entry.updated_at).toLocaleString()}`}
                >
                  <History className="size-3" />
                  <span>updated {formatRelativeTime(entry.updated_at)}</span>
                </div>
              )}
            </div>
          </div>

          {/* Rendered content */}
          <div className="pt-2">
            <MarkdownView content={entry.text} className="text-sm md:text-base leading-relaxed" />
          </div>
        </article>
      </main>

      {/* Subtle footer */}
      <footer className="border-t border-border py-4 px-4 text-center font-mono text-[11px] text-muted-foreground/60">
        Powered by rust-rag knowledge base
      </footer>
    </div>
  );
}
