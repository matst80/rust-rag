import { MCP_SETUP_MARKDOWN } from "@/lib/start-page"

export function GET() {
  return new Response(MCP_SETUP_MARKDOWN, {
    headers: {
      "content-type": "text/markdown; charset=utf-8",
      "cache-control": "public, max-age=0, must-revalidate",
    },
  })
}
