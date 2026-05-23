"use client"

import { useState, useMemo, useEffect, memo } from "react"
import useSWR, { mutate } from "swr"
import { Plus, MoreVertical, Pencil, Trash2, GripVertical, Clock, User, AlertCircle } from "lucide-react"
import { motion, AnimatePresence } from "framer-motion"
import { Card, CardHeader, CardTitle, CardContent, CardFooter } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { api } from "@/lib/api"
import { Entry } from "@/lib/api/types"
import { cn } from "@/lib/utils"
import { toast } from "sonner"
import { EntryForm } from "@/components/entries/entry-form"
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"

const COLUMNS = [
  { id: "backlog", title: "Backlog", color: "bg-slate-500/10 text-slate-500 border-slate-500/20" },
  { id: "todo", title: "To Do", color: "bg-blue-500/10 text-blue-500 border-blue-500/20" },
  { id: "in_progress", title: "In Progress", color: "bg-amber-500/10 text-amber-500 border-amber-500/20" },
  { id: "done", title: "Done", color: "bg-emerald-500/10 text-emerald-500 border-emerald-500/20" },
]

export function KanbanBoard() {
  const { data, isLoading, error } = useSWR("kanban-cards", () =>
    api.items.list({ type: "kanban_card", limit: 1000 })
  )

  const [editingCard, setEditingCard] = useState<Entry | null>(null)
  const [isAddingToColumn, setIsAddingToColumn] = useState<string | null>(null)
  const [draggedCard, setDraggedCard] = useState<Entry | null>(null)

  const cardsByStatus = useMemo(() => {
    const map: Record<string, Entry[]> = {
      backlog: [],
      todo: [],
      in_progress: [],
      done: [],
    }
    if (data?.items) {
      data.items.forEach((item) => {
        const status = (item.data as any)?.status || "backlog"
        if (map[status]) {
          map[status].push(item)
        } else {
          map.backlog.push(item)
        }
      })
    }
    return map
  }, [data])

  const handleMoveCard = async (card: Entry, newStatus: string) => {
    const updatedData = {
      ...(card.data as any),
      status: newStatus,
    }

    try {
      // Optimistic update
      mutate(
        "kanban-cards",
        (current: any) => ({
          ...current,
          items: current.items.map((item: Entry) =>
            item.id === card.id ? { ...item, data: updatedData } : item
          ),
        }),
        false
      )

      await api.items.update(card.id, {
        source_id: card.source_id,
        text: card.text,
        metadata: card.metadata,
        data: updatedData,
      })
      toast.success(`Moved to ${newStatus.replace("_", " ")}`)
    } catch (err) {
      toast.error("Failed to move card")
      mutate("kanban-cards")
    }
  }

  const handleDeleteCard = async (id: string) => {
    if (!confirm("Are you sure you want to delete this card?")) return

    try {
      await api.items.delete(id)
      mutate("kanban-cards")
      toast.success("Card deleted")
    } catch (err) {
      toast.error("Failed to delete card")
    }
  }

  if (isLoading) return <div className="flex items-center justify-center h-64 font-mono text-sm animate-pulse">Loading board…</div>
  if (error) return <div className="p-4 bg-destructive/10 border border-destructive/20 rounded-md text-destructive flex items-center gap-3">
    <AlertCircle className="size-4" />
    <span className="text-sm font-mono">Failed to load Kanban board</span>
  </div>

  return (
    <div className="flex gap-6 h-full min-h-[calc(100vh-200px)] overflow-x-auto pb-8 no-scrollbar">
      {COLUMNS.map((column) => (
        <div
          key={column.id}
          className="flex flex-col w-80 shrink-0 bg-muted/30 rounded-lg border border-border/50"
          onDragOver={(e) => e.preventDefault()}
          onDrop={() => {
            if (draggedCard && (draggedCard.data as any).status !== column.id) {
              handleMoveCard(draggedCard, column.id)
              setDraggedCard(null)
            }
          }}
        >
          <div className="p-4 flex items-center justify-between border-b border-border/50">
            <div className="flex items-center gap-2">
              <h3 className="font-mono text-[10px] font-black uppercase tracking-[2px]">{column.title}</h3>
              <Badge variant="outline" className={cn("px-1.5 py-0 text-[10px] rounded-sm font-mono", column.color)}>
                {cardsByStatus[column.id].length}
              </Badge>
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="size-6 text-muted-foreground hover:text-foreground"
              onClick={() => setIsAddingToColumn(column.id)}
            >
              <Plus className="size-3.5" />
            </Button>
          </div>

          <div className="flex-1 p-3 flex flex-col gap-3 overflow-y-auto max-h-[calc(100vh-250px)] no-scrollbar">
            <AnimatePresence initial={false}>
              {cardsByStatus[column.id].map((card) => (
                <KanbanCard
                  key={card.id}
                  card={card}
                  onEdit={() => setEditingCard(card)}
                  onDelete={() => handleDeleteCard(card.id)}
                  onDragStart={() => setDraggedCard(card)}
                />
              ))}
            </AnimatePresence>
            {cardsByStatus[column.id].length === 0 && (
              <div className="flex-1 border-2 border-dashed border-border/20 rounded-lg flex items-center justify-center p-8 opacity-40">
                <span className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground">Empty</span>
              </div>
            )}
          </div>
        </div>
      ))}

      {/* Edit/New Dialog */}
      <Dialog open={!!editingCard || !!isAddingToColumn} onOpenChange={(open) => {
        if (!open) {
          setEditingCard(null)
          setIsAddingToColumn(null)
        }
      }}>
        <DialogContent className="max-w-2xl max-h-[90vh] overflow-hidden flex flex-col p-0">
          <DialogHeader className="p-6 pb-2">
            <DialogTitle className="font-mono text-sm uppercase tracking-widest">
              {editingCard ? "Edit Card" : `New Card in ${isAddingToColumn?.replace("_", " ")}`}
            </DialogTitle>
          </DialogHeader>
          <div className="flex-1 overflow-y-auto px-6 pb-6 no-scrollbar">
            <EntryForm
              minimal
              entry={editingCard || undefined}
              initialData={editingCard ? undefined : { 
                type_name: "kanban_card", 
                data: { status: isAddingToColumn },
                source_id: "knowledge"
              }}
              onSuccess={() => {
                mutate("kanban-cards")
                setEditingCard(null)
                setIsAddingToColumn(null)
              }}
              onCancel={() => {
                setEditingCard(null)
                setIsAddingToColumn(null)
              }}
            />
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}

const KanbanCard = memo(function KanbanCard({ card, onEdit, onDelete, onDragStart }: { 
  card: Entry, 
  onEdit: () => void, 
  onDelete: () => void,
  onDragStart: () => void
}) {
  const data = card.data as any
  const priorityColors = {
    urgent: "bg-red-500/20 text-red-500 border-red-500/30",
    high: "bg-orange-500/20 text-orange-500 border-orange-500/30",
    medium: "bg-blue-500/20 text-blue-500 border-blue-500/30",
    low: "bg-slate-500/20 text-slate-500 border-slate-500/30",
  }

  return (
    <motion.div
      layout="position"
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.95 }}
      draggable
      onDragStart={onDragStart}
      className="group cursor-grab active:cursor-grabbing"
    >
      <Card className="p-0 overflow-hidden hover:border-primary/50 transition-colors shadow-none border-border/60 bg-card">
        <div className="p-3.5 space-y-3">
          <div className="flex items-start justify-between gap-2">
            <h4 className="text-xs font-semibold leading-snug group-hover:text-primary transition-colors line-clamp-2">
              {data.title || "Untitled Card"}
            </h4>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon" className="size-7 -mr-1.5 opacity-0 group-hover:opacity-100 transition-opacity">
                  <MoreVertical className="size-3.5" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-32 font-mono text-[10px] uppercase tracking-wider">
                <DropdownMenuItem onClick={onEdit} className="gap-2">
                  <Pencil className="size-3" /> Edit
                </DropdownMenuItem>
                <DropdownMenuItem onClick={onDelete} className="gap-2 text-destructive focus:text-destructive">
                  <Trash2 className="size-3" /> Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          {data.description && (
            <p className="text-[11px] text-muted-foreground leading-relaxed line-clamp-3">
              {data.description}
            </p>
          )}

          <div className="flex flex-wrap gap-2 pt-1">
            {data.priority && (
              <Badge variant="outline" className={cn("px-1.5 py-0 text-[9px] rounded-sm font-black uppercase tracking-tighter", priorityColors[data.priority as keyof typeof priorityColors])}>
                {data.priority}
              </Badge>
            )}
            {data.assignee && (
              <div className="flex items-center gap-1 text-[10px] text-muted-foreground font-mono">
                <User className="size-3" />
                <span className="truncate max-w-[100px]">{data.assignee}</span>
              </div>
            )}
            {data.due_date && (
              <div className="flex items-center gap-1 text-[10px] text-muted-foreground font-mono ml-auto">
                <Clock className="size-3" />
                <span>{data.due_date}</span>
              </div>
            )}
          </div>
        </div>
      </Card>
    </motion.div>
  )
})
