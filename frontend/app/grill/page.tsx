import { Suspense } from "react"
import { GrillCockpit } from "@/components/grill/cockpit";
import { AppHeader } from "@/components/app-header";

export default function GrillPage() {
  return (
    <div className="flex flex-col h-screen">
      <AppHeader />
      <Suspense>
        <GrillCockpit />
      </Suspense>
    </div>
  );
}
