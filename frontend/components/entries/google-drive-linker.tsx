"use client"

import { useState } from "react"
import { useDriveSearch, useAttachUrl } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Search, Loader2, Link2, ExternalLink, FileText, Layout, Presentation } from "lucide-react"
import { toast } from "sonner"
import { DriveFile } from "@/lib/api/types"

interface GoogleDriveLinkerProps {
  itemId: string
  onAttached?: () => void
}

export function GoogleDriveLinker({ itemId, onAttached }: GoogleDriveLinkerProps) {
  const [query, setQuery] = useState("")
  const [deferredQuery, setDeferredQuery] = useState("")
  const { data: results, isLoading } = useDriveSearch(deferredQuery)
  const { trigger: attachUrl, isMutating: isAttaching } = useAttachUrl(itemId)

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault()
    setDeferredQuery(query.trim())
  }

  const handleLink = async (file: DriveFile) => {
    if (!file.webViewLink) {
      toast.error("File has no web view link")
      return
    }

    try {
      await attachUrl({
        url: file.webViewLink,
        filename: file.name
      })
      toast.success(`Linked ${file.name}`)
      onAttached?.()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Linking failed")
    }
  }

  const getFileIcon = (mimeType: string) => {
    if (mimeType.includes("document")) return <FileText className="size-4 text-blue-500" />
    if (mimeType.includes("spreadsheet")) return <Layout className="size-4 text-green-500" />
    if (mimeType.includes("presentation")) return <Presentation className="size-4 text-orange-500" />
    return <FileText className="size-4 text-muted-foreground" />
  }

  return (
    <div className="space-y-4 border border-border bg-muted/30 p-4 rounded-lg">
      <div className="flex flex-col gap-1">
        <h3 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
          Link Google Drive File
        </h3>
        <p className="text-[10px] text-muted-foreground/70">
          Search and attach files from your connected Google account.
        </p>
      </div>

      <form onSubmit={handleSearch} className="flex gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input
            placeholder="Search Drive..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="pl-9 h-9 text-xs"
          />
        </div>
        <Button 
          type="submit" 
          variant="secondary" 
          size="sm" 
          className="h-9 px-4 font-mono text-[10px] uppercase tracking-wider"
          disabled={isLoading || !query.trim()}
        >
          {isLoading ? <Loader2 className="size-3.5 animate-spin mr-2" /> : <Search className="size-3.5 mr-2" />}
          Search
        </Button>
      </form>

      {results && results.files.length > 0 && (
        <div className="space-y-1 max-h-[300px] overflow-y-auto pr-1 custom-scrollbar">
          {results.files.map((file) => (
            <div
              key={file.id}
              className="flex items-center justify-between gap-3 p-2 rounded-md hover:bg-muted group transition-colors border border-transparent hover:border-border"
            >
              <div className="flex items-center gap-3 min-w-0 flex-1">
                {getFileIcon(file.mimeType)}
                <div className="flex flex-col min-w-0">
                  <span className="text-xs font-medium truncate">{file.name}</span>
                  {file.modifiedTime && (
                    <span className="text-[10px] text-muted-foreground">
                      Modified {new Date(file.modifiedTime).toLocaleDateString()}
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                {file.webViewLink && (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7"
                    asChild
                  >
                    <a href={file.webViewLink} target="_blank" rel="noopener noreferrer">
                      <ExternalLink className="size-3.5" />
                    </a>
                  </Button>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 px-2 font-mono text-[9px] uppercase tracking-tight"
                  onClick={() => handleLink(file)}
                  disabled={isAttaching}
                >
                  <Link2 className="size-3 mr-1" />
                  Link
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      {results && results.files.length === 0 && (
        <p className="text-center py-4 text-xs text-muted-foreground font-mono">
          No files found for "{deferredQuery}"
        </p>
      )}
    </div>
  )
}
