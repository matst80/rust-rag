# rust-rag — usage guide

Operational reference for the live API surface. Companion to `setup-guide.md`
(install/build) and `mcp-setup.md` (MCP client wiring).

## Auth

`/api/*` and `/mcp` are gated by `RAG_AUTH_ENABLED`. Three accepted credentials:

- `Authorization: Bearer <api-key>` — value of `RAG_FRONTEND_API_KEY` (also
  used by the Next.js BFF when proxying to `/api/*`).
- `Authorization: Bearer <mcp-token>` — provisioned via the `/auth/tokens`
  flow; intended for MCP clients (Claude Desktop, Cursor, etc.).
- Session cookie — set by the Zitadel OAuth flow on the frontend.

Unauthenticated paths: `/healthz`, `/assets/*`, the OAuth login routes.

---

## Entries: store, search, navigate

### Store an entry with a wiki path

`POST /api/store`

```json
{
  "id": "team_handbook_onboarding",
  "text": "# Onboarding\n\n…",
  "metadata": { "author": "mats", "tags": ["wiki"] },
  "source_id": "project:rust-rag:knowledge",
  "path": "team/handbook/onboarding"
}
```

`path` is optional. Slash-separated, normalized server-side (rejects `..`,
absolute paths, empty segments). Distinct from chunk-level `section_path`,
which is derived from markdown headers by the chunker.

### Search

`POST /api/search`

```json
{
  "query": "db failover steps",
  "top_k": 10,
  "source_id": "project:rust-rag:knowledge",
  "rerank": true
}
```

Hybrid (dense + sparse) is on by default. Reranking is opt-in over HTTP and
on-by-default for MCP callers.

### Wiki tree navigation

Two endpoints:

- `GET /api/entries/paths?source_id=<optional>` — every distinct
  `(source_id, path)` with entry counts in one round-trip. Used by the
  frontend sidebar to build the full tree client-side.
- `GET /api/entries/tree?source_id=<id>&prefix=<optional path>` — direct
  child segments + leaf entries at the given prefix. Used for drill-down
  rendering.

Example tree response:

```json
{
  "source_id": "project:rust-rag:knowledge",
  "prefix": "engineering",
  "children": [
    { "segment": "runbooks", "count": 2, "has_children": false }
  ],
  "entries": [
    { "id": "engineering_overview", "path": "engineering", … }
  ]
}
```

### List with path filter

`GET /admin/items?source_id=...&path_prefix=team`

`path_prefix` matches the prefix itself or anything under it (case-insensitive).

---

## Typed Entries and Schemas

Entries can be stored as structured data instead of raw text. This enables schema validation and specialized frontend views.

### List Schemas

`GET /api/schemas`

Returns registered schemas with their `type_name`, `json_schema`, and `item_count`.

### Store Typed Entry

`POST /api/store`

```json
{
  "id": "dec_auth_strategy",
  "text": "Using Zitadel for OAuth2 device flow.",
  "source_id": "project:rust-rag:knowledge",
  "type": "decision",
  "data": {
    "title": "Auth Strategy",
    "status": "accepted",
    "rationale": "Built-in support for device flow and OIDC.",
    "alternatives": ["Auth0", "Keycloak"]
  },
  "metadata": { "author": "mats", "tags": ["auth"] }
}
```

When `type` is set, `data` MUST validate against the registered schema for that type.

---

## Attachments

Files bind to existing entries. On disk under `RAG_UPLOAD_PATH`
(`/app/data/uploads` in the cuda Deployment). Served read-only via
`/assets/<stored_name>`. Cascade-deleted when the parent entry goes away.

### Upload (multipart)

```
POST /api/attachments
Content-Type: multipart/form-data
Fields: item_id (text), file (binary)
```

Returns `{id, item_id, filename, stored_name, url, mime, size, sha256, created_at}`.

### Attach a remote URL (server-side fetch)

```
POST /api/attachments/from-url
{
  "item_id": "engineering_overview",
  "url": "https://example.com/handbook.pdf",
  "filename": "handbook.pdf"
}
```

SSRF guard:

- Only `http`/`https` schemes.
- Resolves the host; rejects private/loopback/link-local/multicast IPs.
- Cap on response size (`RAG_ATTACHMENT_MAX_BYTES`, default 25 MiB).
- 30s timeout, max 3 redirects, IP re-resolve on each hop.

### List / delete

```
GET    /api/items/{id}/attachments
DELETE /api/attachments/{id}
```

`DELETE` removes the row and the on-disk file.

---

## ACP discovery & registration

mDNS browse (`_acp-ws._tcp`) is the default discovery mechanism. The cuda
Deployment runs in a k8s pod whose subnet can't see LAN multicast, so it
runs in HTTP-only mode (`RAG_ACP_DISCOVERY_MODE=http`). Clients announce
themselves over HTTP.

### Register (LAN client → in-cluster server)

```
POST /api/acp/register
{
  "name": "acp-ws-9001",
  "host": "10.10.11.50",
  "port": 9001,
  "url": "ws://10.10.11.50:9001/",
  "txt": { "version": "1", "auth": "bearer" }
}
```

`url` is optional (defaults to `ws://host:port/`). First registration
auto-selects so `acp_ws` connects immediately. On select, the manager's
`acp_ws` handle calls `set_target(url, token)` and reconnects.

### Heartbeat

```
POST /api/acp/heartbeat
{ "name": "acp-ws-9001" }
```

Required every ≤ `RAG_ACP_REGISTER_TTL_SECS` (default 120s) to stay in
the registry. The janitor prunes silent registrations every 15s.

### Unregister

```
DELETE /api/acp/register/{name}
```

### Inspect / select

```
GET  /api/acp/instances
POST /api/acp/select   { "name": "acp-ws-9001" }
```

`source` is `"mdns"` or `"registered"` — lets the UI distinguish.

### Suggested client loop

```
on startup:  POST /api/acp/register
every 60s:   POST /api/acp/heartbeat
on shutdown: DELETE /api/acp/register/{name}
```

---

## Manager agent: ACP tools

The manager (LLM loop in `src/manager.rs`) exposes a tool surface for
driving ACP sessions through the in-cluster `acp_ws` connection. Available
tools, all routed via the active `acp_ws` target:

| Tool | Purpose |
|---|---|
| `acp_list_sessions` | Trigger a fresh ListSessions WS message; read result via `acp_recent_events`. |
| `acp_spawn` | Start a new headless session: `{project_path, agent_command?, metadata?}`. |
| `acp_send_prompt` | Send a prompt to an existing session. |
| `acp_cancel` | Cancel the running prompt on a session. |
| `acp_end_session` | Gracefully terminate a session. |
| `acp_set_permission_mode` | `auto` vs `manual` tool-call approval. |
| `acp_set_config` | Per-session config option. |
| `acp_permission_respond` | Reply to a `PermissionRequest` (`allow_once`, `allow_always`, `deny`, `deny_always`). |
| `acp_recent_events` | Read recent events from the ring buffer (~200/session). Filter by `session_id`, `since_local_seq`, `kinds`. |
| `acp_pending_permissions` | List outstanding `PermissionRequest` events. |
| `acp_bind_telegram_thread` | Bind a session to a Telegram forum topic. |
| `acp_get_snapshot` | Latest Snapshot event for the active connection. |

Combined with the ACP register endpoint, the full path is:

```
LAN client → /api/acp/register
           ↓ on_select
           acp_ws.set_target(url)
           ↓ WebSocket
           manager tools (acp_spawn, acp_send_prompt, …)
```

---

## MCP tools

Most HTTP endpoints have an MCP equivalent under `/mcp` (in-process). Notable additions:

- `store_entry` / `update_item` / `list_items` — accept `path`.
- `list_entry_tree {source_id, prefix?}` — same shape as the HTTP tree.
- `attach_url {item_id, url, filename?}` — SSRF-guarded attachment.
- `list_attachments {id}` / `delete_attachment {id}`.
- `search_entries` — defaults `rerank: true` for MCP callers.


---

## Environment knobs

| Var | Default | Effect |
|---|---|---|
| `RAG_AUTH_ENABLED` | `false` | Gate `/api/*` and `/mcp`. |
| `RAG_UPLOAD_PATH` | `uploads` | Where attachments + ingested images land. |
| `RAG_ATTACHMENT_MAX_BYTES` | `26214400` (25 MiB) | Per-file cap (multipart + URL fetch). |
| `RAG_ACP_DISCOVERY_MODE` | `mdns` | Set to `http` to skip mDNS daemon (k8s). |
| `RAG_ACP_REGISTER_TTL_SECS` | `120` | Heartbeat deadline before prune. |
| `RAG_RERANKER_DEFAULT` | `false` | Server-side default for `rerank` on `/api/search` (MCP overrides to `true`). |
| `RAG_RERANKER_TOP_N` | `15` | Candidate pool size for cross-encoder rerank. |

Full list lives in `src/config/mod.rs`.
