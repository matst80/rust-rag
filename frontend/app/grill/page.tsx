import { Suspense } from "react"
import { GrillCockpit } from "@/components/grill/cockpit";

export default function GrillPage() {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <Suspense>
        <GrillCockpit />
      </Suspense>
    </div>
  );
}
