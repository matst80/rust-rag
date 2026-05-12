import { DocsShell } from "@/components/docs/docs-shell"
import { MCP_SETUP_MARKDOWN } from "@/lib/start-page"

// Public docs page — no session required.
export default function McpSetupPage() {
  return (
    <DocsShell
      title="MCP Setup"
      eyebrow="Agent Integration"
      description="Connect MCP clients (Claude Code, Cursor, Codex, …) to the in-process Streamable-HTTP MCP endpoint at /mcp."
      markdown={MCP_SETUP_MARKDOWN}
      pathname="/mcp-setup"
      resources={[
        { label: "Issue MCP token", href: "/auth/tokens" },
        { label: "Start Guide", href: "/start-guide" },
        {
          label: "GitHub Repository",
          href: "https://github.com/matst80/rust-rag",
          external: true,
        },
      ]}
    />
  )
}