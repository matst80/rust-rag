"use client";

import { useItem } from "@/lib/api";
import { EntryForm } from "@/components/entries/entry-form";
import { Button } from "@/components/ui/button";
import Link from "next/link";

export function EntryEditClient({ id }: { id: string }) {
  const { data: entry, isLoading, error } = useItem(id);

  if (isLoading) {
    return (
      <div className="flex min-h-[calc(100vh-3.5rem)] items-center justify-center">
        <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    );
  }

  if (error || !entry) {
    return (
      <div className="flex min-h-[calc(100vh-3.5rem)] flex-col items-center justify-center gap-4 text-center">
        <p className="font-mono text-sm text-muted-foreground">
          {error?.message || "Entry not found"}
        </p>
        <Button asChild variant="outline" size="sm">
          <Link href="/entries">Back to Entries</Link>
        </Button>
      </div>
    );
  }

  return <EntryForm entry={entry} mode="edit" />;
}
