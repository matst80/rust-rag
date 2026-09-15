"use client";

import Link from "next/link";
import { GitBranch } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { RELATION_STYLES } from "../graph/relation-item";
import { cn, edgeEndpointTitle } from "@/lib/utils";
import type { Edge } from "@/lib/api";

interface RelatedPanelProps {
  id: string;
  edges: Edge[] | undefined;
}

/** Content-focused list of an entry's manual links and semantic neighbors, titles first. */
export function RelatedPanel({ id, edges }: RelatedPanelProps) {
  if (!edges || edges.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-8 text-center gap-2">
        <GitBranch className="size-5 text-muted-foreground/30" />
        <p className="text-sm text-muted-foreground">No related entries yet</p>
      </div>
    );
  }

  const sorted = [...edges].sort((a, b) => {
    if (a.edge_type === "manual" && b.edge_type !== "manual") return -1;
    if (a.edge_type !== "manual" && b.edge_type === "manual") return 1;
    return a.sort_order.localeCompare(b.sort_order);
  });

  const manual = sorted.filter((e) => e.edge_type === "manual");
  const similar = sorted.filter((e) => e.edge_type !== "manual");

  return (
    <div className="flex h-full flex-col overflow-y-auto p-4 space-y-6">
      {manual.length > 0 && (
        <RelatedSection title="Linked" id={id} edges={manual} />
      )}
      {similar.length > 0 && (
        <RelatedSection title="Similar" id={id} edges={similar} muted />
      )}
    </div>
  );
}

function RelatedSection({
  title,
  id,
  edges,
  muted,
}: {
  title: string;
  id: string;
  edges: Edge[];
  muted?: boolean;
}) {
  return (
    <div className="space-y-2">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        {title}
      </h3>
      <div className="flex flex-col gap-1.5">
        {edges.map((edge) => {
          const targetId = edge.source_id === id ? edge.target_id : edge.source_id;
          const targetTitle = edge.source_id === id ? edge.target_title : edge.source_title;

          return (
            <Link
              key={edge.id}
              href={`/entries/${encodeURIComponent(targetId)}`}
              className={cn(
                "group flex flex-col gap-1 rounded-lg border p-3 transition-colors",
                muted
                  ? "border-border/60 hover:border-primary/30 hover:bg-primary/[0.03]"
                  : "border-border hover:border-primary/40 hover:bg-primary/5",
              )}
            >
              <span className="text-sm font-medium text-foreground group-hover:text-primary transition-colors truncate">
                {edgeEndpointTitle(targetId, targetTitle)}
              </span>
              <div className="flex items-center gap-2">
                <Badge
                  variant="outline"
                  className={cn(
                    "h-5 px-1.5 text-[10px] font-medium uppercase tracking-wide leading-none",
                    RELATION_STYLES[edge.relationship.toLowerCase()] || "",
                  )}
                >
                  {edge.relationship.replace(/_/g, " ")}
                </Badge>
                {edge.edge_type !== "manual" && (
                  <span className="text-[10px] text-muted-foreground/60">
                    {Math.round((1 - (edge.distance ?? 0)) * 100)}% match
                  </span>
                )}
              </div>
            </Link>
          );
        })}
      </div>
    </div>
  );
}
