export const START_GUIDE_MARKDOWN = `# Start Guide

rust-rag is a local retrieval backend with a Next.js frontend, an Axum HTTP API, SQLite/sqlite-vec storage, and an MCP stdio bridge for agent clients.

## What You Can Do

- Search entries semantically from the web UI.
- Browse and edit stored entries.
- Visualize manual and semantic graph relationships.
- Connect MCP-compatible agents to the mcp-stdio bridge.

## Human Routes

- "/" - search and overview
- "/start-guide" - product and route guide
- "/mcp-setup" - agent integration guide
- "/entries" - browse and edit entries
- "/visualize" - graph explorer

## HTTP API

The frontend proxies these backend routes:

- POST /search
- POST /store
- GET /admin/categories
- GET /admin/items
- GET /graph/status
- GET /graph/neighborhood/:id

## Search Workflow

1. Open the search page and enter a natural-language query.
2. Filter by source if you want to scope results to a category.
3. Open an entry to inspect or edit the full markdown content.
4. Use the graph view to move from one entry into connected context.

## Data Model

- Entries are stored with \`id\`, \`text\`, \`metadata\`, and \`source_id\`.
- Semantic search uses vector distance over stored embeddings.
- Graph edges can be manual or similarity-derived.
- The MCP bridge exposes the same underlying capabilities to agents.

## Links

- GitHub: https://github.com/matst80/rust-rag
- Releases: https://github.com/matst80/rust-rag/releases
- MCP README: https://github.com/matst80/rust-rag/blob/main/mcp-stdio/README.md
`

export const MCP_SETUP_MARKDOWN = `# MCP Setup

The \`mcp-stdio\` bridge exposes rust-rag to MCP-compatible agent clients over stdio. The HTTP server stays behind the bridge.

## Setup Flow

1. Download the latest \`mcp-stdio\` release binary from GitHub Releases.
2. Point it at your rust-rag API with \`RAG_MCP_API_BASE_URL\`.
3. Register the binary in Claude Code, Gemini, or another MCP client.

## Recommended Environment

- \`RAG_MCP_API_BASE_URL\` - your deployed rust-rag base URL
- \`RAG_MCP_TOOL_GROUPS\` - keep this narrow, for example \`core,graph\`
- \`RAG_MCP_SEARCH_FORMAT\` - \`markdown\` is the best default for agents
- \`RAG_MCP_AUTH_BEARER\` - optional auth for upstream requests

## Claude Code

~~~bash
claude mcp add rust-rag \
  --env RAG_MCP_SEARCH_FORMAT=markdown \
  -- /absolute/path/to/mcp-stdio
~~~

## Generic MCP JSON

~~~json
{
  "mcpServers": {
    "rust-rag": {
      "command": "/absolute/path/to/mcp-stdio",
      "env": {
        "RAG_MCP_API_BASE_URL": "https://your-rag-host",
        "RAG_MCP_TOOL_GROUPS": "core,graph"
      }
    }
  }
}
~~~

## Release Artifacts

The repository publishes Linux amd64 and arm64 mcp-stdio archives when a tag matching \`mcp-stdio-v*\` is pushed.

- GitHub: https://github.com/matst80/rust-rag
- Releases: https://github.com/matst80/rust-rag/releases
- MCP README: https://github.com/matst80/rust-rag/blob/main/mcp-stdio/README.md
`

export const START_PAGE_MARKDOWN = `# rust-rag

For human-readable documentation, open:

- /start-guide
- /mcp-setup

For agent-oriented integration, use the MCP setup flow and release binaries from GitHub.

## Quick Links

- GitHub: https://github.com/matst80/rust-rag
- Releases: https://github.com/matst80/rust-rag/releases
- Start Guide: /start-guide
- MCP Setup: /mcp-setup
`

export function acceptsMarkdown(acceptHeader: string | null): boolean {
  if (!acceptHeader) {
    return false
  }

  return acceptHeader
    .toLowerCase()
    .split(",")
    .some((part) => part.trim().startsWith("text/markdown"))
}