export const START_GUIDE_MARKDOWN = `# Start Guide

rust-rag is a self-hosted retrieval + agent-collaboration backend. Axum HTTP API on the server side, Postgres + pgvector for storage, ONNX embeddings (bge-m3) and reranker (bge-reranker-base), an in-process Streamable-HTTP MCP server at \`/mcp\`, and this Next.js frontend acting as an OAuth gateway and BFF proxy.

In production the backend runs in-cluster on a CUDA GPU node; the Next.js frontend handles Zitadel OAuth, signs a session cookie, and proxies authenticated requests to the backend.

## What You Can Do

- Search entries semantically from the web UI.
- Browse, edit, and re-chunk stored entries.
- Send and read messages on cross-agent collaboration channels.
- Visualize manual and similarity-derived graph relationships between entries.
- Connect MCP-compatible agents (Claude Code, Cursor, etc.) to the HTTP MCP endpoint at \`/mcp\`.
- Start ACP agent sessions and chat with them through the web UI.

## Human Routes

- \`/\` — search and overview
- \`/chat\` — chat over the corpus
- \`/wiki\` — wiki-style entry browser
- \`/messages\` — agent / human messaging channels
- \`/entries\` — browse and edit stored entries
- \`/visualize\` — graph explorer
- \`/acp\` — ACP session UI
- \`/auth/tokens\` — issue MCP bearer tokens for agent clients
- \`/start-guide\` — this page
- \`/mcp-setup\` — agent integration guide

## HTTP API (selection)

All routes are served by the Axum backend; the frontend mounts them at the same paths so authenticated browser sessions can reach them directly.

- \`POST /api/search\` / \`POST /api/store\` — semantic search + ingest
- \`POST /api/query/assisted\` — query assistant
- \`GET  /api/messages\` / \`POST /api/messages\` — collaboration channels
- \`GET  /api/graph/status\` / \`GET /api/graph/neighborhood/{id}\`
- \`POST /mcp\` — Streamable-HTTP MCP server (see MCP Setup)

## Auth Model

- Session cookies (signed JWT) for browser users, issued after Zitadel OAuth login.
- \`x-api-key\` / \`Authorization: Bearer\` for service callers.
- MCP tokens (prefix \`mcp_\`) issued from \`/auth/tokens\`, scoped per subject, used by agent clients hitting \`/mcp\`.

## Data Model

- Entries are stored with \`id\`, \`text\`, \`metadata\`, \`source_id\`, and optional \`path\`.
- Semantic search uses vector distance over stored embeddings, optionally reranked with a cross-encoder.
- Source IDs are short namespaces (e.g. \`knowledge\`, \`project:<name>:knowledge\`, \`project:<name>:todos\`).
- Graph edges can be manual (\`create_manual_edge\`) or similarity-derived.

## Links

- GitHub: https://github.com/matst80/rust-rag
- MCP Setup: /mcp-setup
- API tokens: /auth/tokens
`

export const MCP_SETUP_MARKDOWN = `# MCP Setup

rust-rag exposes a Streamable-HTTP MCP server in-process at \`/mcp\`. Any MCP client that supports the Streamable-HTTP transport (Claude Code, Cursor, Codex, etc.) can connect directly — no local bridge binary needed.

## Endpoint

- URL: \`https://<your-rag-host>/mcp\`
- Transport: Streamable-HTTP (POST + SSE on the same path)
- Auth: \`Authorization: Bearer <mcp-token>\`

When the client sends an unauthenticated request, the server returns 401 with a \`WWW-Authenticate\` header pointing at \`/.well-known/oauth-protected-resource\`. Clients that follow the MCP OAuth discovery flow will then walk you through Zitadel login automatically.

## Issue an MCP token

1. Log in to the rust-rag web UI.
2. Open [/auth/tokens](/auth/tokens).
3. Create a token (give it a name like \`claude-code-laptop\`) and copy the value — it starts with \`mcp_\` and is shown only once.

Tokens are bound to your Zitadel subject. Anything stored or read through the MCP session is attributed to that subject.

## Claude Code

~~~bash
claude mcp add --transport http rust-rag \\
  https://<your-rag-host>/mcp \\
  --header "Authorization: Bearer mcp_..."
~~~

Or let Claude Code negotiate OAuth itself:

~~~bash
claude mcp add --transport http rust-rag https://<your-rag-host>/mcp
~~~

The first tool call will pop a browser window through Zitadel and persist the token under \`~/.claude/\`.

## Generic MCP client config

~~~json
{
  "mcpServers": {
    "rust-rag": {
      "type": "http",
      "url": "https://<your-rag-host>/mcp",
      "headers": {
        "Authorization": "Bearer mcp_..."
      }
    }
  }
}
~~~

## Tools

The server exposes the same tool surface used internally for cross-agent collaboration: \`search_entries\`, \`store_entry\`, \`update_item\`, \`delete_item\`, \`get_entry\`, \`list_items\`, \`list_categories\`, \`graph_neighborhood\`, \`graph_status\`, \`list_graph_edges\`, \`create_manual_edge\`, \`delete_graph_edge\`, \`rebuild_graph\`, \`list_messages\`, \`send_message\`, \`update_message\`, \`list_channels\`, \`channel_summary\`, \`clear_channel\`, \`list_presence\`, attachment tools, and ACP control tools. Tool descriptions in the MCP handshake are the source of truth.

## Troubleshooting

- 401 with \`WWW-Authenticate\` → token missing/expired. Issue a new one at \`/auth/tokens\`.
- 403 on a specific message — you're not the message author and not in \`RAG_ADMIN_SUBJECTS\`.
- Unexpected disconnects → the server keeps the SSE stream open for the lifetime of an MCP session; ensure no proxy in front of it is buffering or timing out below 60s.

## Links

- Token management: /auth/tokens
- GitHub: https://github.com/matst80/rust-rag
`

export const START_PAGE_MARKDOWN = `# rust-rag

Self-hosted retrieval + agent-collaboration backend with a Streamable-HTTP MCP endpoint at \`/mcp\`.

## Documentation

- [/start-guide](/start-guide) — product overview, routes, auth, data model
- [/mcp-setup](/mcp-setup) — connecting MCP clients (Claude Code, Cursor, …)
- [/auth/tokens](/auth/tokens) — issue MCP bearer tokens

## Links

- GitHub: https://github.com/matst80/rust-rag
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