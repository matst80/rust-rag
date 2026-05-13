"use client"

import * as React from "react"
import { motion, AnimatePresence } from "framer-motion"
import { Check, X, Info, Network, Zap, ShieldCheck } from "lucide-react"
import { api } from "@/lib/api/client"
import { Edge } from "@/lib/api/types"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import { toast } from "sonner"
import { cn } from "@/lib/utils"

interface EdgeReviewListProps {
  onReviewComplete?: () => void
}

export function EdgeReviewList({ onReviewComplete }: EdgeReviewListProps) {
  const [edges, setEdges] = React.useState<Edge[]>([])
  const [loading, setLoading] = React.useState(true)

  const fetchSuggestedEdges = React.useCallback(async () => {
    try {
      setLoading(true)
      const allEdges = await api.edges.list()
      const suggested = allEdges.filter(
        (e) => e.metadata?.status === "suggested"
      )
      setEdges(suggested)
    } catch (error) {
      console.error("Failed to fetch edges:", error)
      toast.error("Failed to load suggested edges")
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    fetchSuggestedEdges()
  }, [fetchSuggestedEdges])

  const handleApprove = async (edge: Edge) => {
    try {
      const updatedMetadata = {
        ...(edge.metadata || {}),
        status: "confirmed",
      }
      await api.edges.update(edge.id, { metadata: updatedMetadata })
      setEdges((prev) => prev.filter((e) => e.id !== edge.id))
      toast.success("Edge confirmed")
      onReviewComplete?.()
    } catch (error) {
      console.error("Failed to approve edge:", error)
      toast.error("Failed to approve edge")
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await api.edges.delete(id)
      setEdges((prev) => prev.filter((e) => e.id !== id))
      toast.success("Edge rejected and deleted")
      onReviewComplete?.()
    } catch (error) {
      console.error("Failed to delete edge:", error)
      toast.error("Failed to delete edge")
    }
  }

  if (loading && edges.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-primary"></div>
      </div>
    )
  }

  if (edges.length === 0) {
    return (
      <Card className="border-dashed bg-muted/50">
        <CardContent className="flex flex-col items-center justify-center py-12 text-center">
          <ShieldCheck className="h-12 w-12 text-muted-foreground mb-4 opacity-20" />
          <h3 className="text-lg font-medium">Clear Queue</h3>
          <p className="text-sm text-muted-foreground max-w-[250px]">
            No suggested edges awaiting review. The ontology is synchronized.
          </p>
        </CardContent>
      </Card>
    )
  }

  return (
    <ScrollArea className="h-full pr-4">
      <div className="space-y-4 pb-8">
        <AnimatePresence mode="popLayout">
          {edges.map((edge) => (
            <motion.div
              key={edge.id}
              layout
              initial={{ opacity: 0, scale: 0.95, y: 10 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.9, x: -20 }}
              transition={{ duration: 0.2 }}
            >
              <Card className="overflow-hidden border-primary/10 hover:border-primary/30 transition-colors group">
                <CardHeader className="pb-3 flex flex-row items-start justify-between space-y-0">
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <Badge variant="outline" className="bg-primary/5 text-primary border-primary/20 flex gap-1 items-center">
                        <Zap className="h-3 w-3" />
                        AI Inferred
                      </Badge>
                      <Badge variant="secondary" className="font-mono text-[10px]">
                        {edge.relationship}
                      </Badge>
                      {edge.metadata?.confidence && (
                        <span className="text-[10px] font-medium text-muted-foreground">
                          {Math.round((edge.metadata.confidence as number) * 100)}% confidence
                        </span>
                      )}
                    </div>
                    <CardTitle className="text-sm font-mono mt-2 flex items-center gap-2">
                      <span className="text-muted-foreground truncate max-w-[120px]">{edge.source_id}</span>
                      <Network className="h-3 w-3 text-primary/50" />
                      <span className="truncate max-w-[120px]">{edge.target_id}</span>
                    </CardTitle>
                  </div>
                  <div className="flex gap-2">
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8 text-destructive hover:bg-destructive/10"
                      onClick={() => handleDelete(edge.id)}
                    >
                      <X className="h-4 w-4" />
                    </Button>
                    <Button
                      size="icon"
                      variant="default"
                      className="h-8 w-8 bg-emerald-500 hover:bg-emerald-600 shadow-lg shadow-emerald-500/20"
                      onClick={() => handleApprove(edge)}
                    >
                      <Check className="h-4 w-4" />
                    </Button>
                  </div>
                </CardHeader>
                <CardContent className="pt-0">
                  {edge.metadata?.reasoning && (
                    <div className="mt-2 p-3 rounded-lg bg-muted/40 border border-primary/5 text-xs text-muted-foreground italic relative overflow-hidden group-hover:bg-muted/60 transition-colors">
                      <Info className="h-3 w-3 absolute top-3 right-3 opacity-20" />
                      &quot;{edge.metadata.reasoning as string}&quot;
                    </div>
                  )}
                </CardContent>
              </Card>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ScrollArea>
  )
}
