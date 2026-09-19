"use client"

import { useEffect, useState } from "react"
import { usePathname } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { AppSidebar } from "@/components/app-sidebar"

/** Application frame: persistent sidebar (nav + wiki tree) on the left,
 *  slim top bar with the quick search, page content scrolls inside <main>.
 *  The top bar is 3rem tall — pages rely on `calc(100vh - 3rem)` fills.
 *  Standalone routes (e.g. public share links) render without the frame. */
const STANDALONE_ROUTES = ["/public"]

export function AppShell({ children }: { children: React.ReactNode }) {
  const [mobileNavOpen, setMobileNavOpen] = useState(false)
  const pathname = usePathname()

  // Close the mobile drawer whenever the route changes (covers links inside
  // the wiki tree that can't be intercepted individually).
  useEffect(() => {
    setMobileNavOpen(false)
  }, [pathname])

  if (STANDALONE_ROUTES.some((route) => pathname.startsWith(route))) {
    return <>{children}</>
  }

  return (
    <div className="flex h-screen overflow-hidden">
      <AppSidebar
        mobileOpen={mobileNavOpen}
        onClose={() => setMobileNavOpen(false)}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <AppHeader onMenu={() => setMobileNavOpen(true)} />
        <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
      </div>
    </div>
  )
}
