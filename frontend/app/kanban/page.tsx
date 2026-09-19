import { KanbanBoard } from "@/components/kanban/kanban-board";

export default function KanbanPage() {
  return (
    <div className="mx-auto flex h-full max-w-350 flex-col gap-6 p-6">
      <header className="flex flex-col gap-1">
        <h1 className="text-xl font-bold font-mono uppercase tracking-[3px]">
          Kanban Board
        </h1>
        <p className="text-xs text-muted-foreground font-mono uppercase tracking-widest">
          Manage work items and tasks
        </p>
      </header>

      <div className="min-h-0 flex-1">
        <KanbanBoard />
      </div>
    </div>
  );
}
