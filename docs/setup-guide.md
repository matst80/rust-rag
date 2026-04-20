# Start Guide

> **Documentation**
> How the product is organized, which routes matter, and how humans should move through search, entries, and graph exploration.

rust-rag is a local retrieval backend with a Next.js frontend, an Axum HTTP API, SQLite/sqlite-vec storage, and an MCP stdio bridge for agent clients.

The web application is protected with Zitadel-backed sign-in. The Next.js server completes the authorization-code exchange, stores a signed session cookie, and proxies browser traffic to the Rust API with an internal API key.

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

Direct HTTP and MCP access can use configured API keys via `x-api-key` or `Authorization: Bearer`.

## Search Workflow

1. Open the search page and enter a natural-language query.
2. Filter by source if you want to scope results to a category.
3. Open an entry to inspect or edit the full markdown content.
4. Use the graph view to move from one entry into connected context.

## Data Model

- Entries are stored with `id`, `text`, `metadata`, and `source_id`.
- Semantic search uses vector distance over stored embeddings.
- Graph edges can be manual or similarity-derived.
- The MCP bridge exposes the same underlying capabilities to agents.

## Links

- GitHub: https://github.com/matst80/rust-rag
- Releases: https://github.com/matst80/rust-rag/releases
- MCP README: https://github.com/matst80/rust-rag/blob/main/mcp-stdio/README.md
