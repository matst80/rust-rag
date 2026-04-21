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
- `RAG_MCP_AUTH_BEARER` - optional bearer-style API key for upstream requests
- `RAG_MCP_HEADERS` - optional extra headers, for example `x-api-key=your-direct-key`

The bridge accepts upstream auth only through environment variables. There is no dedicated command-line flag for API keys.

If the Rust API is protected with `RAG_API_KEYS`, use one of these:

- `RAG_MCP_AUTH_BEARER=your-direct-key`
- `RAG_MCP_HEADERS=x-api-key=your-direct-key`

If you want `x-api-key` auth to be enforced by the Rust API, make sure backend auth is actually enabled, for example with `RAG_AUTH_ENABLED=true` and a matching configured key.

## Local x-api-key Example

For a local server on `http://127.0.0.1:4001` using the default shared key from the Makefile:

~~~bash
RAG_AUTH_ENABLED=true \
RAG_FRONTEND_API_KEY=replace-with-shared-frontend-backend-key \
make run
~~~

Then configure the MCP bridge like this:

~~~bash
RAG_MCP_API_BASE_URL=http://127.0.0.1:4001 \
RAG_MCP_TOOL_GROUPS=core,graph \
RAG_MCP_SEARCH_FORMAT=markdown \
RAG_MCP_HEADERS=x-api-key=replace-with-shared-frontend-backend-key \
/absolute/path/to/mcp-stdio
~~~

## Claude Code

~~~bash
claude mcp add rust-rag \
  --env RAG_MCP_API_BASE_URL=http://127.0.0.1:4001 \
  --env RAG_MCP_TOOL_GROUPS=core,graph \
  --env RAG_MCP_SEARCH_FORMAT=markdown \
  --env RAG_MCP_HEADERS=x-api-key=replace-with-shared-frontend-backend-key \
  -- /absolute/path/to/mcp-stdio
~~~

## Generic MCP JSON

~~~json
{
  "mcpServers": {
    "rust-rag": {
      "command": "/absolute/path/to/mcp-stdio",
      "env": {
        "RAG_MCP_API_BASE_URL": "http://127.0.0.1:4001",
        "RAG_MCP_TOOL_GROUPS": "core,graph",
        "RAG_MCP_HEADERS": "x-api-key=replace-with-shared-frontend-backend-key"
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
