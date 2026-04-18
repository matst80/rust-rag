import { DocsShell } from "@/components/docs/docs-shell"
import { MCP_SETUP_MARKDOWN } from "@/lib/start-page"

export default function McpSetupPage() {
  return (
    <DocsShell
      title="MCP Setup"
      eyebrow="Agent Integration"
      description="Download the released bridge binary, configure the upstream rust-rag API, and register the MCP server with agent clients."
      markdown={MCP_SETUP_MARKDOWN}
      pathname="/mcp-setup"
      resources={[
        {
          label: "GitHub Releases",
          href: "https://github.com/matst80/rust-rag/releases",
          external: true,
        },
        {
          label: "MCP README",
          href: "https://github.com/matst80/rust-rag/blob/main/mcp-stdio/README.md",
          external: true,
        },
        { label: "Start Guide", href: "/start-guide" },
      ]}
    />
  )
}