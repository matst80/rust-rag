"use client"

import { useState } from "react"
import { useRouter, usePathname } from "next/navigation"
import Link from "next/link"
import { ArrowLeft, FileText, ImageIcon, Globe } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { EntryForm } from "./entry-form"
import { ImageUpload } from "./image-upload"
import { UrlIngest } from "./url-ingest"

interface NewEntryClientProps {
  defaultTab?: string
}

export function NewEntryClient({ defaultTab = "manual" }: NewEntryClientProps) {
  const router = useRouter()
  const pathname = usePathname()
  
  // Validate tab value
  const initialTab = (defaultTab === "image" || defaultTab === "url") ? defaultTab : "manual"
  const [tab, setTab] = useState<"manual" | "image" | "url">(initialTab)

  const handleTabChange = (newTab: "manual" | "image" | "url") => {
    setTab(newTab)
    router.replace(`${pathname}?tab=${newTab}`, { scroll: false })
  }

  return (
    <div className="mx-auto w-full max-w-2xl p-4 md:py-8">
      {/* Unified Header */}
      <div className="mb-6 flex items-center gap-4">
        <Button variant="ghost" size="icon" asChild>
          <Link href="/entries">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div>
          <h1 className="text-2xl font-bold">New Entry</h1>
          <p className="text-xs text-muted-foreground mt-0.5">
            {tab === "manual" && "Create a new knowledge base entry manually"}
            {tab === "image" && "Extract and index content from an image using a multimodal model"}
            {tab === "url" && "Fetch, clean, and index web content into your knowledge base"}
          </p>
        </div>
      </div>

      {/* Mode Switcher */}
      <div className="mb-6 p-1 bg-muted/65 border border-border rounded-xl flex gap-1">
        <button
          onClick={() => handleTabChange("manual")}
          className={cn(
            "flex-1 flex items-center justify-center gap-2 py-2.5 rounded-lg font-mono text-[10px] font-black uppercase tracking-widest transition-all cursor-pointer",
            tab === "manual"
              ? "bg-background text-foreground shadow-sm border border-border/10"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <FileText className="size-3.5" />
          Text Form
        </button>
        <button
          onClick={() => handleTabChange("image")}
          className={cn(
            "flex-1 flex items-center justify-center gap-2 py-2.5 rounded-lg font-mono text-[10px] font-black uppercase tracking-widest transition-all cursor-pointer",
            tab === "image"
              ? "bg-background text-foreground shadow-sm border border-border/10"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <ImageIcon className="size-3.5" />
          Image Upload
        </button>
        <button
          onClick={() => handleTabChange("url")}
          className={cn(
            "flex-1 flex items-center justify-center gap-2 py-2.5 rounded-lg font-mono text-[10px] font-black uppercase tracking-widest transition-all cursor-pointer",
            tab === "url"
              ? "bg-background text-foreground shadow-sm border border-border/10"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <Globe className="size-3.5" />
          URL Ingest
        </button>
      </div>

      {/* Tab Contents */}
      <div className="mt-4">
        {tab === "manual" && <EntryForm mode="create" minimal={true} />}
        {tab === "image" && <ImageUpload minimal={true} />}
        {tab === "url" && <UrlIngest minimal={true} />}
      </div>
    </div>
  )
}
