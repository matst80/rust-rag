import { KanbanBoard } from "@/components/kanban/kanban-board";
import { AppHeader } from "@/components/app-header";

export default function KanbanPage() {
  return (
    <div className="flex flex-col h-screen">
      <AppHeader />
      <main className="flex-1 p-6">
        <div className="max-w-350 mx-auto h-full flex flex-col gap-6">
          <header className="flex flex-col gap-1">
            <h1 className="text-xl font-bold font-mono uppercase tracking-[3px]">
              Kanban Board
            </h1>
            <p className="text-xs text-muted-foreground font-mono uppercase tracking-widest">
              Manage work items and tasks
            </p>
          </header>

          <div className="flex-1">
            <KanbanBoard />
          </div>
        </div>
      </main>
    </div>
  );
}
