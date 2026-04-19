import { START_GUIDE_MARKDOWN } from "@/lib/start-page"

export function GET() {
  return new Response(START_GUIDE_MARKDOWN, {
    headers: {
      "content-type": "text/markdown; charset=utf-8",
      "cache-control": "public, max-age=0, must-revalidate",
    },
  })
}
