import Link from "next/link"
import { ChevronRight, ExternalLink } from "lucide-react"
import { AppHeader } from "@/components/app-header"
import { MarkdownView } from "@/components/entries/markdown-view"
import { cn } from "@/lib/utils"

type DocsPageLink = {
  title: string
  href: string
  description: string
}

type DocsResource = {
  label: string
  href: string
  external?: boolean
}

const docsLinks: DocsPageLink[] = [
  {
    title: "Start Guide",
    href: "/start-guide",
    description: "Routes, search flow, and product overview.",
  },
  {
    title: "MCP Setup",
    href: "/mcp-setup",
    description: "Bridge binaries, env vars, and client configuration.",
  },
]

interface DocsShellProps {
  title: string
  eyebrow: string
  description: string
  markdown: string
  pathname: string
  resources?: DocsResource[]
}

export function DocsShell({
  title,
  eyebrow,
  description,
  markdown,
  pathname,
  resources = [],
}: DocsShellProps) {
  return (
    <>
      <AppHeader />
      <main className="min-h-[calc(100vh-3.5rem)] bg-[linear-gradient(180deg,rgba(0,0,0,0.02),transparent_220px)]">
        <div className="mx-auto grid w-full max-w-[1400px] gap-0 lg:grid-cols-[260px_minmax(0,1fr)_220px]">
          <aside className="hidden border-r border-border/60 px-6 py-10 lg:block">
            <div className="sticky top-24 space-y-8">
              <div className="space-y-2">
                <p className="text-[11px] font-black uppercase tracking-[0.28em] text-primary/70">
                  Guides
                </p>
                <h2 className="text-sm font-semibold text-foreground/90">
                  rust-rag documentation
                </h2>
              </div>

              <nav className="space-y-1.5">
                {docsLinks.map((link) => {
                  const active = pathname === link.href
                  return (
                    <Link
                      key={link.href}
                      href={link.href}
                      className={cn(
                        "block rounded-xl border px-4 py-3 transition-colors",
                        active
                          ? "border-primary/20 bg-primary/5"
                          : "border-transparent text-muted-foreground hover:border-border/60 hover:bg-muted/40 hover:text-foreground"
                      )}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-sm font-semibold">{link.title}</span>
                        {active ? <ChevronRight className="size-4 text-primary" /> : null}
                      </div>
                      <p className="mt-1 text-xs leading-5">{link.description}</p>
                    </Link>
                  )
                })}
              </nav>
            </div>
          </aside>

          <section className="min-w-0 px-6 py-10 lg:px-12 xl:px-16">
            <div className="mx-auto max-w-4xl">
              <div className="mb-8 grid gap-3 lg:hidden">
                {docsLinks.map((link) => {
                  const active = pathname === link.href
                  return (
                    <Link
                      key={link.href}
                      href={link.href}
                      className={cn(
                        "rounded-2xl border px-4 py-3 transition-colors",
                        active
                          ? "border-primary/20 bg-primary/5"
                          : "border-border/60 bg-card hover:bg-muted/40"
                      )}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-sm font-semibold">{link.title}</span>
                        <ChevronRight className="size-4 text-muted-foreground" />
                      </div>
                      <p className="mt-1 text-xs leading-5 text-muted-foreground">
                        {link.description}
                      </p>
                    </Link>
                  )
                })}
              </div>

              <div className="border-b border-border/60 pb-8">
                <p className="text-[11px] font-black uppercase tracking-[0.28em] text-primary/70">
                  {eyebrow}
                </p>
                <h1 className="mt-3 text-4xl font-extrabold tracking-tight text-foreground sm:text-5xl">
                  {title}
                </h1>
                <p className="mt-4 max-w-2xl text-base leading-7 text-muted-foreground">
                  {description}
                </p>
              </div>

              <div className="pt-10">
                <MarkdownView
                  content={markdown}
                  className="prose prose-slate max-w-none prose-headings:scroll-mt-24 prose-headings:font-semibold prose-h1:text-4xl prose-h2:mt-12 prose-h2:border-t prose-h2:pt-8 prose-code:rounded prose-code:bg-muted prose-code:px-1 prose-code:py-0.5 prose-pre:rounded-2xl prose-pre:border prose-pre:border-border/60 prose-pre:bg-[#111827] prose-pre:px-5 prose-pre:py-4 dark:prose-invert"
                />
              </div>
            </div>
          </section>

          <aside className="hidden border-l border-border/60 px-6 py-10 xl:block">
            <div className="sticky top-24 space-y-4">
              <p className="text-[11px] font-black uppercase tracking-[0.28em] text-muted-foreground">
                Resources
              </p>
              <div className="space-y-2">
                {resources.map((resource) => (
                  <a
                    key={resource.href}
                    href={resource.href}
                    target={resource.external ? "_blank" : undefined}
                    rel={resource.external ? "noreferrer" : undefined}
                    className="flex items-center justify-between gap-3 rounded-xl border border-transparent px-3 py-2 text-sm text-muted-foreground transition-colors hover:border-border/60 hover:bg-muted/40 hover:text-foreground"
                  >
                    <span>{resource.label}</span>
                    {resource.external ? <ExternalLink className="size-3.5" /> : null}
                  </a>
                ))}
              </div>
            </div>
          </aside>
        </div>
      </main>
    </>
  )
}