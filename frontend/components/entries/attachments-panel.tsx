"use client"

import { useRef, useState } from "react"
import {
  useAttachments,
  useUploadAttachment,
  useAttachUrl,
  useDeleteAttachment,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Paperclip, Trash2, Upload, Link2 } from "lucide-react"
import { toast } from "sonner"

interface AttachmentsPanelProps {
  itemId: string
}

function formatBytes(n?: number | null): string {
  if (n === null || n === undefined) return ""
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}

export function AttachmentsPanel({ itemId }: AttachmentsPanelProps) {
  const { data: attachments, mutate } = useAttachments(itemId)
  const { trigger: uploadFile, isMutating: isUploading } = useUploadAttachment(itemId)
  const { trigger: attachUrl, isMutating: isAttaching } = useAttachUrl(itemId)
  const { trigger: deleteAttachment } = useDeleteAttachment(itemId)
  const fileRef = useRef<HTMLInputElement>(null)
  const [urlInput, setUrlInput] = useState("")
  const [showUrl, setShowUrl] = useState(false)

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    try {
      await uploadFile(file)
      mutate()
      toast.success(`Uploaded ${file.name}`)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Upload failed")
    } finally {
      if (fileRef.current) fileRef.current.value = ""
    }
  }

  const handleAttachUrl = async () => {
    const url = urlInput.trim()
    if (!url) return
    try {
      await attachUrl({ url })
      mutate()
      setUrlInput("")
      setShowUrl(false)
      toast.success("Attached from URL")
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Fetch failed")
    }
  }

  const handleDelete = async (id: string, name: string) => {
    try {
      await deleteAttachment(id)
      mutate()
      toast.success(`Deleted ${name}`)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Delete failed")
    }
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h2 className="font-mono text-xs font-bold uppercase tracking-widest text-muted-foreground">
          Attachments {attachments && attachments.length > 0 ? `— ${attachments.length}` : ""}
        </h2>
        <div className="flex items-center gap-2">
          <input
            ref={fileRef}
            type="file"
            className="hidden"
            onChange={handleFileChange}
          />
          <Button
            variant="outline"
            size="sm"
            className="font-mono text-[10px] uppercase tracking-[1.5px] h-8"
            onClick={() => fileRef.current?.click()}
            disabled={isUploading}
          >
            <Upload className="size-3.5 mr-1.5" />
            {isUploading ? "Uploading…" : "Upload"}
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="font-mono text-[10px] uppercase tracking-[1.5px] h-8"
            onClick={() => setShowUrl((v) => !v)}
          >
            <Link2 className="size-3.5 mr-1.5" />
            URL
          </Button>
        </div>
      </div>

      {showUrl && (
        <div className="flex gap-2 mb-3">
          <Input
            placeholder="https://example.com/file.pdf"
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault()
                handleAttachUrl()
              }
            }}
          />
          <Button
            variant="default"
            size="sm"
            className="font-mono text-[10px] uppercase tracking-[1.5px] h-9"
            onClick={handleAttachUrl}
            disabled={isAttaching || !urlInput.trim()}
          >
            {isAttaching ? "Fetching…" : "Attach"}
          </Button>
        </div>
      )}

      {attachments && attachments.length > 0 ? (
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          {attachments.map((a) => (
            <div
              key={a.id}
              className="flex items-center gap-3 border border-border bg-card p-3"
            >
              <Paperclip className="size-4 text-muted-foreground shrink-0" />
              <div className="flex flex-col min-w-0 flex-1">
                <a
                  href={a.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="font-mono text-xs text-foreground hover:text-primary truncate"
                >
                  {a.filename ?? a.stored_name}
                </a>
                <span className="font-mono text-[10px] text-muted-foreground">
                  {a.mime ?? ""} {formatBytes(a.size)}
                </span>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 shrink-0 hover:bg-destructive hover:text-destructive-foreground"
                onClick={() => handleDelete(a.id, a.filename ?? a.stored_name)}
              >
                <Trash2 className="size-3.5" />
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <p className="font-mono text-[11px] text-muted-foreground/70">
          No attachments yet.
        </p>
      )}
    </div>
  )
}
