"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useSearchParams } from "next/navigation"
import {
  Braces,
  ChevronDown,
  Code2,
  FileCode,
  FolderOpen,
  ListTree,
  LoaderCircle,
  Network,
  ScanSearch,
  Search,
  Trash2,
  Upload,
} from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  deleteCodeRepo,
  getCodeFileDetail,
  listCodeFiles,
  listCodeRepos,
  searchCode,
  uploadCodeRepo,
} from "@/lib/api/client"
import type {
  CodeFileDetail,
  CodeFileMeta,
  CodeRepoSummary,
  CodeSearchHit,
} from "@/lib/api/types"
import { cn } from "@/lib/utils"
import { ScoreBadge, SymbolGraphSheet, type SymbolGraphTarget } from "./code-symbol-graph"

function basename(path: string): string {
  return path.split("/").pop() ?? path
}

export function CodeBrowser() {
  const searchParams = useSearchParams()
  const urlRepo = searchParams.get("repo")
  const urlQuery = searchParams.get("q")
  const urlFile = searchParams.get("file")
  const [repos, setRepos] = useState<CodeRepoSummary[]>([])
  const [reposLoaded, setReposLoaded] = useState(false)
  const [selectedRepo, setSelectedRepo] = useState<string | null>(null)
  const [files, setFiles] = useState<CodeFileMeta[]>([])
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [detail, setDetail] = useState<CodeFileDetail | null>(null)
  const [query, setQuery] = useState("")
  const [hits, setHits] = useState<CodeSearchHit[] | null>(null)
  const [filter, setFilter] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<"search" | "files">("search")
  const [graphTarget, setGraphTarget] = useState<(SymbolGraphTarget & { repo: string }) | null>(null)
  const [uploadOpen, setUploadOpen] = useState(false)
  const [uploadRepoName, setUploadRepoName] = useState("")
  const [uploadHasFile, setUploadHasFile] = useState(false)
  const [uploading, setUploading] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)
  // Query the current hits were produced with — drives the shareable URL.
  const lastSearched = useRef("")

  const refreshRepos = useCallback(async () => {
    try {
      setRepos(await listCodeRepos())
    } catch (e) {
      setError(String(e))
    } finally {
      setReposLoaded(true)
    }
  }, [])

  useEffect(() => {
    refreshRepos()
  }, [refreshRepos])

  // Auto-pick a scope once, so the page is usable immediately — unless the URL
  // names a repo or query, in which case the deep-link effect decides the scope.
  useEffect(() => {
    if (urlRepo || urlQuery) return
    setSelectedRepo((current) => current ?? repos[0]?.name ?? null)
  }, [repos, urlRepo, urlQuery])

  // Apply ?repo= / ?q= / ?file= deep links (from entries etc.) once, after the
  // repo list is known — an unknown repo falls back to all-repos scope.
  const repoMissing = !!urlRepo && !repos.some((r) => r.name === urlRepo)
  useEffect(() => {
    if (!reposLoaded) return
    let repo: string | null = null
    if (urlRepo) {
      repo = repos.some((r) => r.name === urlRepo) ? urlRepo : null
      setSelectedRepo(repo)
    }
    if (urlFile && repo) {
      setSelectedFile(urlFile)
      setTab("files")
    }
    if (urlQuery) {
      setQuery(urlQuery)
      lastSearched.current = urlQuery
      setBusy(true)
      searchCode({ query: urlQuery, repo: repo ?? undefined, limit: 20 })
        .then(setHits)
        .catch((e) => setError(String(e)))
        .finally(() => setBusy(false))
    }
    // Deep links only need to apply once per page load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reposLoaded])

  // Keep the URL in sync with the current scope/search so views stay shareable.
  useEffect(() => {
    if (!reposLoaded) return
    const params = new URLSearchParams()
    if (selectedRepo) params.set("repo", selectedRepo)
    if (lastSearched.current) params.set("q", lastSearched.current)
    if (tab === "files" && selectedFile) params.set("file", selectedFile)
    const qs = params.toString()
    window.history.replaceState(null, "", qs ? `/code?${qs}` : "/code")
  }, [reposLoaded, selectedRepo, selectedFile, tab, hits])

  useEffect(() => {
    if (!selectedRepo) return
    setFiles([])
    setSelectedFile(null)
    setDetail(null)
    listCodeFiles(selectedRepo)
      .then(setFiles)
      .catch((e) => setError(String(e)))
  }, [selectedRepo])

  useEffect(() => {
    if (!selectedRepo || !selectedFile) {
      setDetail(null)
      return
    }
    getCodeFileDetail(selectedRepo, selectedFile)
      .then(setDetail)
      .catch((e) => setError(String(e)))
  }, [selectedRepo, selectedFile])

  const filteredFiles = useMemo(() => {
    if (!filter.trim()) return files
    const q = filter.toLowerCase()
    return files.filter(
      (f) => f.path.toLowerCase().includes(q) || f.basename.toLowerCase().includes(q)
    )
  }, [files, filter])

  const totalFiles = useMemo(() => repos.reduce((n, r) => n + r.file_count, 0), [repos])

  const onSearch = useCallback(async () => {
    if (!query.trim()) return
    setBusy(true)
    setError(null)
    lastSearched.current = query.trim()
    try {
      setHits(
        await searchCode({
          query,
          repo: selectedRepo ?? undefined,
          limit: 20,
        })
      )
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }, [query, selectedRepo])

  const onDeleteRepo = useCallback(
    async (name: string) => {
      if (!confirm(`Delete code repo "${name}" and all its files+symbols?`)) return
      try {
        await deleteCodeRepo(name)
        if (selectedRepo === name) setSelectedRepo(null)
        refreshRepos()
      } catch (e) {
        setError(String(e))
      }
    },
    [selectedRepo, refreshRepos]
  )

  const onUpload = useCallback(async () => {
    const file = fileInputRef.current?.files?.[0]
    const name = uploadRepoName.trim()
    if (!file || !name) return
    setUploading(true)
    setError(null)
    try {
      await uploadCodeRepo(name, file)
      setUploadRepoName("")
      if (fileInputRef.current) fileInputRef.current.value = ""
      setUploadHasFile(false)
      setUploadOpen(false)
      setSelectedRepo(name)
      await refreshRepos()
    } catch (e) {
      setError(String(e))
    } finally {
      setUploading(false)
    }
  }, [uploadRepoName, refreshRepos])

  /** Open a file in the Files tab (from search hits or the graph sheet). */
  const openFile = useCallback((repo: string, path: string) => {
    setGraphTarget(null)
    setSelectedRepo(repo)
    setSelectedFile(path)
    setTab("files")
  }, [])

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <h1 className="flex items-center gap-2.5 text-2xl font-semibold tracking-tight">
            <span className="flex size-9 items-center justify-center rounded-xl bg-primary/10 text-primary">
              <Code2 className="size-5" />
            </span>
            Code Search
          </h1>
          <p className="text-sm text-muted-foreground">
            Semantic symbol search across indexed repos — with call-graph
            connections, powered by all-MiniLM-L6-v2.
          </p>
        </div>

        <Dialog open={uploadOpen} onOpenChange={setUploadOpen}>
          <DialogTrigger asChild>
            <Button variant="outline" className="rounded-xl">
              <Upload className="size-4" />
              Upload snapshot
            </Button>
          </DialogTrigger>
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle>Upload repo snapshot</DialogTitle>
              <DialogDescription>
                Push a repo&apos;s{" "}
                <code className="rounded bg-muted px-1 py-0.5">.codesearch/codesearch.db</code>{" "}
                here — symbols, embeddings and the call graph merge into
                Postgres. Re-uploading the same name replaces it.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-3">
              <Input
                value={uploadRepoName}
                onChange={(e) => setUploadRepoName(e.target.value)}
                placeholder="repo name, e.g. sync-up"
                className="rounded-xl"
              />
              <Input
                ref={fileInputRef}
                type="file"
                accept=".db,application/octet-stream"
                onChange={(e) => setUploadHasFile(!!e.target.files?.[0])}
                className="rounded-xl file:mr-2 file:rounded-md file:border-0 file:bg-muted file:px-2 file:py-1 file:text-xs"
              />
              <details className="text-xs text-muted-foreground">
                <summary className="cursor-pointer">…or upload from the CLI</summary>
                <pre className="mt-2 overflow-x-auto rounded-lg bg-muted/50 p-2.5 text-[11px]">
{`curl -sf -X POST "$RAG_URL/api/code/upload" \\
  -H "Authorization: Bearer $RAG_TOKEN" \\
  -F repo=myrepo \\
  -F root_path=/path/to/repo \\
  -F file=@/path/to/repo/.codesearch/codesearch.db`}
                </pre>
              </details>
            </div>
            <DialogFooter>
              <Button
                onClick={onUpload}
                disabled={uploading || !uploadRepoName.trim() || !uploadHasFile}
                className="w-full rounded-xl sm:w-auto"
              >
                {uploading ? (
                  <LoaderCircle className="size-4 animate-spin" />
                ) : (
                  <Upload className="size-4" />
                )}
                {uploading ? "Uploading…" : "Upload"}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      {error && (
        <Alert variant="destructive" className="flex items-start justify-between gap-4">
          <AlertDescription className="break-all text-xs">{error}</AlertDescription>
          <button className="shrink-0 text-xs underline" onClick={() => setError(null)}>
            dismiss
          </button>
        </Alert>
      )}

      {/* Scope chips */}
      {repos.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <ScopeChip
            label="All repos"
            active={selectedRepo === null}
            onClick={() => setSelectedRepo(null)}
          />
          {repos.map((r) => (
            <ScopeChip
              key={r.name}
              label={r.name}
              count={r.file_count}
              active={selectedRepo === r.name}
              onClick={() => setSelectedRepo(r.name)}
              onDelete={() => onDeleteRepo(r.name)}
            />
          ))}
        </div>
      )}

      {urlRepo && repoMissing && reposLoaded && (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <ScanSearch className="size-3.5" />
          Repo <span className="font-mono text-foreground/80">{urlRepo}</span> is
          not indexed — searching across all repos instead.
        </p>
      )}

      {/* Search bar */}
      <Card className="border-primary/10 bg-card/60 backdrop-blur-sm">
        <CardContent className="space-y-2 p-4">
          <div className="flex flex-col gap-2 sm:flex-row">
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground/40" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && onSearch()}
                placeholder={
                  selectedRepo
                    ? `Search symbols in ${selectedRepo}…`
                    : "Search symbols across all repos…"
                }
                className="h-10 rounded-xl pl-9"
              />
            </div>
            <Button
              onClick={onSearch}
              disabled={busy || !query.trim()}
              className="h-10 rounded-xl px-6 font-bold uppercase tracking-widest"
            >
              {busy ? <LoaderCircle className="size-4 animate-spin" /> : <Search className="size-4" />}
              Search
            </Button>
          </div>
          <div className="flex items-center gap-4 text-[9px] font-bold uppercase tracking-[0.2em] text-muted-foreground/40">
            <span>{repos.length} repos</span>
            <span>{totalFiles} files indexed</span>
            <span>384-dim · MiniLM</span>
          </div>
        </CardContent>
      </Card>

      <Tabs value={tab} onValueChange={(v) => setTab(v as "search" | "files")}>
        <TabsList className="rounded-xl">
          <TabsTrigger value="search" className="rounded-lg text-[10px] font-bold uppercase tracking-widest">
            <ScanSearch className="size-3.5" />
            Results
            {hits && (
              <span className="ml-1.5 text-muted-foreground/60">{hits.length}</span>
            )}
          </TabsTrigger>
          <TabsTrigger
            value="files"
            disabled={!selectedRepo}
            className="rounded-lg text-[10px] font-bold uppercase tracking-widest"
          >
            <FolderOpen className="size-3.5" />
            Files{selectedRepo ? ` · ${selectedRepo}` : ""}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="search" className="mt-4 space-y-3">
          {hits === null ? (
            <EmptyState
              icon={<ScanSearch className="size-10" />}
              title="No search yet"
              hint="Type a natural-language query — “embedder startup”, “k8s rollout”, “parse snapshot” — and hit Enter."
            />
          ) : hits.length === 0 ? (
            <EmptyState
              icon={<Search className="size-10" />}
              title="No hits"
              hint="Nothing matched that query in the current scope."
            />
          ) : (
            hits.map((hit, i) => (
              <SearchHitCard
                key={`${hit.repo}/${hit.path}/${hit.start_line}/${i}`}
                hit={hit}
                onConnections={
                  hit.symbol_name
                    ? () =>
                      setGraphTarget({
                        repo: hit.repo,
                        path: hit.path,
                        symbol: hit.symbol_name!,
                      })
                    : undefined
                }
                onViewFile={() => openFile(hit.repo, hit.path)}
              />
            ))
          )}
        </TabsContent>

        <TabsContent value="files" className="mt-4">
          {selectedRepo ? (
            <div className="grid gap-4 lg:grid-cols-[340px_1fr]">
              <Card className="self-start">
                <CardContent className="p-3">
                  <Input
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                    placeholder="filter paths…"
                    className="mb-2 h-8 rounded-lg text-xs"
                  />
                  <div className="max-h-[62vh] space-y-0.5 overflow-y-auto scrollbar-thin">
                    {filteredFiles.map((f) => (
                      <button
                        key={f.path}
                        onClick={() => setSelectedFile(f.path)}
                        className={cn(
                          "block w-full rounded-lg px-2.5 py-2 text-left transition-colors",
                          selectedFile === f.path
                            ? "bg-primary/10 text-primary"
                            : "hover:bg-muted/60"
                        )}
                      >
                        <div className="truncate font-mono text-xs">{f.path}</div>
                        <div className="mt-0.5 flex gap-2 text-[10px] text-muted-foreground/60">
                          <span>{f.language ?? "?"}</span>
                          <span>{f.line_count} lines</span>
                        </div>
                      </button>
                    ))}
                    {filteredFiles.length === 0 && (
                      <p className="px-2 py-6 text-center text-xs text-muted-foreground/50">
                        No files match.
                      </p>
                    )}
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardContent className="p-5">
                  {!detail ? (
                    <EmptyState
                      icon={<FileCode className="size-10" />}
                      title="Pick a file"
                      hint="Select a file on the left to see its summary, outline and symbols."
                    />
                  ) : (
                    <FileDetailPanel
                      detail={detail}
                      onSymbolClick={(symbol) =>
                        setGraphTarget({ repo: selectedRepo, path: detail.path, symbol })
                      }
                    />
                  )}
                </CardContent>
              </Card>
            </div>
          ) : (
            <EmptyState
              icon={<FolderOpen className="size-10" />}
              title="No repo selected"
              hint="Pick a repo scope above to browse its files."
            />
          )}
        </TabsContent>
      </Tabs>

      {repos.length === 0 && !error && (
        <EmptyState
          icon={<Upload className="size-10" />}
          title="No repos indexed yet"
          hint="Upload a repo's .codesearch/codesearch.db snapshot to get started."
        />
      )}

      <SymbolGraphSheet
        repo={graphTarget?.repo ?? ""}
        target={graphTarget}
        open={!!graphTarget}
        onOpenChange={(open) => {
          if (!open) setGraphTarget(null)
        }}
        onNavigate={(t) =>
          setGraphTarget((prev) => ({ repo: prev?.repo ?? "", ...t }))
        }
        onViewFile={(path) => {
          if (graphTarget) openFile(graphTarget.repo, path)
        }}
      />
    </div>
  )
}

function ScopeChip({
  label,
  count,
  active,
  onClick,
  onDelete,
}: {
  label: string
  count?: number
  active: boolean
  onClick: () => void
  onDelete?: () => void
}) {
  return (
    <div
      onClick={onClick}
      className={cn(
        "group flex cursor-pointer items-center gap-1.5 rounded-full border px-3 py-1 text-xs transition-all",
        active
          ? "border-primary/40 bg-primary/10 text-primary"
          : "border-border bg-muted/20 text-muted-foreground hover:bg-muted/50"
      )}
    >
      <span className="font-mono font-medium">{label}</span>
      {count !== undefined && (
        <span className="text-[10px] opacity-50">{count}</span>
      )}
      {onDelete && (
        <button
          className="ml-0.5 rounded-full p-0.5 opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100"
          onClick={(e) => {
            e.stopPropagation()
            onDelete()
          }}
          title={`Delete ${label}`}
        >
          <Trash2 className="size-3" />
        </button>
      )}
    </div>
  )
}

function SearchHitCard({
  hit,
  onConnections,
  onViewFile,
}: {
  hit: CodeSearchHit
  onConnections?: () => void
  onViewFile: () => void
}) {
  const [expanded, setExpanded] = useState(false)
  return (
    <Card className="group overflow-hidden transition-all hover:border-primary/20 hover:shadow-md hover:shadow-primary/5">
      <CardContent className="space-y-2.5 p-4">
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <span className="truncate font-mono text-sm font-semibold text-primary">
              {hit.symbol_name ?? basename(hit.path)}
            </span>
            {hit.symbol_kind && (
              <Badge variant="outline" className="text-[9px] uppercase font-bold tracking-wider">
                {hit.symbol_kind}
              </Badge>
            )}
            {hit.language && (
              <Badge variant="secondary" className="text-[9px]">
                {hit.language}
              </Badge>
            )}
          </div>
          <ScoreBadge score={hit.score} />
        </div>

        <button
          onClick={onViewFile}
          className="flex w-full items-center gap-1.5 text-left font-mono text-[11px] text-muted-foreground transition-colors hover:text-primary"
        >
          <FileCode className="size-3 shrink-0" />
          <span className="truncate">
            {hit.repo}/{hit.path}
          </span>
          <span className="shrink-0 text-muted-foreground/50">
            :{hit.start_line}–{hit.end_line}
          </span>
        </button>

        {hit.signature && (
          <pre className="truncate rounded-lg bg-muted/40 px-2.5 py-1.5 font-mono text-[11px] text-foreground/80">
            {hit.signature}
          </pre>
        )}

        <pre
          className={cn(
            "relative whitespace-pre-wrap rounded-lg bg-muted/30 p-3 font-mono text-[11px] leading-relaxed",
            !expanded && "max-h-24 overflow-hidden"
          )}
        >
          {hit.snippet}
        </pre>

        <div className="flex items-center justify-between">
          <div className="flex gap-1.5">
            {hit.symbol_name && onConnections && (
              <Button
                size="sm"
                variant="secondary"
                className="h-7 rounded-lg px-3 text-[10px] font-bold uppercase tracking-widest"
                onClick={onConnections}
              >
                <Network className="mr-1.5 size-3" />
                Connections
              </Button>
            )}
            <Button
              size="sm"
              variant="ghost"
              className="h-7 rounded-lg px-3 text-[10px] font-bold uppercase tracking-widest text-muted-foreground"
              onClick={onViewFile}
            >
              <FileCode className="mr-1.5 size-3" />
              View file
            </Button>
          </div>
          <button
            onClick={() => setExpanded(!expanded)}
            className="flex items-center text-[10px] font-bold uppercase tracking-widest text-muted-foreground/50 transition-colors hover:text-primary"
          >
            {expanded ? "Less" : "More"}
            <ChevronDown className={cn("size-3 transition-transform", expanded && "rotate-180")} />
          </button>
        </div>
      </CardContent>
    </Card>
  )
}

function FileDetailPanel({
  detail,
  onSymbolClick,
}: {
  detail: CodeFileDetail
  onSymbolClick: (symbol: string) => void
}) {
  return (
    <div className="space-y-5">
      <div className="space-y-1.5">
        <div className="font-mono text-sm font-semibold">{detail.basename}</div>
        <div className="font-mono text-[11px] text-muted-foreground/70">{detail.path}</div>
        <div className="flex flex-wrap gap-1.5 pt-0.5">
          {detail.language && <Badge variant="secondary">{detail.language}</Badge>}
          <Badge variant="outline">{detail.line_count} lines</Badge>
          <Badge variant="outline">{Math.round(detail.size_bytes / 1024)} KB</Badge>
          <Badge variant="outline">{detail.outline.length} symbols</Badge>
        </div>
      </div>

      {detail.summary && (
        <div className="space-y-1.5">
          <h3 className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
            Summary
          </h3>
          <p className="whitespace-pre-wrap text-sm leading-relaxed text-foreground/80">
            {detail.summary}
          </p>
        </div>
      )}

      {detail.outline.length > 0 && (
        <div className="space-y-2">
          <h3 className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
            <ListTree className="size-3.5" />
            Outline ({detail.outline.length})
          </h3>
          <div className="space-y-1">
            {detail.outline.map((o, i) => (
              <button
                key={i}
                className="group flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left transition-colors hover:bg-muted/60"
                onClick={() => onSymbolClick(o.name)}
                title="Explore call-graph connections"
              >
                <span className="w-10 shrink-0 text-right font-mono text-[10px] text-muted-foreground/40">
                  L{o.line}
                </span>
                <Badge variant="outline" className="shrink-0 text-[8px] uppercase">
                  {o.kind}
                </Badge>
                <span className="truncate font-mono text-xs font-medium">{o.name}</span>
                {o.is_public && (
                  <span className="shrink-0 text-[9px] font-bold uppercase text-emerald-600">
                    pub
                  </span>
                )}
                {o.is_test && (
                  <span className="shrink-0 text-[9px] font-bold uppercase text-amber-600">
                    test
                  </span>
                )}
                <Network className="ml-auto size-3 shrink-0 text-muted-foreground/0 transition-colors group-hover:text-primary/60" />
              </button>
            ))}
          </div>
        </div>
      )}

      {detail.imports.length > 0 && (
        <div className="space-y-1.5">
          <h3 className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
            Imports ({detail.imports.length})
          </h3>
          <ul className="max-h-32 space-y-0.5 overflow-y-auto font-mono text-xs text-muted-foreground">
            {detail.imports.map((m, i) => (
              <li key={i}>{m}</li>
            ))}
          </ul>
        </div>
      )}

      {detail.todos.length > 0 && (
        <div className="space-y-1.5">
          <h3 className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
            <Braces className="size-3.5" />
            TODOs ({detail.todos.length})
          </h3>
          <ul className="space-y-1 font-mono text-xs">
            {detail.todos.map((t, i) => (
              <li key={i}>
                <span className="text-amber-600">{t.kind}</span>{" "}
                <span className="text-muted-foreground">L{t.line}</span> {t.text}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

function EmptyState({
  icon,
  title,
  hint,
}: {
  icon: React.ReactNode
  title: string
  hint: string
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-14 text-center">
      <div className="text-muted-foreground/30">{icon}</div>
      <div className="space-y-1">
        <p className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
          {title}
        </p>
        <p className="mx-auto max-w-sm text-xs text-muted-foreground/60">{hint}</p>
      </div>
    </div>
  )
}
