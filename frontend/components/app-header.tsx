"use client"

import { Menu } from "lucide-react"
import { ThemeToggle } from "@/components/theme-toggle"
import { HeaderSearch } from "@/components/header-search"

/** Slim top bar: mobile menu toggle, large centered quick search, theme toggle.
 *  Navigation lives in the app sidebar; this bar stays out of the page scroll
 *  (the shell's <main> scrolls beneath it). */
export function AppHeader({ onMenu }: { onMenu: () => void }) {
  return (
    <header className="grid h-12 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-2 border-b border-border bg-background px-2 md:px-4">
      <div className="flex items-center">
        <button
          type="button"
          onClick={onMenu}
          aria-label="Open navigation"
          className="flex size-8 items-center justify-center text-muted-foreground transition-colors hover:text-foreground md:hidden"
        >
          <Menu className="size-4" />
        </button>
      </div>

      <div className="flex w-full min-w-0 justify-center">
        <HeaderSearch />
      </div>

      <div className="flex items-center justify-end">
        <ThemeToggle />
      </div>
    </header>
  )
}
