"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import {
  GitBranch,
  Plus,
  X,
  Search,
  ChevronDown,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { EmbeddedGraph } from "../graph/embedded-graph";
import { RELATION_STYLES } from "../graph/relation-item";
import { cn } from "@/lib/utils";
import {
  useGraphStatus,
  useSearch,
  useCreateEdge,
  useDeleteEdge,
} from "@/lib/api";
import { useSWRConfig } from "swr";
import { toast } from "sonner";

const CANONICAL_PREDICATES = [
  { value: "is_a", label: "Is A", description: "Subtype or instance" },
  { value: "part_of", label: "Part Of", description: "Component of" },
  { value: "caused_by", label: "Caused By", description: "Effect of" },
  { value: "works_for", label: "Works For", description: "Affiliation" },
  {
    value: "contradicts",
    label: "Contradicts",
    description: "Incompatible with",
  },
  { value: "depends_on", label: "Depends On", description: "Requirement" },
  { value: "contains", label: "Contains", description: "Includes/embeds" },
  {
    value: "implemented_by",
    label: "Implemented By",
    description: "Realization of",
  },
];

interface EntryGraphPanelProps {
  id: string;
  edges: any[] | undefined;
}

export function EntryGraphPanel({ id, edges }: EntryGraphPanelProps) {
  const router = useRouter();
  const { mutate } = useSWRConfig();
  const { data: graphStatus } = useGraphStatus();

  // Edge management state
  const [searchQuery, setSearchQuery] = useState("");
  const [isAddingEdge, setIsAddingEdge] = useState(false);
  const [selectedTarget, setSelectedTarget] = useState<string | null>(null);

  const { data: searchResults } = useSearch(
    searchQuery,
    undefined,
    undefined,
    true,
    5,
  );

  const { trigger: createEdge } = useCreateEdge();
  const { trigger: deleteEdge } = useDeleteEdge();

  const handleAddEdge = async (targetId: string, relationship: string) => {
    try {
      await createEdge({
        source_id: id,
        target_id: targetId,
        relationship,
        directed: true,
        weight: 1.0,
      });
      mutate(["edges", id]);
      setIsAddingEdge(false);
      setSelectedTarget(null);
      setSearchQuery("");
      toast.success("Connection added");
    } catch {
      toast.error("Failed to add connection");
    }
  };

  const handleDeleteEdge = async (edgeId: string) => {
    try {
      await deleteEdge(edgeId);
      mutate(["edges", id]);
      toast.success("Connection removed");
    } catch {
      toast.error("Failed to remove connection");
    }
  };

  const handleUpdateEdgeType = async (edgeId: string, newRelation: string) => {
    try {
      const { api } = await import("@/lib/api/client");
      await api.edges.update(edgeId, { metadata: { relation: newRelation } });
      mutate(["edges", id]);
      toast.success("Connection updated");
    } catch {
      toast.error("Failed to update connection");
    }
  };

  return (
    <div className="flex h-full flex-col bg-background">
      <div className="flex h-10 shrink-0 items-center justify-between border-b border-border px-4">
        <div className="flex items-center gap-2">
          <GitBranch className="size-3.5 text-primary" />
          <span className="font-mono text-[10px] font-black uppercase tracking-[2px] text-muted-foreground">
            Connections
          </span>
        </div>
        <div className="flex items-center gap-4">
          <button
            onClick={() => setIsAddingEdge(true)}
            className="font-mono text-[10px] font-black uppercase tracking-[1px] text-primary hover:text-primary/80 transition-colors flex items-center gap-1"
          >
            <Plus className="size-3" />
            Add Edge
          </button>
          {graphStatus?.enabled && (
            <Link
              href={`/visualize?focus=${encodeURIComponent(id)}`}
              className="font-mono text-[10px] font-black uppercase tracking-[1px] text-muted-foreground hover:text-primary transition-colors"
            >
              Full View →
            </Link>
          )}
        </div>
      </div>

      <div className="flex-1 relative overflow-hidden">
        {graphStatus?.enabled ? (
          <EmbeddedGraph
            centerId={id}
            onNodeClick={(clickedId) => {
              if (clickedId !== id)
                router.push(`/entries/${encodeURIComponent(clickedId)}`);
            }}
          />
        ) : (
          <div className="flex h-full flex-col items-center justify-center p-8 text-center">
            <GitBranch className="size-6 mb-3 text-muted-foreground/30" />
            <p className="font-mono text-xs text-muted-foreground">
              Graph unavailable
            </p>
          </div>
        )}

        {/* Add Edge Search Overlay */}
        {isAddingEdge && (
          <div className="absolute inset-0 z-50 bg-background/95 backdrop-blur-sm p-4 animate-in fade-in zoom-in-95 duration-200">
            <div className="flex flex-col h-full max-w-md mx-auto space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="font-mono text-[11px] font-black uppercase tracking-[3px] text-primary">
                  Add Connection
                </h3>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => {
                    setIsAddingEdge(false);
                    setSelectedTarget(null);
                  }}
                  className="size-6"
                >
                  <X className="size-4" />
                </Button>
              </div>

              {!selectedTarget ? (
                <>
                  <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
                    <input
                      autoFocus
                      className="w-full bg-muted/50 border border-border rounded-xl h-10 pl-10 pr-4 text-sm focus:outline-none focus:ring-1 focus:ring-primary transition-all"
                      placeholder="Search entries to link..."
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                    />
                  </div>
                  <div className="flex-1 overflow-y-auto space-y-2 py-2">
                    {searchResults?.results
                      .filter((r) => r.id !== id)
                      .map((result) => (
                        <button
                          key={result.id}
                          onClick={() => setSelectedTarget(result.id)}
                          className="w-full text-left p-3 rounded-xl border border-border bg-card hover:border-primary/40 hover:bg-primary/5 transition-all group"
                        >
                          <div className="flex flex-col gap-0.5">
                            <span className="font-mono text-[10px] text-muted-foreground group-hover:text-primary transition-colors">
                              {result.id.substring(0, 16)}...
                            </span>
                            <span className="text-sm font-medium line-clamp-1">
                              {result.text.substring(0, 100)}
                            </span>
                          </div>
                        </button>
                      ))}
                  </div>
                </>
              ) : (
                <div className="space-y-6 animate-in slide-in-from-right-4 duration-300">
                  <div className="p-4 rounded-2xl bg-primary/5 border border-primary/20">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-mono text-[10px] text-primary/60 uppercase font-black">
                        Linking to
                      </span>
                      <button
                        onClick={() => setSelectedTarget(null)}
                        className="text-[10px] text-primary hover:underline ml-auto font-bold"
                      >
                        Change
                      </button>
                    </div>
                    <div className="font-mono text-xs font-bold truncate">
                      {selectedTarget}
                    </div>
                  </div>

                  <div className="space-y-3">
                    <span className="font-mono text-[10px] font-black uppercase tracking-widest text-muted-foreground">
                      Select Relationship
                    </span>
                    <div className="grid grid-cols-1 gap-2 overflow-y-auto max-h-[40vh] pr-2 custom-scrollbar">
                      {CANONICAL_PREDICATES.map((p) => (
                        <button
                          key={p.value}
                          onClick={() => handleAddEdge(selectedTarget, p.value)}
                          className="flex flex-col items-start gap-1 p-3 rounded-xl border border-border bg-card hover:border-primary/50 hover:bg-primary/5 transition-all group"
                        >
                          <span className="font-bold text-xs text-foreground group-hover:text-primary transition-colors">
                            {p.label}
                          </span>
                          <span className="text-[10px] text-muted-foreground opacity-60 leading-tight text-left">
                            {p.description}
                          </span>
                        </button>
                      ))}
                      <button
                        onClick={() =>
                          handleAddEdge(selectedTarget, "related_to")
                        }
                        className="flex items-center justify-center p-3 rounded-xl border border-dashed border-border bg-transparent hover:border-primary/50 hover:bg-primary/5 transition-all group mt-2"
                      >
                        <span className="text-xs font-bold text-muted-foreground group-hover:text-primary transition-colors">
                          Other / Generic Related
                        </span>
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {edges && edges.length > 0 && (
        <div className="h-[45%] shrink-0 border-t border-border bg-card/30 backdrop-blur-md overflow-hidden flex flex-col">
          <div className="px-4 py-2 border-b border-border bg-muted/5 flex items-center justify-between">
            <h3 className="font-mono text-[10px] font-black uppercase tracking-widest text-muted-foreground flex items-center gap-2">
              <GitBranch className="size-3 opacity-60" />
              Connected — {edges.length}
            </h3>
            <div className="flex items-center gap-2">
              <div className="size-2 rounded-full bg-primary/40 animate-pulse" />
              <span className="font-mono text-[9px] uppercase font-bold text-muted-foreground/60 tracking-wider">
                Manual Enabled
              </span>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-3 space-y-2 custom-scrollbar">
            {edges
              .sort((a, b) => {
                // Manual edges first
                if (a.edge_type === "manual" && b.edge_type !== "manual")
                  return -1;
                if (a.edge_type !== "manual" && b.edge_type === "manual")
                  return 1;
                return 0;
              })
              .map((edge) => {
                const targetId =
                  edge.source_id === id ? edge.target_id : edge.source_id;
                const isManual = edge.edge_type === "manual";

                return (
                  <div
                    key={edge.id}
                    className={cn(
                      "group relative flex flex-col gap-2 rounded-xl border p-3 transition-all",
                      isManual
                        ? "border-primary/20 bg-primary/[0.02] hover:border-primary/40 hover:bg-primary/[0.04] shadow-[0_2px_10px_rgba(var(--primary-rgb),0.02)]"
                        : "border-border bg-background/50 hover:border-border/80",
                    )}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex flex-col gap-1 min-w-0">
                        <div className="flex items-center gap-2">
                          {isManual ? (
                            <Popover>
                              <PopoverTrigger asChild>
                                <Button
                                  variant="ghost"
                                  className={cn(
                                    "h-5 px-1.5 font-mono text-[10px] font-black uppercase tracking-wider rounded-sm border hover:bg-primary/10 transition-colors flex items-center gap-1.5",
                                    RELATION_STYLES[
                                      edge.relationship.toLowerCase()
                                    ] || "text-primary border-primary/30",
                                  )}
                                >
                                  {edge.relationship.replace(/_/g, " ")}
                                  <ChevronDown className="size-2.5 opacity-60" />
                                </Button>
                              </PopoverTrigger>
                              <PopoverContent
                                className="w-64 p-0 rounded-xl border-border shadow-2xl overflow-hidden"
                                align="start"
                              >
                                <Command className="bg-transparent" loop>
                                  <CommandInput
                                    placeholder="Change relationship..."
                                    className="h-9 text-xs"
                                  />
                                  <CommandList>
                                    <CommandEmpty>No results</CommandEmpty>
                                    <CommandGroup heading="Canonical Predicates">
                                      {CANONICAL_PREDICATES.map((p) => (
                                        <CommandItem
                                          key={p.value}
                                          onSelect={() =>
                                            handleUpdateEdgeType(
                                              edge.id,
                                              p.value,
                                            )
                                          }
                                          className="rounded-lg m-1 p-2 cursor-pointer transition-all hover:bg-primary/10"
                                        >
                                          <div className="flex flex-col gap-0.5">
                                            <span className="font-bold text-[10px] text-primary">
                                              {p.label}
                                            </span>
                                            <span className="text-[8px] text-muted-foreground leading-tight">
                                              {p.description}
                                            </span>
                                          </div>
                                        </CommandItem>
                                      ))}
                                    </CommandGroup>
                                  </CommandList>
                                </Command>
                              </PopoverContent>
                            </Popover>
                          ) : (
                            <Badge
                              variant="outline"
                              className={cn(
                                "h-5 px-1.5 font-mono text-[10px] font-black uppercase tracking-wider border-border/60 text-muted-foreground/80 bg-muted/5 leading-none",
                                RELATION_STYLES[
                                  edge.relationship.toLowerCase()
                                ] || "",
                              )}
                            >
                              {edge.relationship}
                            </Badge>
                          )}

                          <span className="font-mono text-[9px] uppercase text-muted-foreground/40 font-bold">
                            {edge.source_id === id ? "→ out" : "← in"}
                          </span>
                        </div>

                        <Link
                          href={`/entries/${encodeURIComponent(targetId)}`}
                          className="font-mono text-xs text-foreground/80 hover:text-primary transition-colors truncate font-medium underline-offset-4 hover:underline"
                          title={targetId}
                        >
                          {targetId}
                        </Link>
                      </div>

                      {isManual && (
                        <div className="shrink-0 flex items-center gap-1">
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-7 rounded-lg text-muted-foreground/40 hover:text-red-500 hover:bg-red-500/10 transition-all opacity-0 group-hover:opacity-100"
                            onClick={() => handleDeleteEdge(edge.id)}
                          >
                            <Trash2 className="size-3.5" />
                          </Button>
                        </div>
                      )}
                    </div>

                    {!isManual && (
                      <div className="flex items-center gap-2">
                        <div className="flex-1 h-0.5 bg-muted rounded-full overflow-hidden">
                          <div
                            className="h-full bg-primary/20"
                            style={{
                              width: `${Math.max(10, (1 - (edge.distance ?? 0)) * 100)}%`,
                            }}
                          />
                        </div>
                        <span className="font-mono text-[8px] text-muted-foreground/40 font-black uppercase">
                          Similarity
                        </span>
                      </div>
                    )}
                  </div>
                );
              })}
          </div>
        </div>
      )}
    </div>
  );
}
