"use client"

import { useState, useCallback } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import { ArrowLeft, Upload, ImageIcon, X, CheckCircle } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { uploadImage } from "@/lib/api"
import { toast } from "sonner"
import { cn } from "@/lib/utils"

export function ImageUpload() {
  const router = useRouter()
  const [file, setFile] = useState<File | null>(null)
  const [preview, setPreview] = useState<string | null>(null)
  const [sourceId, setSourceId] = useState("images")
  const [uploading, setUploading] = useState(false)
  const [isDragging, setIsDragging] = useState(false)

  const handleFile = (f: File) => {
    if (!f.type.startsWith("image/")) {
      toast.error("Only image files are supported")
      return
    }
    setFile(f)
    const url = URL.createObjectURL(f)
    setPreview(url)
  }

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setIsDragging(false)
    const f = e.dataTransfer.files[0]
    if (f) handleFile(f)
  }, [])

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setIsDragging(true)
  }, [])

  const handleDragLeave = useCallback(() => setIsDragging(false), [])

  const handleFileInput = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0]
    if (f) handleFile(f)
  }

  const handleUpload = async () => {
    if (!file) return
    setUploading(true)
    try {
      const result = await uploadImage(file, sourceId)
      toast.success("Image indexed successfully")
      router.push(`/entries/${encodeURIComponent(result.id)}`)
    } catch {
      toast.error("Upload failed")
      setUploading(false)
    }
  }

  const clearFile = () => {
    setFile(null)
    if (preview) URL.revokeObjectURL(preview)
    setPreview(null)
  }

  return (
    <div className="mx-auto max-w-2xl px-4 py-8 space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" className="size-8 shrink-0" asChild>
          <Link href="/entries">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div>
          <h1 className="font-mono text-xs font-black uppercase tracking-[2px] text-foreground">
            Upload Image
          </h1>
          <p className="font-mono text-[10px] text-muted-foreground mt-0.5">
            Extract and index content from an image using a multimodal model
          </p>
        </div>
      </div>

      {/* Drop zone */}
      {!file ? (
        <label
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          className={cn(
            "flex flex-col items-center justify-center gap-4 border-2 border-dashed p-12 cursor-pointer transition-colors",
            isDragging
              ? "border-primary bg-primary/5"
              : "border-border bg-card hover:border-primary/50 hover:bg-card/80"
          )}
        >
          <input
            type="file"
            accept="image/*"
            className="sr-only"
            onChange={handleFileInput}
          />
          <ImageIcon className="size-12 text-muted-foreground/40" />
          <div className="text-center">
            <p className="font-mono text-sm font-bold text-foreground">
              Drop an image here
            </p>
            <p className="font-mono text-xs text-muted-foreground mt-1">
              or click to browse — PNG, JPG, WebP, GIF
            </p>
          </div>
          <div className="flex items-center gap-2 px-3 py-1.5 border border-border bg-background">
            <Upload className="size-3 text-muted-foreground" />
            <span className="font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
              Select file
            </span>
          </div>
        </label>
      ) : (
        <div className="space-y-4">
          {/* Preview */}
          <div className="relative border border-border bg-card overflow-hidden">
            <button
              onClick={clearFile}
              className="absolute top-2 right-2 z-10 size-7 flex items-center justify-center bg-background/80 border border-border hover:bg-background transition-colors"
            >
              <X className="size-3.5" />
            </button>
            <img
              src={preview!}
              alt={file.name}
              className="w-full max-h-80 object-contain"
            />
          </div>

          {/* File info */}
          <div className="flex items-center gap-3 px-3 py-2 border border-border bg-card">
            <CheckCircle className="size-4 text-primary shrink-0" />
            <div className="min-w-0">
              <p className="font-mono text-xs font-bold text-foreground truncate">{file.name}</p>
              <p className="font-mono text-[10px] text-muted-foreground">
                {(file.size / 1024).toFixed(1)} KB — {file.type}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Source ID */}
      <div className="space-y-2">
        <label className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
          Source / Category
        </label>
        <Input
          value={sourceId}
          onChange={(e) => setSourceId(e.target.value)}
          placeholder="images"
          className="font-mono text-sm"
        />
      </div>

      {/* Upload button */}
      <Button
        onClick={handleUpload}
        disabled={!file || uploading}
        className="w-full font-mono text-xs uppercase tracking-[2px]"
      >
        {uploading ? (
          <>
            <div className="size-3.5 mr-2 animate-spin border border-current border-t-transparent rounded-full" />
            Extracting & indexing…
          </>
        ) : (
          <>
            <Upload className="size-3.5 mr-2" />
            Upload & index
          </>
        )}
      </Button>
    </div>
  )
}
