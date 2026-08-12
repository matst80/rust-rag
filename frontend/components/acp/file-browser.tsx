"use client"

import { useEffect, useMemo, useState } from "react"
import CodeMirror from "@uiw/react-codemirror"
import { css } from "@codemirror/lang-css"
import { html } from "@codemirror/lang-html"
import { javascript } from "@codemirror/lang-javascript"
import { json } from "@codemirror/lang-json"
import { markdown } from "@codemirror/lang-markdown"
import { python } from "@codemirror/lang-python"
import { rust } from "@codemirror/lang-rust"
import { oneDark } from "@codemirror/theme-one-dark"
import { FileCode2, Folder, FolderSearch, Loader2, Search, X } from "lucide-react"
import { cn } from "@/lib/utils"
import { FileBrowserState } from "./types"

interface FileBrowserProps {
  hostKey: string
  hostLabel: string
  initialDirectory: string
  state: FileBrowserState
  onListDirectories: (query: string, startDirectory: string) => boolean
  onFindFiles: (query: string, startDirectory: string) => boolean
  onReadFile: (path: string) => boolean
  onClose: () => void
}

function extensionsForPath(path: string) {
  const extension = path.toLowerCase().split(".").pop() ?? ""
  if (extension === "json") return [json()]
  if (["js", "jsx", "ts", "tsx"].includes(extension)) {
    return [javascript({ jsx: true, typescript: ["ts", "tsx"].includes(extension) })]
  }
  if (["md", "mdx"].includes(extension)) return [markdown()]
  if (["html", "htm"].includes(extension)) return [html()]
  if (extension === "css") return [css()]
  if (extension === "py") return [python()]
  if (extension === "rs") return [rust()]
  return []
}

export function FileBrowser({
  hostKey,
  hostLabel,
  initialDirectory,
  state,
  onListDirectories,
  onFindFiles,
  onReadFile,
  onClose,
}: FileBrowserProps) {
  const [directoryQuery, setDirectoryQuery] = useState(initialDirectory || "/")
  const [fileQuery, setFileQuery] = useState("")

  useEffect(() => {
    setDirectoryQuery(initialDirectory || "/")
    setFileQuery("")
  }, [hostKey, initialDirectory])

  useEffect(() => {
    if (!state.directories.length && !state.files.length && !state.file && !state.loading) {
      onListDirectories(initialDirectory || "/", initialDirectory || "/")
    }
    // Load the initial directory once per remote host.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hostKey])

  const extensions = useMemo(
    () => (state.file ? extensionsForPath(state.file.path) : []),
    [state.file],
  )

  return (
    <aside className="absolute inset-y-0 right-0 z-30 flex w-[min(100%,30rem)] min-w-0 flex-col border-l border-border bg-background shadow-2xl">
      <header className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2.5">
        <FolderSearch className="size-4 shrink-0 text-primary" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-semibold">Remote files</div>
          <div className="truncate font-mono text-[9px] text-muted-foreground">{hostLabel}</div>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/40 hover:text-foreground"
          aria-label="Close file browser"
        >
          <X className="size-4" />
        </button>
      </header>

      <div className="flex shrink-0 flex-col gap-2 border-b border-border p-3">
        <form
          className="flex gap-1.5"
          onSubmit={(event) => {
            event.preventDefault()
            onListDirectories(directoryQuery, directoryQuery)
          }}
        >
          <div className="relative min-w-0 flex-1">
            <Folder className="pointer-events-none absolute left-2 top-2 size-3.5 text-muted-foreground" />
            <input
              value={directoryQuery}
              onChange={(event) => setDirectoryQuery(event.target.value)}
              className="h-8 w-full border border-border bg-muted/20 pl-7 pr-2 font-mono text-[11px] outline-none focus:border-primary/60"
              placeholder="Directory path"
              aria-label="Directory path"
            />
          </div>
          <button
            type="submit"
            className="flex h-8 items-center gap-1 rounded-md border border-border px-2 font-mono text-[10px] uppercase tracking-wider text-muted-foreground hover:bg-muted/40 hover:text-foreground"
          >
            {state.loading === "directories" ? <Loader2 className="size-3 animate-spin" /> : "Browse"}
          </button>
        </form>

        <form
          className="flex gap-1.5"
          onSubmit={(event) => {
            event.preventDefault()
            onFindFiles(fileQuery, directoryQuery)
          }}
        >
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-2 top-2 size-3.5 text-muted-foreground" />
            <input
              value={fileQuery}
              onChange={(event) => setFileQuery(event.target.value)}
              className="h-8 w-full border border-border bg-muted/20 pl-7 pr-2 font-mono text-[11px] outline-none focus:border-primary/60"
              placeholder="Find files…"
              aria-label="Find files"
            />
          </div>
          <button
            type="submit"
            className="flex h-8 items-center gap-1 rounded-md border border-border px-2 font-mono text-[10px] uppercase tracking-wider text-muted-foreground hover:bg-muted/40 hover:text-foreground"
          >
            {state.loading === "files" ? <Loader2 className="size-3 animate-spin" /> : "Find"}
          </button>
        </form>
        {state.error && <p className="text-[10px] text-red-500">{state.error}</p>}
      </div>

      <div className="grid min-h-0 flex-1 grid-rows-[minmax(8rem,35%)_minmax(0,1fr)]">
        <div className="min-h-0 overflow-y-auto border-b border-border p-2">
          <div className="mb-1 px-1 font-mono text-[9px] font-bold uppercase tracking-wider text-muted-foreground/60">
            {state.files.length > 0 ? "Files" : "Directories"}
          </div>
          {state.files.length > 0 ? (
            <ul className="space-y-0.5">
              {state.files.map((path) => (
                <li key={path}>
                  <button
                    type="button"
                    onClick={() => onReadFile(path)}
                    className={cn(
                      "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left font-mono text-[10px] hover:bg-muted/40",
                      state.selectedPath === path ? "bg-primary/10 text-primary" : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    <FileCode2 className="size-3 shrink-0" />
                    <span className="truncate">{path}</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : state.directories.length > 0 ? (
            <ul className="space-y-0.5">
              {state.directories.map(({ path }) => (
                <li key={path}>
                  <button
                    type="button"
                    onClick={() => {
                      setDirectoryQuery(path)
                      onListDirectories(path, path)
                    }}
                    className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left font-mono text-[10px] text-muted-foreground hover:bg-muted/40 hover:text-foreground"
                  >
                    <Folder className="size-3 shrink-0 text-amber-500" />
                    <span className="truncate">{path}</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <div className="px-2 py-5 text-center text-[10px] text-muted-foreground/60">
              Search a directory or files on this host.
            </div>
          )}
        </div>

        <div className="min-h-0 overflow-hidden bg-[#282c34]">
          {state.file ? (
            <div className="flex h-full min-h-0 flex-col">
              <div className="flex shrink-0 items-center gap-2 border-b border-white/10 bg-[#21252b] px-3 py-2">
                <FileCode2 className="size-3.5 text-primary" />
                <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-white/80">{state.file.path}</span>
                <span className="shrink-0 font-mono text-[9px] text-white/40">{state.file.totalLines} lines</span>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                <CodeMirror
                  value={state.file.content}
                  height="100%"
                  theme={oneDark}
                  extensions={extensions}
                  editable={false}
                  basicSetup={{ lineNumbers: true, foldGutter: true, highlightActiveLine: false }}
                  className="h-full text-[12px]"
                />
              </div>
            </div>
          ) : (
            <div className="flex h-full items-center justify-center px-8 text-center text-xs text-white/40">
              Select a file to preview it here.
            </div>
          )}
        </div>
      </div>
    </aside>
  )
}
