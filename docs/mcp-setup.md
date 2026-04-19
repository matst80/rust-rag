# MCP Setup

> **Agent Integration**
> Download the released bridge binary, configure the upstream rust-rag API, and register the MCP server with agent clients.

The `mcp-stdio` bridge exposes rust-rag to MCP-compatible agent clients over stdio. The HTTP server stays behind the bridge.

## Setup Flow

1. Download the latest `mcp-stdio` release binary from GitHub Releases.
2. Point it at your rust-rag API with `RAG_MCP_API_BASE_URL`.
3. Register the binary in Claude Code, Gemini, or another MCP client.

## Recommended Environment

- `RAG_MCP_API_BASE_URL` - your deployed rust-rag base URL
- `RAG_MCP_TOOL_GROUPS` - keep this narrow, for example `core,graph`
- `RAG_MCP_SEARCH_FORMAT` - `markdown` is the best default for agents
- `RAG_MCP_AUTH_BEARER` - optional auth for upstream requests

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

The repository publishes Linux amd64 and arm64 mcp-stdio archives when a tag matching `mcp-stdio-v*` is pushed.

- GitHub: https://github.com/matst80/rust-rag
- Releases: https://github.com/matst80/rust-rag/releases
- MCP README: https://github.com/matst80/rust-rag/blob/main/mcp-stdio/README.md
