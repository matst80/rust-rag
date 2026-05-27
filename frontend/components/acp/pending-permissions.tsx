"use client";

import { Circle } from "lucide-react";

interface PendingPermissionsProps {
  pendingPermissions: any[];
}

export function PendingPermissions({ pendingPermissions }: PendingPermissionsProps) {
  if (pendingPermissions.length === 0) return null;

  return (
    <div className="border-t border-border bg-amber-500/5 p-4 shrink-0">
      <div className="text-xs font-bold text-amber-600 uppercase tracking-wider mb-2 flex items-center gap-2">
        <Circle className="size-2 fill-amber-500 animate-pulse" />
        Pending Permissions
      </div>
      <div className="space-y-2 max-h-40 overflow-y-auto">
        {pendingPermissions.map((p) => (
          <div
            key={p.localSeq}
            className="text-sm p-3 bg-card border border-border rounded-md shadow-sm font-mono text-[10px]"
          >
            {JSON.stringify(p.payload, null, 2)}
          </div>
        ))}
      </div>
    </div>
  );
}
