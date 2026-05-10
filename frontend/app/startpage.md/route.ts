import { START_PAGE_MARKDOWN } from "@/lib/start-page"

// Public endpoint — no auth check. Linked from / as a markdown landing
// page for crawlers and unauthenticated agents.
export function GET() {
  return new Response(START_PAGE_MARKDOWN, {
    headers: {
      "content-type": "text/markdown; charset=utf-8",
      "cache-control": "public, max-age=0, must-revalidate",
    },
  })
}