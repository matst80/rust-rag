# mcp-stdio

A stdio MCP bridge that exposes the `rust-rag` HTTP API to any MCP-compatible client (Claude Code, Gemini CLI, Claude Desktop, etc.).

The `rust-rag` HTTP server must be running and reachable — this binary only proxies.

## Build

From the repo root:

```bash
make build-mcp
# or:
cargo build --release --manifest-path mcp-stdio/Cargo.toml
```

The binary is produced at `mcp-stdio/target/release/mcp-stdio`. Use the absolute path to this binary in the client configs below.

## Configuration

All configuration is via environment variables passed to the bridge process.

| Variable | Default | Description |
|---|---|---|
| `RAG_MCP_API_BASE_URL` | `https://rag.k6n.net` | rust-rag HTTP base URL |
| `RAG_MCP_TIMEOUT_SECS` | `30` | Outbound HTTP timeout |
| `RAG_MCP_TOOL_GROUPS` | `core,admin,graph` | Comma-separated tool groups to expose |
| `RAG_MCP_SEARCH_FORMAT` | `markdown` | `markdown`, `json`, or `both` |
| `RAG_MCP_AUTH_BEARER` | — | Bearer token forwarded upstream |
| `RAG_MCP_HEADERS` | — | Extra upstream headers: `Name=Value;Other=Value` |
| `RAG_MCP_SERVER_NAME` | `rust-rag-mcp` | Reported during MCP initialization |
| `RAG_MCP_SERVER_VERSION` | crate version | Reported during MCP initialization |
| `RAG_MCP_SERVER_INSTRUCTIONS` | built-in | Server instructions text |

### Tool groups

- `core` — `health_status`, `store_entry`, `search_entries`
- `admin` — `list_categories`, `list_items`, `update_item`, `delete_item`
- `graph` — `graph_status`, `list_graph_edges`, `graph_neighborhood`, `rebuild_graph`, `create_manual_edge`, `delete_graph_edge`

Restrict to what an agent actually needs (e.g. `RAG_MCP_TOOL_GROUPS=core`) to reduce tool-list bloat in the model's context.

### Search format

Controls how `search_entries` responds. The tool always returns both the top vector hits and a `related` list of manually linked items anchored on the top hit.

- `markdown` *(default)* — LLM-friendly prose. Smallest context footprint.
- `json` — Structured JSON only (via MCP `structured_content`). Use when the client chains tool outputs programmatically.
- `both` — Markdown for the model, structured JSON for tooling. Widest compatibility, largest token cost.

## Claude Code

Register the bridge with Claude Code using the CLI:

```bash
claude mcp add rust-rag \
  --env RAG_MCP_SEARCH_FORMAT=markdown \
  -- /absolute/path/to/mcp-stdio/target/release/mcp-stdio
```

Or add it manually to `~/.claude.json` (user scope) or `.mcp.json` (project scope):

```json
{
  "mcpServers": {
    "rust-rag": {
      "command": "/home/mats/github.com/matst80/rust-rag/mcp-stdio/target/release/mcp-stdio",
      "args": [],
      "env": {
        "RAG_MCP_SEARCH_FORMAT": "markdown",
        "RAG_MCP_TOOL_GROUPS": "core,graph"
      }
    }
  }
}
```

Point it at a self-hosted instance with `RAG_MCP_API_BASE_URL=http://127.0.0.1:4001` (or your own URL).

Verify the connection:

```bash
claude mcp list
```

Inside a Claude Code session, the tools appear as `mcp__rust-rag__search_entries`, etc. Use `/mcp` to inspect status.

## Gemini CLI

Add the bridge to `~/.gemini/settings.json` under `mcpServers`:

```json
{
  "mcpServers": {
    "rust-rag": {
      "command": "/absolute/path/to/mcp-stdio/target/release/mcp-stdio",
      "env": {
        "RAG_MCP_SEARCH_FORMAT": "markdown"
      }
    }
  }
}
```

Add `"RAG_MCP_API_BASE_URL"` to the `env` block to target a self-hosted instance. For a project-local config, use `.gemini/settings.json` in the repo root — values there override the user-level file.

Restart the CLI after editing. The tools are invoked via the standard function-call interface once Gemini discovers them during `initialize`.

## Quick sanity check

Before wiring it into a client, confirm the bridge speaks MCP over stdio. This sends an `initialize` request and should return a JSON-RPC response listing the tool capabilities:

```bash
./mcp-stdio/target/release/mcp-stdio <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"probe","version":"0.0.1"}}}
EOF
```

If the HTTP server is unreachable, tool calls will return an error from the bridge describing the failure.

## Search result shape

When `RAG_MCP_SEARCH_FORMAT=json` or `both`, the structured content of `search_entries` follows:

```json
{
  "results": [
    { "id": "...", "text": "...", "metadata": {}, "source_id": "...",
      "created_at": 0, "distance": 0.2 }
  ],
  "related": [
    { "id": "...", "text": "...", "metadata": {}, "source_id": "...",
      "created_at": 0, "distance": 0.35, "relation": "supports" }
  ]
}
```

`related` contains items that a user has manually linked from the top-ranked result via the graph's manual edges. It excludes items already present in `results` and is sorted by distance to the query (ascending). The optional `relation` string is the edge label.
