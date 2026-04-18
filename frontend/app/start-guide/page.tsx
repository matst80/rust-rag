import { DocsShell } from "@/components/docs/docs-shell"
import { START_GUIDE_MARKDOWN } from "@/lib/start-page"

export default function StartGuidePage() {
  return (
    <DocsShell
      title="Start Guide"
      eyebrow="Documentation"
      description="How the product is organized, which routes matter, and how humans should move through search, entries, and graph exploration."
      markdown={START_GUIDE_MARKDOWN}
      pathname="/start-guide"
      resources={[
        { label: "Search", href: "/" },
        { label: "Entries", href: "/entries" },
        { label: "Graph", href: "/visualize" },
        {
          label: "GitHub Repository",
          href: "https://github.com/matst80/rust-rag",
          external: true,
        },
      ]}
    />
  )
}