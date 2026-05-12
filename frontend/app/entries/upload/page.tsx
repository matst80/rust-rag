"use client"

import { useState } from "react"
import { AppHeader } from "@/components/app-header"
import { ImageUpload } from "@/components/entries/image-upload"
import { UrlIngest } from "@/components/entries/url-ingest"
import { ImageIcon, Globe } from "lucide-react"
import { cn } from "@/lib/utils"

export default function UploadPage() {
  const [mode, setMode] = useState<"image" | "url">("image")

  return (
    <>
      <AppHeader />
      <main className="pb-20">
        <div className="mx-auto max-w-2xl px-4 pt-8">
          <div className="flex p-1 bg-muted border border-border">
            <button
              onClick={() => setMode("image")}
              className={cn(
                "flex-1 flex items-center justify-center gap-2 py-2 font-mono text-[10px] font-black uppercase tracking-widest transition-all",
                mode === "image" ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
              )}
            >
              <ImageIcon className="size-3" />
              Image
            </button>
            <button
              onClick={() => setMode("url")}
              className={cn(
                "flex-1 flex items-center justify-center gap-2 py-2 font-mono text-[10px] font-black uppercase tracking-widest transition-all",
                mode === "url" ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
              )}
            >
              <Globe className="size-3" />
              URL
            </button>
          </div>
        </div>

        {mode === "image" ? <ImageUpload /> : <UrlIngest />}
      </main>
    </>
  )
}
