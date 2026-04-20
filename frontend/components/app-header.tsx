"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import useSWR from "swr"
import {
	BookOpen,
	Brain,
	FolderOpen,
	GitBranch,
	Github,
	LogIn,
	LogOut,
	Search,
	User,
} from "lucide-react"
import { ThemeToggle } from "@/components/theme-toggle"
import { cn } from "@/lib/utils"

const GITHUB_REPO_URL = "https://github.com/matst80/rust-rag"

const navigation = [
	{ name: "Search", href: "/", icon: Search },
	{ name: "Docs", href: "/start-guide", icon: BookOpen },
	{ name: "Entries", href: "/entries", icon: FolderOpen },
	{ name: "Graph", href: "/visualize", icon: GitBranch },
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
	if (!response.ok) {
		return { authenticated: false, auth_enabled: true }
	}

	return response.json()
}

export function AppHeader() {
	const pathname = usePathname()
	const { data: session } = useSWR<SessionResponse>("/auth/session", loadSession, {
		revalidateOnFocus: true,
	})
	const displayName =
		session?.user?.name ?? session?.user?.preferred_username ?? session?.user?.email ?? "Signed in"

	return (
		<header className="sticky top-0 z-50 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
			<div className="flex h-14 items-center justify-between px-4">
				<div className="flex items-center gap-6">
					<Link href="/" className="flex items-center gap-2 font-semibold">
						<Brain className="size-5" />
						<span className="hidden sm:inline">RAG Memory</span>
					</Link>
					<nav className="flex items-center gap-1">
						{navigation.map((item) => {
							const isActive =
								item.href === "/" ? pathname === "/" : pathname.startsWith(item.href)
							return (
								<Link
									key={item.name}
									href={item.href}
									className={cn(
										"flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium transition-colors",
										isActive
											? "bg-accent text-accent-foreground"
											: "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
									)}
								>
									<item.icon className="size-4" />
									<span className="hidden sm:inline">{item.name}</span>
								</Link>
							)
						})}
					</nav>
				</div>
				<div className="flex items-center gap-2">
					{session?.authenticated ? (
						<>
							<span className="hidden items-center gap-2 rounded-md px-3 py-2 text-sm text-muted-foreground md:flex">
								<User className="size-4" />
								<span>{displayName}</span>
							</span>
							<a
								href="/auth/logout"
								className="flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
							>
								<LogOut className="size-4" />
								<span className="hidden sm:inline">Sign out</span>
							</a>
						</>
					) : session?.auth_enabled ? (
						<a
							href="/auth/login"
							className="flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
						>
							<LogIn className="size-4" />
							<span className="hidden sm:inline">Sign in</span>
						</a>
					) : null}
					<a
						href={GITHUB_REPO_URL}
						target="_blank"
						rel="noreferrer"
						className="flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
						aria-label="Open rust-rag on GitHub"
					>
						<Github className="size-4" />
						<span className="hidden sm:inline">GitHub</span>
					</a>
					<ThemeToggle />
				</div>
			</div>
		</header>
	)
}
