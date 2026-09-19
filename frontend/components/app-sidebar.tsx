"use client"

import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"
import { Suspense, useMemo, useState } from "react"
import useSWR from "swr"
import {
  BookOpen,
  Braces,
  Brain,
  ChevronDown,
  ChevronRight,
  Code,
  Database,
  Flame,
  FolderOpen,
  FolderTree,
  GitBranch,
  Inbox,
  KeyRound,
  LayoutGrid,
  LogIn,
  LogOut,
  MessageSquare,
  Plug,
  Radar,
  Search,
  Server,
  Share2,
  Sparkles,
  Terminal,
  X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { ThemeToggle } from "@/components/theme-toggle"
import { WikiTreeNode } from "@/components/wiki/wiki-tree-node"
import { useEntriesPaths } from "@/lib/api"
import { useWikiMoveEntry } from "@/hooks/use-wiki-move"
import { ancestorChain, buildSourceTrees, wikiHref } from "@/lib/wiki-tree"

const navigationGroups = [
  {
    label: "Views",
    items: [
      { name: "Search", href: "/", icon: Search },
      { name: "Deep Search", href: "/assisted", icon: Sparkles },
      { name: "Entries", href: "/entries", icon: FolderOpen },
      { name: "Wiki", href: "/wiki", icon: FolderTree },
      { name: "Graph", href: "/visualize", icon: GitBranch },
      { name: "Code", href: "/code", icon: Code },
      { name: "Schemas", href: "/schemas", icon: Braces },
    ],
  },
  {
    label: "Workspace",
    items: [
      { name: "Kanban", href: "/kanban", icon: LayoutGrid },
      { name: "Chat", href: "/chat", icon: MessageSquare },
      { name: "Messages", href: "/messages", icon: Inbox },
      { name: "Agents", href: "/acp", icon: Terminal },
      { name: "Grill", href: "/grill", icon: Flame },
      { name: "Insights", href: "/insights", icon: Radar },
    ],
  },
  {
    label: "Resources",
    items: [
      { name: "Docs", href: "/start-guide", icon: BookOpen },
      { name: "MCP Setup", href: "/mcp-setup", icon: Server },
      { name: "Integrations", href: "/settings/integrations", icon: Plug },
      { name: "Share", href: "/share", icon: Share2 },
    ],
  },
]

interface SessionResponse {
  authenticated: boolean
  auth_enabled: boolean
  user?: {
    name?: string
    email?: string
    preferred_username?: string
  }
}

async function loadSession(url: string): Promise<SessionResponse> {
  const response = await fetch(url, { cache: "no-store" })
  if (!response.ok) return { authenticated: false, auth_enabled: true }
  return response.json()
}

interface AppSidebarProps {
  /** Mobile drawer open state (ignored on desktop where the sidebar is persistent). */
  mobileOpen: boolean
  onClose: () => void
}

export function AppSidebar({ mobileOpen, onClose }: AppSidebarProps) {
  const pathname = usePathname()

  const body = (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      {/* Brand */}
      <div className="flex h-12 shrink-0 items-center border-b border-sidebar-border px-4">
        <Link
          href="/"
          onClick={onClose}
          className="flex items-center gap-2 font-mono text-xs font-black uppercase tracking-[3px] text-primary"
        >
          <Brain className="size-4" />
          <span>bRAG</span>
        </Link>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close sidebar"
          className="ml-auto flex size-7 items-center justify-center text-muted-foreground hover:text-foreground md:hidden"
        >
          <X className="size-4" />
        </button>
      </div>

      {/* Views */}
      <nav className="min-h-0 shrink overflow-y-auto px-2 py-3">
        {navigationGroups.map((group) => (
          <div key={group.label}>
            <p className="px-2 pb-1.5 font-mono text-[9px] font-black uppercase tracking-[0.25em] text-muted-foreground/70 first:pt-0 [&:not(:first-child)]:pt-4">
              {group.label}
            </p>
            {group.items.map((item) => {
              const isActive =
                item.href === "/" ? pathname === "/" : pathname.startsWith(item.href)
              return (
                <Link
                  key={item.name}
                  href={item.href}
                  onClick={onClose}
                  prefetch={false}
                  aria-label={item.name}
                  aria-current={isActive ? "page" : undefined}
                  className={cn(
                    "flex items-center gap-2.5 border-l-2 px-2.5 py-1.5 font-mono text-[11px] font-bold uppercase tracking-[1.5px] transition-colors",
                    isActive
                      ? "border-primary bg-primary/10 text-primary"
                      : "border-transparent text-muted-foreground hover:border-sidebar-border hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
                  )}
                >
                  <item.icon className="size-3.5 shrink-0" aria-hidden="true" />
                  {item.name}
                </Link>
              )
            })}
          </div>
        ))}
      </nav>

      {/* Wiki tree */}
      <div className="flex min-h-0 flex-1 flex-col border-t border-sidebar-border pt-2">
        <div className="flex shrink-0 items-center justify-between px-4 pb-1">
          <p className="font-mono text-[9px] font-black uppercase tracking-[0.25em] text-muted-foreground/70">
            Wiki
          </p>
          <Link
            href="/wiki"
            onClick={onClose}
            prefetch={false}
            className="font-mono text-[9px] font-bold uppercase tracking-wider text-muted-foreground transition-colors hover:text-primary"
          >
            Open →
          </Link>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-3">
          <Suspense fallback={<p className="px-3 font-mono text-[10px] text-muted-foreground">Loading…</p>}>
            <SidebarWikiTree onNavigate={onClose} />
          </Suspense>
        </div>
      </div>

      {/* Footer */}
      <div className="flex h-12 shrink-0 items-center gap-1 border-t border-sidebar-border px-2">
        <SidebarSessionActions />
        <div className="ml-auto flex items-center">
          <ThemeToggle />
        </div>
      </div>
    </div>
  )

  return (
    <>
      {/* Desktop: persistent column */}
      <aside className="hidden h-full w-60 shrink-0 border-r border-sidebar-border md:flex md:flex-col">
        {body}
      </aside>

      {/* Mobile: slide-in drawer */}
      {mobileOpen && (
        <div className="fixed inset-0 z-[70] flex md:hidden">
          <div
            className="absolute inset-0 bg-background/80 backdrop-blur-sm"
            onClick={onClose}
          />
          <div className="animate-in slide-in-from-left relative h-full w-72 max-w-[80%] border-r border-sidebar-border shadow-lg duration-200">
            {body}
          </div>
        </div>
      )}
    </>
  )
}

function SidebarSessionActions() {
  const pathname = usePathname()
  const { data: session } = useSWR<SessionResponse>("/auth/session", loadSession, {
    revalidateOnFocus: true,
  })
  const displayName =
    session?.user?.name ?? session?.user?.preferred_username ?? session?.user?.email ?? "Signed in"

  if (session?.authenticated) {
    return (
      <>
        <Link
          href="/auth/tokens"
          prefetch={false}
          aria-label="MCP tokens"
          title="MCP tokens"
          className={cn(
            "flex items-center gap-1.5 px-2 py-1.5 font-mono text-[10px] font-bold uppercase tracking-[1.5px] transition-colors",
            pathname.startsWith("/auth/tokens") || pathname.startsWith("/auth/device")
              ? "text-primary"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <KeyRound className="size-3.5" aria-hidden="true" />
        </Link>
        <a
          href="/auth/logout"
          aria-label="Sign out"
          title={`Sign out (${displayName})`}
          className="flex min-w-0 items-center gap-1.5 px-2 py-1.5 font-mono text-[10px] font-bold uppercase tracking-[1.5px] text-muted-foreground transition-colors hover:text-foreground"
        >
          <LogOut className="size-3.5 shrink-0" aria-hidden="true" />
          <span className="truncate">{displayName}</span>
        </a>
      </>
    )
  }

  if (session?.auth_enabled) {
    return (
      <a
        href="/auth/login"
        aria-label="Sign in"
        className="flex items-center gap-1.5 px-2 py-1.5 font-mono text-[10px] font-bold uppercase tracking-[1.5px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <LogIn className="size-3.5" aria-hidden="true" />
        <span>Sign in</span>
      </a>
    )
  }

  return null
}

/** Sources + folder tree shown in the app sidebar. Suspense-wrapped by the
 *  sidebar because it reads the current wiki selection from search params. */
function SidebarWikiTree({ onNavigate }: { onNavigate: () => void }) {
  const searchParams = useSearchParams()
  const activeSourceId = searchParams.get("source_id")
  const activePath = searchParams.get("path")
  const activeEntryId = searchParams.get("entry")

  const { data: pathsResp } = useEntriesPaths()
  const sourceTrees = useMemo(
    () => (pathsResp ? buildSourceTrees(pathsResp.paths) : []),
    [pathsResp]
  )
  const activeChain = useMemo(() => ancestorChain(activePath ?? undefined), [activePath])
  const onDropEntry = useWikiMoveEntry()

  if (!pathsResp) {
    return (
      <p className="px-3 font-mono text-[10px] text-muted-foreground">Loading…</p>
    )
  }

  if (sourceTrees.length === 0) {
    return (
      <p className="px-3 font-mono text-[10px] leading-relaxed text-muted-foreground">
        No wiki paths yet.
      </p>
    )
  }

  return (
    <>
      {sourceTrees.map((tree) => (
        <SidebarSourceRoot
          key={tree.sourceId}
          tree={tree}
          activeSourceId={activeSourceId}
          activePath={activePath}
          activeEntryId={activeEntryId}
          activeChain={activeChain}
          onDropEntry={onDropEntry}
          onNavigate={onNavigate}
        />
      ))}
    </>
  )
}

interface SidebarSourceRootProps {
  tree: ReturnType<typeof buildSourceTrees>[number]
  activeSourceId: string | null
  activePath: string | null
  activeEntryId: string | null
  activeChain: Set<string>
  onDropEntry: (targetSourceId: string, targetPath: string, entryId: string) => void
  onNavigate: () => void
}

function SidebarSourceRoot({
  tree,
  activeSourceId,
  activePath,
  activeEntryId,
  activeChain,
  onDropEntry,
  onNavigate,
}: SidebarSourceRootProps) {
  const isActiveSource = tree.sourceId === activeSourceId
  const [open, setOpen] = useState(isActiveSource)
  const isSelected = isActiveSource && activePath === null

  return (
    <div className="flex flex-col">
      <div className="flex items-center">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="flex size-5 shrink-0 items-center justify-center text-muted-foreground transition-colors hover:text-foreground"
          aria-label={open ? "Collapse" : "Expand"}
        >
          {open ? (
            <ChevronDown className="size-3" />
          ) : (
            <ChevronRight className="size-3" />
          )}
        </button>
        <Link
          href={wikiHref(tree.sourceId)}
          onClick={onNavigate}
          prefetch={false}
          className={cn(
            "flex min-w-0 flex-1 items-center gap-2 px-1.5 py-1 font-mono text-[11px] font-bold uppercase tracking-wider transition-colors hover:bg-sidebar-accent",
            isSelected
              ? "bg-primary/10 text-primary"
              : "text-foreground"
          )}
        >
          {open ? (
            <FolderOpen className="size-3.5 shrink-0 text-primary" />
          ) : (
            <Database className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          <span className="truncate">{tree.sourceId}</span>
          <span className="ml-auto shrink-0 font-mono text-[9px] tabular-nums text-muted-foreground">
            {tree.totalCount}
          </span>
        </Link>
      </div>

      {open && (
        <div className="flex flex-col">
          {tree.roots.map((node) => (
            <WikiTreeNode
              key={node.path}
              sourceId={tree.sourceId}
              node={node}
              selectedSourceId={activeSourceId}
              selectedPath={activePath}
              selectedEntryId={activeEntryId}
              depth={0}
              activeChain={activeChain}
              onDropEntry={onDropEntry}
            />
          ))}
        </div>
      )}
    </div>
  )
}
