"use client"

import { useState } from "react"
import { useRouter, usePathname } from "next/navigation"
import Link from "next/link"
import { ArrowLeft, FileText, ImageIcon, Globe, LayoutTemplate } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { EntryForm } from "./entry-form"
import { ImageUpload } from "./image-upload"
import { UrlIngest } from "./url-ingest"

interface NewEntryClientProps {
  defaultTab?: string
}

interface EntryTemplate {
  id: string
  label: string
  description: string
  text: string
  path: string
  metadata: Record<string, string>
}

const TEMPLATES: EntryTemplate[] = [
  {
    id: "blank",
    label: "Blank note",
    description: "Empty page, file it later",
    text: "",
    path: "",
    metadata: {},
  },
  {
    id: "meeting",
    label: "Meeting note",
    description: "Attendees, notes, actions",
    text: `# Meeting — ${new Date().toLocaleDateString()}

**Attendees:** 

## Notes

- 

## Actions

- [ ] `,
    path: "notes/meetings",
    metadata: { kind: "meeting" },
  },
  {
    id: "decision",
    label: "Decision",
    description: "Context, choice, consequences",
    text: `# Decision: 

## Context


## Decision


## Consequences

`,
    path: "decisions",
    metadata: { kind: "decision" },
  },
  {
    id: "adr",
    label: "ADR",
    description: "Architecture decision record",
    text: `# ADR: 

**Status:** proposed

## Context


## Decision


## Consequences

`,
    path: "decisions/adr",
    metadata: { kind: "adr" },
  },
]

export function NewEntryClient({ defaultTab = "manual" }: NewEntryClientProps) {
  const router = useRouter()
  const pathname = usePathname()
  
  // Validate tab value
  const initialTab = (defaultTab === "image" || defaultTab === "url") ? defaultTab : "manual"
  const [tab, setTab] = useState<"manual" | "image" | "url">(initialTab)
  const [templateId, setTemplateId] = useState("blank")
  const template = TEMPLATES.find((t) => t.id === templateId) ?? TEMPLATES[0]

  const handleTabChange = (newTab: "manual" | "image" | "url") => {
    setTab(newTab)
    router.replace(`${pathname}?tab=${newTab}`, { scroll: false })
  }

  return (
    <div className="mx-auto w-full max-w-2xl md:max-w-4xl p-4 md:py-8">
      {/* Unified Header */}
      <div className="mb-6 flex items-center gap-4">
        <Button variant="ghost" size="icon" asChild className="shrink-0">
          <Link href="/entries">
            <ArrowLeft className="size-4" />
          </Link>
        </Button>
        <div>
          <h1 className="text-2xl font-bold tracking-tight">New Entry</h1>
          <p className="text-xs text-muted-foreground mt-0.5">
            {tab === "manual" && "Create a new knowledge base entry manually"}
            {tab === "image" && "Extract and index content from an image using a multimodal model"}
            {tab === "url" && "Fetch, clean, and index web content into your knowledge base"}
          </p>
        </div>
      </div>

      {/* Mode Switcher */}
      <div className="mb-6 p-1 bg-muted/65 border border-border rounded-xl flex gap-1 shadow-sm">
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

      {/* Template picker (manual tab) */}
      {tab === "manual" && (
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <span className="flex items-center gap-1.5 font-mono text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
            <LayoutTemplate className="size-3.5" />
            Template
          </span>
          {TEMPLATES.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setTemplateId(t.id)}
              title={t.description}
              className={cn(
                "rounded-full border px-3 py-1.5 text-xs font-medium transition-colors",
                t.id === templateId
                  ? "border-primary/50 bg-primary/10 text-primary"
                  : "border-border text-muted-foreground hover:border-primary/30 hover:text-foreground"
              )}
            >
              {t.label}
            </button>
          ))}
        </div>
      )}

      {/* Tab Contents */}
      <div className="mt-4">
        {tab === "manual" && (
          <EntryForm
            key={templateId}
            mode="create"
            minimal={true}
            initialData={
              template.id === "blank"
                ? undefined
                : {
                    text: template.text,
                    path: template.path,
                    metadata: template.metadata,
                    source_id: "knowledge",
                  }
            }
          />
        )}
        {tab === "image" && <ImageUpload minimal={true} />}
        {tab === "url" && <UrlIngest minimal={true} />}
      </div>
    </div>
  )
}
