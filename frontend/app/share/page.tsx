"use client"

import { Suspense } from "react"
import { useSearchParams } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { EntryForm } from "@/components/entries/entry-form"

function ShareContent() {
  const searchParams = useSearchParams()
  const title = searchParams.get("title")
  const text = searchParams.get("text")
  const url = searchParams.get("url")

  // Combine title, text and url into a single content block
  const combinedText = [
    title && `# ${title}`,
    text,
    url && `Source: ${url}`
  ].filter(Boolean).join("\n\n")

  const initialEntry = {
    id: "",
    text: combinedText,
    source_id: "shared",
    metadata: {
      original_title: title || "",
      shared_url: url || "",
      platform: "android_share"
    },
    created_at: Date.now()
  }

  return (
    <main className="container py-8">
      <div className="mb-8">
        <h2 className="text-3xl font-bold tracking-tight">Save Shared Content</h2>
        <p className="text-muted-foreground">Review and categorize the content you shared from Android.</p>
      </div>
      <EntryForm mode="create" entry={initialEntry as any} />
    </main>
  )
}

export default function SharePage() {
  return (
    <div className="min-h-screen flex flex-col">
      <AppHeader />
      <Suspense fallback={<div className="p-8 text-center">Loading shared content...</div>}>
        <ShareContent />
      </Suspense>
    </div>
  )
}
