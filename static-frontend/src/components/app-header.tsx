import useSWR from "swr"
import {
  BookOpen,
  Brain,
  FolderOpen,
  GitBranch,
  Github,
  KeyRound,
  LogIn,
  LogOut,
  MessageSquare,
  Search,
  Sparkles,
  User,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { ThemeToggle } from "@/components/theme-toggle"

const GITHUB_REPO_URL = "https://github.com/matst80/rust-rag"

const navigation = [
  { name: "Search", href: "/", icon: Search },
  { name: "Assisted", href: "/assisted/", icon: Sparkles },
  { name: "Chat", href: "/chat/", icon: MessageSquare },
  { name: "Entries", href: "/entries/", icon: FolderOpen },
  { name: "Graph", href: "/visualize/", icon: GitBranch },
]

const navigationRight = [
  { name: "Docs", href: "/start-guide/", icon: BookOpen },
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

export function AppHeader() {
  const pathname = window.location.pathname
  const { data: session } = useSWR<SessionResponse>("/auth/session", loadSession, {
    revalidateOnFocus: true,
  })
  const displayName =
    session?.user?.name ?? session?.user?.preferred_username ?? session?.user?.email ?? "Signed in"

  return (
    <header className="sticky top-0 z-50 border-b border-border bg-background">
      <div className="flex items-center justify-between px-4">

        {/* Logo */}
        <div className="flex items-center gap-6">
          <a
            href="/"
            className="flex items-center gap-2 font-mono text-xs font-black uppercase tracking-[3px] text-primary"
          >
            <Brain className="size-4" />
            <span className="hidden sm:inline">bRAG</span>
          </a>

          {/* Nav */}
          <nav className="flex items-center">
            {navigation.map((item) => {
              const isActive =
                item.href === "/" ? pathname === "/" : pathname.startsWith(item.href)
              return (
                <a
                  key={item.name}
                  href={item.href}
                  className={cn(
                    "flex items-center gap-1.5 px-3 py-3 font-mono text-[10px] font-bold uppercase tracking-[2px] transition-colors border-b-2",
                    isActive
                      ? "text-primary border-primary"
                      : "text-muted-foreground border-transparent hover:text-foreground hover:border-border"
                  )}
                >
                  <item.icon className="size-3.5" />
                  <span className="hidden sm:inline">{item.name}</span>
                </a>
              )
            })}
          </nav>
        </div>

        {/* Right side */}
        <div className="flex items-center">
          {navigationRight.map((item) => {
            const isActive = pathname.startsWith(item.href)
            return (
              <a
                key={item.name}
                href={item.href}
                className={cn(
                  "flex items-center gap-1.5 px-3 py-3 font-mono text-[10px] font-bold uppercase tracking-[2px] transition-colors border-b-2",
                  isActive
                    ? "text-primary border-primary"
                    : "text-muted-foreground border-transparent hover:text-foreground hover:border-border"
                )}
              >
                <item.icon className="size-3.5" />
                <span className="hidden sm:inline">{item.name}</span>
              </a>
            )
          })}
          {session?.authenticated ? (
            <>
              <a
                href="/auth/tokens/"
                className={cn(
                  "flex items-center gap-1.5 px-3 py-3 font-mono text-[10px] font-bold uppercase tracking-[2px] transition-colors",
                  pathname.startsWith("/auth/tokens") || pathname.startsWith("/auth/device")
                    ? "text-primary"
                    : "text-muted-foreground hover:text-foreground"
                )}
                title="MCP tokens"
              >
                <KeyRound className="size-3.5" />
                <span className="hidden sm:inline">Tokens</span>
              </a>
              <span className="hidden items-center gap-1.5 px-3 py-3 font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground md:flex">
                <User className="size-3.5" />
                <span>{displayName}</span>
              </span>
              <a
                href="/auth/logout"
                className="flex items-center gap-1.5 px-3 py-3 font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground transition-colors hover:text-foreground"
              >
                <LogOut className="size-3.5" />
                <span className="hidden sm:inline">Sign out</span>
              </a>
            </>
          ) : session?.auth_enabled ? (
            <a
              href="/auth/login"
              className="flex items-center gap-1.5 px-3 py-3 font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground transition-colors hover:text-foreground"
            >
              <LogIn className="size-3.5" />
              <span className="hidden sm:inline">Sign in</span>
            </a>
          ) : null}
          <ThemeToggle />
          <a
            href={GITHUB_REPO_URL}
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-1.5 px-3 py-3 font-mono text-[10px] font-bold uppercase tracking-[2px] text-muted-foreground transition-colors hover:text-foreground"
            aria-label="GitHub"
          >
            <Github className="size-3.5" />
            <span className="hidden sm:inline">GitHub</span>
          </a>
        </div>
      </div>
    </header>
  )
}

