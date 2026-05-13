# MCP Setup

> **Agent Integration**
> rust-rag supports the Model Context Protocol (MCP) via **SSE** (direct HTTP).

## Transport Options

### 1. Direct SSE Transport (Recommended)
Once you have an MCP token, you can connect directly to the `/mcp` endpoint of your rust-rag server. This is the simplest setup for clients that support remote MCP servers (like Claude Code).

#### Claude Code
~~~bash
claude mcp add --transport http rust-rag https://your-rag-server.com/mcp \
  --header "Authorization: Bearer your-mcp-token"
~~~

#### Raw SSE Client Note
If you are implementing a raw SSE client (e.g., using `curl`), the first response from `/mcp` will include an `Mcp-Session-Id` header. **You must echo this header back in all subsequent POST requests** for that session so the server can route your messages to the correct in-process state.


## Authentication & Device Login

The recommended way to authenticate agents is via the **OAuth 2.0 Device Authorization Grant**. This allows you to log in via a browser and mint a long-lived `rag_mcp_*` token for your agent.


### Manual/Custom Integration (like Chrome Extension)
If you are building your own integration, use these endpoints:

1.  **Request a Code**: `POST /auth/device/code`
    - Request: `{ "client_name": "My Custom Agent" }`
    - Response: `{ "device_code", "user_code", "verification_uri", "verification_uri_complete", "interval" }`
2.  **Display to User**: Show the `user_code` and provide a clickable link to `verification_uri_complete`.
3.  **Poll for Token**: `POST /auth/device/token`
    - Request: `{ "device_code": "..." }`
    - The server will return `400 {"error": "authorization_pending"}` until the user approves.
    - On success, it returns: `{ "access_token": "rag_mcp_...", "token_type": "Bearer" }`

### Direct API Keys (Legacy/CI)
If you are using a shared API key configured via `RAG_API_KEYS`, you can bypass the device flow:
- **SSE**: Add the header `Authorization: Bearer <key>` or `x-api-key: <key>`.


## Configuration

The following server settings affect tool behavior:

- `RAG_MCP_TOOL_GROUPS` - comma-separated groups: `core`, `admin`, `graph`. Default: `core`.
- `RAG_MCP_SEARCH_FORMAT` - `markdown` is recommended for agents.
- `RAG_MCP_ALLOWED_HOSTS` - (Server-side) Must include your public hostname for SSE to work.


- GitHub: https://github.com/matst80/rust-rag
- Releases: https://github.com/matst80/rust-rag/releases
- Detailed MCP README: [readme_mcp.md](../readme_mcp.md)
