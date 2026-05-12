"use client"

import { useState, useCallback, useEffect, useRef } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import { ArrowLeft, Upload, ImageIcon, X, CheckCircle, Sparkles, RotateCcw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { uploadImage, api } from "@/lib/api"
import { toast } from "sonner"
import { cn } from "@/lib/utils"
import {
  captionImage,
  classifyImage,
  formatClassificationResult,
  formatLoadProgress,
  isWebGpuAvailable,
  useLlmHelperStatus,
} from "@rust-rag/llm"
import { MarkdownView } from "./markdown-view"

const IMAGE_PROMPT = `You are extracting text and describing this image for a personal knowledge base.

If the image contains readable text (screenshot, document, whiteboard), transcribe it verbatim in markdown.
If it is a photo or diagram, write 2-4 sentences describing the scene, then list any visible text.
Output plain markdown only, no preamble.`

export function ImageUpload() {
  const router = useRouter()
  const [file, setFile] = useState<File | null>(null)
  const [preview, setPreview] = useState<string | null>(null)
  const [sourceId, setSourceId] = useState("images")
  const [uploading, setUploading] = useState(false)
  const [isDragging, setIsDragging] = useState(false)
  const [webgpu, setWebgpu] = useState(false)
  const [useLocal, setUseLocal] = useState(true)
  const [caption, setCaption] = useState<string>("")
  const [labels, setLabels] = useState<string>("")
  const [captioning, setCaptioning] = useState(false)
  const visionStatus = useLlmHelperStatus()
  const abortRef = useRef<AbortController | null>(null)

  useEffect(() => { const hasGpu = isWebGpuAvailable(); setWebgpu(hasGpu); setUseLocal(hasGpu) }, [])

  const handleFile = useCallback((f: File) => {
    if (!f.type.startsWith("image/")) {
      toast.error("Only image files are supported")
      return
    }
    setFile(f)
    const url = URL.createObjectURL(f)
    setPreview(url)
  }, [])

  const handlePaste = useCallback((e: ClipboardEvent) => {
    const items = e.clipboardData?.items
    if (!items) return

    for (let i = 0; i < items.length; i++) {
      if (items[i].type.indexOf("image") !== -1) {
        const f = items[i].getAsFile()
        if (f) {
          handleFile(f)
          toast.info("Image pasted from clipboard")
          break
        }
      }
    }
  }, [handleFile])

  useEffect(() => {
    window.addEventListener("paste", handlePaste)
    return () => window.removeEventListener("paste", handlePaste)
  }, [handlePaste])

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

  const scaleImage = (file: File, maxWidth = 1600, maxHeight = 1600): Promise<Blob> => {
    return new Promise((resolve, reject) => {
      const img = new Image()
      img.src = URL.createObjectURL(file)
      img.onload = () => {
        URL.revokeObjectURL(img.src)
        let width = img.width
        let height = img.height

        if (width <= maxWidth && height <= maxHeight) {
          resolve(file)
          return
        }

        if (width > height) {
          if (width > maxWidth) {
            height *= maxWidth / width
            width = maxWidth
          }
        } else {
          if (height > maxHeight) {
            width *= maxHeight / height
            height = maxHeight
          }
        }

        const canvas = document.createElement("canvas")
        canvas.width = width
        canvas.height = height
        const ctx = canvas.getContext("2d")
        if (!ctx) {
          reject(new Error("Failed to get canvas context"))
          return
        }

        ctx.drawImage(img, 0, 0, width, height)
        canvas.toBlob(
          (blob) => {
            if (blob) {
              resolve(blob)
            } else {
              reject(new Error("Canvas toBlob failed"))
            }
          },
          file.type,
          0.85 // quality
        )
      }
      img.onerror = reject
    })
  }

  const runLocalCaption = useCallback(async (sourceFile: File): Promise<string> => {
    setCaptioning(true)
    setCaption("")
    setLabels("")
    abortRef.current = new AbortController()
    try {
      const scaledBlob = await scaleImage(sourceFile)

      // Step 1: Classify (very fast)
      try {
        const res = await classifyImage(scaledBlob)
        const formatted = formatClassificationResult(res)
        setLabels(formatted)
      } catch (err) {
        console.warn("Classification failed", err)
      }

      // Step 2: Caption (LLM)
      const text = await captionImage(scaledBlob, {
        prompt: IMAGE_PROMPT,
        onToken: (partial) => setCaption(partial),
        signal: abortRef.current.signal,
      })
      setCaption(text)
      return text
    } catch (err) {
      console.error("Caption error", err)
      const msg = err instanceof Error ? err.message : String(err)
      if (msg !== "aborted") toast.error(`Local caption failed: ${msg}`)
      return ""
    } finally {
      setCaptioning(false)
    }
  }, [])

  const handleRecaption = () => {
    if (file) runLocalCaption(file)
  }

  const handleUpload = async () => {
    if (!file) return
    setUploading(true)
    try {
      const scaledBlob = await scaleImage(file)
      const scaledFile = new File([scaledBlob], file.name, { type: file.type })

      if (useLocal) {
        // Caption locally if not already done, then create entry + attach.
        let text = caption
        if (!text) {
          text = await runLocalCaption(file)
        }

        // Combine labels and caption
        const combinedText = labels
          ? `Labels: ${labels}\n\n${text}`
          : text

        const entry = await api.items.create({
          text: combinedText,
          source_id: sourceId || "images",
          metadata: {
            source_type: "image",
            original_filename: file.name,
            captioned_by: "gemma-4-E4B-it",
            labels: labels || undefined,
          },
        })
        try {
          await api.attachments.upload(entry.id, scaledFile)
        } catch (err) {
          console.warn("Attachment upload failed (entry already created)", err)
        }
        toast.success("Image indexed locally")
        router.push(`/entries/${encodeURIComponent(entry.id)}`)
        return
      }

      const result = await uploadImage(scaledFile, sourceId)
      toast.success("Image indexed (server)")
      router.push(`/entries/${encodeURIComponent(result.id)}`)
    } catch (err) {
      console.error("Upload error:", err)
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

      {/* Engine toggle */}
      <div className="flex items-center justify-between gap-3 px-3 py-2 border border-border bg-card">
        <div className="flex items-center gap-2 min-w-0">
          <Sparkles className={cn("size-3.5 shrink-0", useLocal ? "text-primary" : "text-muted-foreground/40")} />
          <div className="min-w-0">
            <p className="font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              {useLocal ? "Caption locally (Gemma 4 · private)" : "Caption on backend (Claude)"}
            </p>
            {useLocal && visionStatus.kind === "loading" && (
              <p className="font-mono text-[9px] text-muted-foreground/70 tabular-nums">
                {formatLoadProgress(visionStatus)}
              </p>
            )}
            {useLocal && visionStatus.kind === "error" && (
              <p className="font-mono text-[9px] text-destructive truncate">
                {visionStatus.message}
              </p>
            )}
          </div>
        </div>
        <button
          type="button"
          onClick={() => webgpu && setUseLocal((v) => !v)}
          disabled={!webgpu}
          className={cn(
            "font-mono text-[9px] uppercase tracking-[1.5px] px-2 py-1 border transition-colors",
            useLocal
              ? "border-primary/50 text-primary bg-primary/10"
              : "border-border text-muted-foreground hover:text-foreground",
            !webgpu && "opacity-30 cursor-not-allowed"
          )}
          title={webgpu ? "Toggle engine" : "WebGPU not available — server only"}
        >
          {useLocal ? "Local" : "Server"}
        </button>
      </div>

      {/* Caption preview */}
      {file && useLocal && (caption || captioning) && (
        <div className="relative border border-primary/20 bg-primary/[0.02] p-4 space-y-2">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <Sparkles className="size-3 text-primary" />
              <span className="font-mono text-[10px] font-bold uppercase tracking-[2px] text-primary/80">
                Extracted caption
              </span>
              {captioning && (
                <span className="font-mono text-[9px] text-primary/60 uppercase tracking-widest animate-pulse">
                  generating…
                </span>
              )}
            </div>
            <button
              type="button"
              onClick={handleRecaption}
              disabled={captioning}
              className="font-mono text-[9px] uppercase tracking-widest text-muted-foreground/60 hover:text-primary flex items-center gap-1 disabled:opacity-30"
            >
              <RotateCcw className="size-2.5" />
              Re-caption
            </button>
          </div>
          {caption ? (
            <MarkdownView content={caption} />
          ) : (
            <p className="text-sm text-muted-foreground/60 italic">…</p>
          )}
        </div>
      )}

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
