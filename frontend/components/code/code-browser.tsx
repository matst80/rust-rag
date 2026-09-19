"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
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

export function CodeBrowser() {
  const [repos, setRepos] = useState<CodeRepoSummary[]>([])
  const [selectedRepo, setSelectedRepo] = useState<string | null>(null)
  const [files, setFiles] = useState<CodeFileMeta[]>([])
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [detail, setDetail] = useState<CodeFileDetail | null>(null)
  const [query, setQuery] = useState("")
  const [hits, setHits] = useState<CodeSearchHit[] | null>(null)
  const [filter, setFilter] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [uploadRepoName, setUploadRepoName] = useState("")
  const [uploading, setUploading] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const refreshRepos = useCallback(async () => {
    try {
      const r = await listCodeRepos()
      setRepos(r)
      if (!selectedRepo && r.length > 0) setSelectedRepo(r[0].name)
    } catch (e) {
      setError(String(e))
    }
  }, [selectedRepo])

  useEffect(() => {
    refreshRepos()
  }, [refreshRepos])

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

  const onSearch = useCallback(async () => {
    if (!query.trim()) return
    setBusy(true)
    setError(null)
    try {
      const out = await searchCode({
        query,
        repo: selectedRepo ?? undefined,
        limit: 20,
      })
      setHits(out)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }, [query, selectedRepo])

  const onDeleteRepo = useCallback(
    async (name: string) => {
      if (!confirm(`Delete code repo "${name}" and all its files+chunks?`)) return
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
      setSelectedRepo((prev) => prev ?? name)
      await refreshRepos()
    } catch (e) {
      setError(String(e))
    } finally {
      setUploading(false)
    }
  }, [uploadRepoName, refreshRepos])

  return (
    <div className="space-y-6">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">Code Search</h1>
        <p className="text-sm text-muted-foreground">
          Snapshots from each repo&apos;s{" "}
          <code className="rounded bg-muted px-1 py-0.5">.codesearch/codesearch.db</code>{" "}
          (sqlite-vec, all-MiniLM-L6-v2, 384-dim float32) merged into Postgres.
          Queries are re-embedded server-side with the same MiniLM model.
        </p>
      </header>

      {error && (
        <div className="rounded border border-red-300 bg-red-50 p-3 text-sm text-red-900">
          {error}
          <button
            className="ml-2 underline"
            onClick={() => setError(null)}
          >
            dismiss
          </button>
        </div>
      )}

      <UploadPanel
        repoName={uploadRepoName}
        onRepoNameChange={setUploadRepoName}
        uploading={uploading}
        onUpload={onUpload}
        fileInputRef={fileInputRef}
      />

      <section className="space-y-2">
        <h2 className="text-lg font-medium">Repos</h2>
        {repos.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No repos indexed yet. Upload a snapshot below.
          </p>
        ) : (
          <div className="grid grid-cols-1 gap-2 md:grid-cols-2 lg:grid-cols-3">
            {repos.map((r) => (
              <button
                key={r.name}
                onClick={() => setSelectedRepo(r.name)}
                className={`rounded border p-3 text-left transition ${
                  selectedRepo === r.name
                    ? "border-primary bg-primary/5"
                    : "border-border hover:bg-muted/50"
                }`}
              >
                <div className="flex items-center justify-between">
                  <span className="font-mono text-sm">{r.name}</span>
                  <span className="text-xs text-muted-foreground">{r.file_count} files</span>
                </div>
                <div className="mt-1 truncate text-xs text-muted-foreground">{r.root_path}</div>
                <button
                  className="mt-2 text-xs text-red-600 hover:underline"
                  onClick={(e) => {
                    e.stopPropagation()
                    onDeleteRepo(r.name)
                  }}
                >
                  delete
                </button>
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="space-y-2">
        <h2 className="text-lg font-medium">Semantic search</h2>
        <div className="flex gap-2">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && onSearch()}
            placeholder={
              selectedRepo
                ? `Search in ${selectedRepo}...`
                : "Pick a repo above first or search across all..."
            }
            className="flex-1 rounded border px-3 py-2 text-sm"
          />
          <button
            disabled={busy || !query.trim()}
            onClick={onSearch}
            className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground disabled:opacity-50"
          >
            {busy ? "..." : "Search"}
          </button>
        </div>
        {hits && hits.length === 0 && (
          <p className="text-sm text-muted-foreground">No hits.</p>
        )}
        {hits && hits.length > 0 && (
          <ol className="space-y-2">
            {hits.map((h, i) => (
              <li key={i} className="rounded border p-3">
                <div className="flex items-baseline justify-between gap-2">
                  <div className="font-mono text-sm">
                    <span className="text-muted-foreground">{h.repo}/</span>
                    {h.path}
                    <span className="ml-2 text-xs text-muted-foreground">
                      L{h.start_line}–{h.end_line}
                    </span>
                  </div>
                  <span className="text-xs text-muted-foreground">{h.score.toFixed(3)}</span>
                </div>
                {h.symbol_name && (
                  <div className="text-xs text-muted-foreground">
                    {h.symbol_kind} <span className="font-mono">{h.symbol_name}</span>
                  </div>
                )}
                {h.signature && (
                  <pre className="mt-1 truncate text-xs font-mono">{h.signature}</pre>
                )}
                <pre className="mt-2 whitespace-pre-wrap rounded bg-muted/50 p-2 text-xs font-mono">
                  {h.snippet}
                </pre>
              </li>
            ))}
          </ol>
        )}
      </section>

      {selectedRepo && (
        <section className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-medium">Files in {selectedRepo}</h2>
              <input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="filter…"
                className="rounded border px-2 py-1 text-xs"
              />
            </div>
            <div className="max-h-[60vh] overflow-y-auto rounded border">
              {filteredFiles.map((f) => (
                <button
                  key={f.path}
                  onClick={() => setSelectedFile(f.path)}
                  className={`block w-full border-b px-3 py-2 text-left font-mono text-xs hover:bg-muted/50 ${
                    selectedFile === f.path ? "bg-primary/10" : ""
                  }`}
                >
                  <div className="truncate">{f.path}</div>
                  <div className="mt-0.5 flex gap-2 text-[10px] text-muted-foreground">
                    <span>{f.language ?? "?"}</span>
                    <span>{f.role ?? "?"}</span>
                    <span>{f.line_count} lines</span>
                  </div>
                </button>
              ))}
            </div>
          </div>
          <div className="space-y-2">
            <h2 className="text-lg font-medium">File detail</h2>
            {!detail && (
              <p className="text-sm text-muted-foreground">Pick a file on the left.</p>
            )}
            {detail && <FileDetailPanel detail={detail} />}
          </div>
        </section>
      )}
    </div>
  )
}

function FileDetailPanel({ detail }: { detail: CodeFileDetail }) {
  return (
    <div className="space-y-3 rounded border p-3">
      <div>
        <div className="font-mono text-sm">{detail.path}</div>
        <div className="mt-1 flex gap-2 text-xs text-muted-foreground">
          <span>{detail.language ?? "?"}</span>
          <span>{detail.role ?? "?"}</span>
          <span>{detail.line_count} lines</span>
          <span>{Math.round(detail.size_bytes / 1024)} KB</span>
        </div>
      </div>

      {detail.summary && (
        <div>
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Summary</h3>
          <p className="mt-1 whitespace-pre-wrap text-sm">{detail.summary}</p>
        </div>
      )}

      {detail.outline.length > 0 && (
        <div>
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            Outline ({detail.outline.length})
          </h3>
          <ul className="mt-1 space-y-1 font-mono text-xs">
            {detail.outline.map((o, i) => (
              <li key={i} className="flex gap-2">
                <span className="text-muted-foreground">L{o.line}</span>
                <span className="text-muted-foreground">{o.kind}</span>
                <span>{o.name}</span>
                {o.is_public && <span className="text-green-600">pub</span>}
                {o.is_test && <span className="text-amber-600">test</span>}
              </li>
            ))}
          </ul>
        </div>
      )}

      {detail.imports.length > 0 && (
        <div>
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            Imports ({detail.imports.length})
          </h3>
          <ul className="mt-1 max-h-32 overflow-y-auto font-mono text-xs">
            {detail.imports.map((m, i) => (
              <li key={i}>{m}</li>
            ))}
          </ul>
        </div>
      )}

      {detail.todos.length > 0 && (
        <div>
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            TODOs ({detail.todos.length})
          </h3>
          <ul className="mt-1 space-y-1 font-mono text-xs">
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

function UploadPanel({
  repoName,
  onRepoNameChange,
  uploading,
  onUpload,
  fileInputRef,
}: {
  repoName: string
  onRepoNameChange: (v: string) => void
  uploading: boolean
  onUpload: () => void
  fileInputRef: React.RefObject<HTMLInputElement | null>
}) {
  const [hasFile, setHasFile] = useState(false)
  return (
    <section className="space-y-2">
      <h2 className="text-lg font-medium">Upload repo snapshot</h2>
      <p className="text-xs text-muted-foreground">
        Push a repo&apos;s <code className="rounded bg-muted px-1 py-0.5">.codesearch/codesearch.db</code>{" "}
        here — the server reads the symbols + embeddings and replaces that
        repo&apos;s rows in Postgres. Re-uploading the same name refreshes it.
      </p>
      <div className="flex flex-wrap gap-2">
        <input
          value={repoName}
          onChange={(e) => onRepoNameChange(e.target.value)}
          placeholder="repo name, e.g. sync-up"
          className="w-48 rounded border px-3 py-2 text-sm"
        />
        <input
          ref={fileInputRef}
          type="file"
          accept=".db,application/octet-stream"
          onChange={(e) => setHasFile(!!e.target.files?.[0])}
          className="rounded border px-3 py-2 text-sm file:mr-2 file:rounded file:border-0 file:bg-muted file:px-2 file:py-1 file:text-xs"
        />
        <button
          disabled={uploading || !repoName.trim() || !hasFile}
          onClick={onUpload}
          className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground disabled:opacity-50"
        >
          {uploading ? "Uploading…" : "Upload"}
        </button>
      </div>
      <details className="text-xs text-muted-foreground">
        <summary className="cursor-pointer">…or upload from the CLI</summary>
        <pre className="mt-2 overflow-x-auto rounded bg-background p-2 text-xs">
{`curl -sf -X POST "$RAG_URL/api/code/upload" \\
  -H "Authorization: Bearer $RAG_TOKEN" \\
  -F repo=myrepo \\
  -F root_path=/path/to/repo \\
  -F file=@/path/to/repo/.codesearch/codesearch.db`}
        </pre>
      </details>
    </section>
  )
}
