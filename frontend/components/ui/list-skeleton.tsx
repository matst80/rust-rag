import { Skeleton } from "./skeleton"

export function ListSkeleton({ count = 4, compact = false }: { count?: number; compact?: boolean }) {
  return <div role="status" aria-label="Loading entries" className="w-full space-y-3">
    {Array.from({ length: count }, (_, index) => <div key={index} className={compact ? "flex items-center gap-3 py-2" : "space-y-3 rounded-2xl border border-border/40 p-5"}>
      <Skeleton className={compact ? "size-5 shrink-0" : "h-4 w-2/3"} />
      <Skeleton className={compact ? "h-4 w-2/3" : "h-3 w-5/6"} />
      {!compact && <Skeleton className="h-3 w-1/3" />}
    </div>)}
  </div>
}
