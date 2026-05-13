# MCP Access for rust-rag

rust-rag speaks the Model Context Protocol via **`/mcp` in-process** — a streamable-HTTP transport mounted directly on the Axum server. MCP clients that support remote servers (Claude Code, etc.) connect to it straight, no bridge process required.

Both surfaces authenticate with long-lived bearer tokens minted through an
OAuth 2.0 device authorization grant (RFC 8628). Tokens are bound to a Zitadel
identity, stored hashed, and revocable per-token from the UI.

## Table of contents

- [Overview](#overview)
- [Server configuration](#server-configuration)

- [Client: remote MCP at `/mcp`](#client-remote-mcp-at-mcp)
- [Auth endpoints](#auth-endpoints)
- [Frontend routes](#frontend-routes)
- [Token format and lifecycle](#token-format-and-lifecycle)
- [Security notes](#security-notes)
- [Caveats and roadmap](#caveats-and-roadmap)

## Overview

```
┌────────────────┐    user_code    ┌────────────────┐    approve     ┌──────────────┐
│  MCP client    │ ──────────────> │ Next.js UI     │ ─────────────> │  rust-rag    │
│ (Claude etc.)  │                 │ /auth/device   │  session cookie│  backend     │
└────┬───────────┘                 └────────────────┘                └──────┬───────┘
     │                                                                     │
     │ poll /auth/device/token                                              │
     │ <─── access_token ─────────────────────────────────────────────────┘
     │
     │ Authorization: Bearer rag_mcp_…
     ▼
┌────────────────┐
│ /mcp (in-proc) │
│  streamable-   │
│  HTTP server   │
└────────────────┘
```

## Server configuration

All auth/MCP knobs are environment variables on the `rust-rag` binary. They
extend the existing auth surface (`RAG_FRONTEND_API_KEY`, `RAG_API_KEYS`,
`AUTH_SESSION_SECRET`).

| Variable | Default | Purpose |
|---|---|---|
| `RAG_APP_BASE_URL` | — | Absolute base URL the frontend is served at. Used as the `verification_uri` returned in device-code responses, so CLIs can print a clickable link. Falls back to `APP_BASE_URL`. |
| `RAG_DEVICE_CODE_TTL_SECS` | `600` | How long a device code stays valid between issuance and approval. |
| `RAG_DEVICE_CODE_INTERVAL_SECS` | `5` | Minimum seconds between polls on `/auth/device/token`. CLIs that poll faster get `slow_down`. |
| `RAG_MCP_TOKEN_TTL_DAYS` | — (never) | Expiry for minted MCP tokens. Leave unset for never-expire; add a number for rotation policies. |
| `RAG_MCP_ALLOWED_HOSTS` | `localhost,127.0.0.1,::1` | Comma-separated hostnames/authorities allowed on the `Host` header of `/mcp`. Defends against DNS rebinding. **Must include your public hostname** when exposing `/mcp` externally. |

Auth is considered enabled when any of `RAG_FRONTEND_API_KEY`,
`AUTH_SESSION_SECRET`, or `RAG_API_KEYS` is set (or when `RAG_AUTH_ENABLED=true`
explicitly). Device-code and `/mcp` endpoints only gate on these.

The frontend shares `AUTH_SESSION_SECRET` with the backend so a `rag_session`
cookie minted in Next.js validates directly on the Axum side — that's how the
device-approval handshake flows from browser → Next → backend without a second
round of OAuth.


## Client: remote MCP at `/mcp`

Once you have a token, any client that speaks the
[Streamable HTTP MCP transport](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports)
can connect straight to rust-rag. No bridge process.

### Claude Code

```bash
claude mcp add --transport http rust-rag https://rag.example.com/mcp \
  --header "Authorization: Bearer $(cat ~/.config/rust-rag/mcp-token)"
```

### Raw JSON config

```json
{
  "mcpServers": {
    "rust-rag": {
      "url": "https://rag.example.com/mcp",
      "headers": {
        "Authorization": "Bearer rag_mcp_..."
      }
    }
  }
}
```

### Curl sanity check

```bash
curl -N https://rag.example.com/mcp \
  -H "Authorization: Bearer $(cat ~/.config/rust-rag/mcp-token)" \
  -H "Accept: application/json, text/event-stream" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

The first response includes an `Mcp-Session-Id` header; subsequent
requests must echo it back so rmcp can route to the right in-process session.

`/mcp` exposes the core tool surface:

- **Core**: `health_status`, `store_entry`, `search_entries`, `get_entry`.
- **Admin**: `list_categories`, `list_items`, `update_item`, `delete_item`.
- **Graph**: `graph_status`, `list_graph_edges`, `graph_neighborhood`,
  `rebuild_graph`, `create_manual_edge`, `delete_graph_edge`.

Implementation lives in `src/mcp.rs` and reuses the same

## Auth endpoints

All under the Axum server. Public routes accept no auth; session routes
require the `rag_session` cookie; protected routes accept any bearer
(`RAG_API_KEYS`, `RAG_FRONTEND_API_KEY`, or an MCP token).

### Public

| Method | Path | Request | Response |
|---|---|---|---|
| `POST` | `/auth/device/code` | `{ "client_name": "optional label" }` | `{ device_code, user_code, verification_uri, verification_uri_complete, expires_in, interval }` |
| `POST` | `/auth/device/token` | `{ "device_code": "..." }` | `200 { access_token, token_type, token_id, expires_at }` or `400 { error: "authorization_pending" \| "slow_down" \| "access_denied" \| "expired_token" \| "invalid_grant" }` |

### Session-protected

Require the `rag_session` cookie (same JWT the Zitadel callback sets).

| Method | Path | Purpose |
|---|---|---|
| `GET`    | `/auth/device` | Server-rendered HTML fallback form (when no frontend is deployed). |
| `GET`    | `/auth/device/verify?user_code=…` | Look up a pending user-code to render context in the UI. |
| `POST`   | `/auth/device/approve` | `{ user_code, name? }` → mints a token bound to the caller's `sub`. |
| `GET`    | `/api/auth/tokens` | Lists tokens belonging to the caller. |
| `DELETE` | `/api/auth/tokens/{id}` | Revokes a token. |

### Protected (any bearer)

`/mcp` plus every existing `/search`, `/store`, `/admin/*`, `/graph/*` route.
The bearer can be an API key or an `rag_mcp_*` token.

## Frontend routes

- `/auth/device?user_code=…` — approval page. Redirects to
  `/auth/login?returnTo=…` if not signed in.
- `/auth/tokens` — list + revoke. New nav item (`KeyRound` icon) appears in
  the header when the user is signed in.

Both pages are server components that read the session via
`readSessionFromCookies()` (uses `next/headers`).

Next.js server routes forward to the backend with the session cookie
preserved:

- `POST /api/device/approve` → backend `/auth/device/approve`
- `GET  /api/device/verify` → backend `/auth/device/verify`

Token management (`/api/auth/tokens`, `/api/auth/tokens/{id}`) is routed to
the backend via the ingress, while `/auth/tokens` is the frontend page route.
The browser talks to the backend same-origin and the session cookie rides along.
The backend route also handles its own session validation, so no Next.js
proxy layer is needed.

The auth path prefix otherwise stays owned by Next (callback, login, logout,
session) — only the explicitly-listed `/auth/*` paths are forwarded by the
ingress.

## Token format and lifecycle

- **Format**: `rag_mcp_<43 base64url chars>`. 256 bits of entropy from
  `getrandom`.
- **Storage**: only the SHA-256 hex of the plaintext is stored (`mcp_tokens`
  table). The plaintext is returned exactly once — on the `/auth/device/token`
  poll that follows approval.
- **User codes**: 8 chars from a Crockford-ish alphabet (no O/0/I/1/L),
  rendered `XXXX-XXXX`. Random per-request, stored in `device_auth_requests`.
- **Subject**: each token has a `subject` column holding the Zitadel `sub` of
  the approving user. `GET /api/auth/tokens` and `DELETE /api/auth/tokens/{id}` are
  filtered by this column so users only see / revoke their own.
- **`last_used_at`**: updated out-of-band (spawned task) on each successful
  bearer auth so it never blocks the request path.
- **Expiry**: unset by default. Set `RAG_MCP_TOKEN_TTL_DAYS=90` (or similar)
  to force rotation. Expired tokens are rejected by the middleware but not
  auto-deleted — clean them up with the revoke API if you care.

## Security notes

- `rag_mcp_*` tokens and `RAG_API_KEYS` are interchangeable for protected
  routes. Treat them with equal care.
- The `require_api_key` middleware checks, in order: API key, MCP token,
  session cookie. Explicit API keys win over tokens, so a misconfigured
  shared key won't silently defer to token lookup.
- `/mcp` enforces `Host` header validation via rmcp's
  `allowed_hosts`. Default allows loopback only — **public deployments must
  set `RAG_MCP_ALLOWED_HOSTS`** to include the real hostname or the endpoint
  returns `421`.
- Device approval is tied to the `rag_session` cookie which is `HttpOnly`,
  `SameSite=Lax`, `Secure` when `APP_BASE_URL` is HTTPS. Standard web-session
  CSRF posture; the POST body includes no sensitive input (just
  `user_code` + optional label), so CSRF on this specific endpoint is low-
  value.
- If you accept inbound browser traffic directly on the rust-rag backend
  (not behind the Next.js frontend), make sure `RAG_APP_BASE_URL` points to
  the origin you actually serve the UI from — the inline backend fallback at
  `GET /auth/device` is convenience, not production UX.

## Caveats and roadmap

### Pending token cache (single-replica assumption)

Approval and the token-poll response are bridged by an **in-process**
`PendingTokenCache`. If a poll lands on a different backend replica than the
approve call, the poll returns `invalid_grant`. Same thing if the process
restarts between approve and poll.

Fine for a single instance. For multi-replica deploys, stash the
(single-use) plaintext in `device_auth_requests` and read-then-delete in the
token handler — a short follow-up, no new columns beyond `approved_token`.

### Stateful MCP sessions

`StreamableHttpServerConfig::stateful_mode = true`. Sessions are kept in
rmcp's `LocalSessionManager` (in-memory). Load balancers fronting multiple
replicas must pin on `Mcp-Session-Id` or flip `stateful_mode` off (simple
request/response, no resumable SSE).

### No scopes yet

All MCP tokens have the same capability set. An admin-only revocation
doesn't restrict which tools the token can call. A future `scopes` column
on `mcp_tokens` + a `require_scope("admin")` check on the protected router
tier would give per-token least-privilege.

### Frontend build

The frontend changes pass `tsc --noEmit`. ESLint wasn't installed locally;
run `pnpm build` in `frontend/` before deploying to catch anything the type
checker doesn't.

### Related files

- `src/api/auth.rs` — device-code + token endpoints.
- `src/api/mod.rs` — `require_api_key` (accepts `rag_mcp_*`), router wiring,
  `store_entry_core` / `search_core` extracted for reuse.
- `src/mcp.rs` — in-process MCP server + `streamable_http_service()`.
- `src/db/mod.rs` — `mcp_tokens`, `device_auth_requests` schema + `AuthStore`
  trait.
- `frontend/app/auth/device/`, `frontend/app/auth/tokens/`,
  `frontend/components/auth/` — Next.js UI.
- `frontend/app/api/device/` — session-aware proxies to the backend device-
  auth endpoints (approve/verify).
- `deploy/kubernetes/rust-rag-ingress.yaml` — routes `/api/auth/tokens*` and
  selected `/auth/device/*` paths straight to the backend.
