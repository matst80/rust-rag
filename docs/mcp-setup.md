# MCP Setup

> **Agent Integration**
> rust-rag supports the Model Context Protocol (MCP) via two transports: **stdio** (via a bridge binary) and **SSE** (direct HTTP).

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

### 2. stdio Bridge (`mcp-stdio`)
The `mcp-stdio` bridge binary launches as a subprocess and forwards tool calls to the HTTP API. This is useful for clients that only support stdio-based MCP servers.

#### Setup Flow
1. Download the latest `mcp-stdio` release binary from GitHub Releases.
2. Log in to get a token: `mcp-stdio login --base-url https://your-rag-server.com`
3. Register the binary in your MCP client.

#### Claude Code
~~~bash
claude mcp add rust-rag \
  -- /absolute/path/to/mcp-stdio
~~~

#### Generic MCP JSON
~~~json
{
  "mcpServers": {
    "rust-rag": {
      "command": "/absolute/path/to/mcp-stdio"
    }
  }
}
~~~

## Authentication & Device Login

The recommended way to authenticate agents is via the **OAuth 2.0 Device Authorization Grant**. This allows you to log in via a browser and mint a long-lived `rag_mcp_*` token for your agent.

### 1. Using the Bridge (`mcp-stdio login`)
The simplest way to get a token for local use:
~~~bash
mcp-stdio login --base-url https://your-rag-server.com
~~~
1. It will print a **User Code** (e.g., `ABCD-1234`) and a **Verification Link**.
2. Open the link, sign in to your rust-rag instance, and enter the code.
3. The bridge will automatically poll for the token and save it to `~/.config/rust-rag/mcp-token`.

### 2. Manual/Custom Integration (like Chrome Extension)
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
- **stdio**: Set the environment variable `RAG_MCP_AUTH_BEARER=<key>`.

## Configuration

The following environment variables (for `mcp-stdio`) or server settings affect tool behavior:

- `RAG_MCP_TOOL_GROUPS` - comma-separated groups: `core`, `admin`, `graph`. Default: `core`.
- `RAG_MCP_SEARCH_FORMAT` - `markdown` is recommended for agents.
- `RAG_MCP_ALLOWED_HOSTS` - (Server-side) Must include your public hostname for SSE to work.

## Release Artifacts

The repository publishes Linux amd64 and arm64 mcp-stdio archives when a tag matching `mcp-stdio-v*` is pushed.

- GitHub: https://github.com/matst80/rust-rag
- Releases: https://github.com/matst80/rust-rag/releases
- Detailed MCP README: [readme_mcp.md](../readme_mcp.md)
