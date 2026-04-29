# Messages API

Slack-like chat for human/agent communication. Persisted in SQLite (`messages` table). Same auth as other protected routes (session cookie, API key, or MCP bearer token).

## Endpoints

All routes are protected and live under the same auth as `/api/store` etc.

### `POST /api/messages`

Send a message.

**Body**

| Field         | Type                              | Required | Notes                                                                  |
|---------------|-----------------------------------|----------|------------------------------------------------------------------------|
| `channel`     | string                            | yes      | Channel/folder name (e.g. `general`, `ops`).                           |
| `text`        | string                            | yes      | Message body. Cannot be empty.                                         |
| `sender`      | string                            | no       | Trusted tokens may set this. Session-authenticated browser calls are stamped from the authenticated subject. |
| `sender_kind` | `"human"` \| `"agent"` \| `"system"` | no    | Session-authenticated browser calls are forced to `human`; MCP/agent tokens default to `agent`. |

**Response** `201 Created`

```json
{
  "id": "019dcf93-51a3-7c11-8a55-841d721b3b7d",
  "channel": "general",
  "sender": "alice",
  "sender_kind": "human",
  "text": "hi",
  "created_at": 1777304031651
}
```

### `GET /api/messages`

List messages with filters. Doubles as the presence-poll endpoint.

**Query params**

| Param        | Type                                | Default  |
|--------------|-------------------------------------|----------|
| `channel`    | string                              | —        |
| `sender`     | string                              | —        |
| `since`      | i64 (ms epoch, inclusive lower)     | —        |
| `until`      | i64 (ms epoch, inclusive upper)     | —        |
| `limit`      | usize                               | 100      |
| `offset`     | usize                               | 0        |
| `sort_order` | `"asc"` \| `"desc"`                 | `desc`   |
| `user`       | string                              | —        |
| `user_kind`  | `"human"` \| `"agent"` \| `"system"` | `human` |

When `channel` is set, the caller is registered as active in that channel for
the next 30 s (`PRESENCE_WINDOW_MS`). Session-authenticated browser calls use
the authenticated subject; trusted agent clients may still provide `user` /
`user_kind`. The response always includes `active_users` for the requested
channel.

**Response** `200 OK`

```json
{
  "messages": [
    {
      "id": "019dcf99-c01d-7bb2-ae51-ad9dd7eeab38",
      "channel": "general",
      "sender": "alice",
      "sender_kind": "human",
      "text": "hi",
      "created_at": 1777304453149
    }
  ],
  "total_count": 1,
  "active_users": [
    { "user": "bob",   "kind": "human", "last_seen": 1777304512304 },
    { "user": "alice", "kind": "human", "last_seen": 1777304511292 }
  ]
}
```

`active_users` is `[]` when no `channel` filter is provided.

### `GET /api/messages/channels`

List channels with their message count and last activity timestamp. Sorted by
`last_message_at DESC`.

**Response** `200 OK`

```json
{
  "channels": [
    { "channel": "general", "message_count": 12, "last_message_at": 1777304511292 }
  ]
}
```

## Presence

- In-memory only (`PresenceTracker` in `src/api/presence.rs`), per-process.
- 30 s active window. Entries are expired lazily on the next `list`.
- Not shared across replicas; fine for the current single-instance deploy.

## CLI

```bash
rag msg send -c general "hello"
rag msg history -c general --since 1777300000000 --limit 50
rag msg channels
```

`--sender` and `--kind` flags override defaults.

## MCP tools

Group: `messages` (default-on; override with `RAG_MCP_TOOL_GROUPS=core,messages`).

| Tool                    | Purpose                                  |
|-------------------------|------------------------------------------|
| `send_message`          | Post to a channel. `sender_kind` defaults to `agent`. |
| `message_history`       | Filter by channel/sender/since/until/limit/offset/sort_order. |
| `list_message_channels` | Channel index with counts.               |

## Frontend

Page: `/messages` (Next route, `frontend/app/messages/page.tsx`).

- Channel sidebar with inline create.
- Polls `GET /api/messages?channel=…&user=…` every 3 s via SWR.
- User identity: `/auth/session` → `preferred_username` | `name` | `email`,
  else a `guest-XXXXX` id stored in `localStorage` (`rag.messages.user`).
- Active-user badge in the thread header (count + first 5 names).
