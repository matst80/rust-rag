# go-cli TODO — entry-focused CLI

## Scope decision

The `go-cli` is being narrowed to an **entry/attachment tool**. Drop all ACP
functionality (both the local process bridge and any thoughts of wrapping the
MCP `acp_*` tools). ACP belongs in the MCP surface and dedicated daemons, not
here.

Keep / focus on:
- storing entries (free-text + typed via schemas)
- searching / listing / browsing entries
- updating + deleting entries
- attachments (attach/list/delete)
- triggering analysis (`analyze_entry` dry-run, `dream` consolidation)
- discovering related nodes via the graph (`neighborhood`, `list_edges`)
- schema discovery (`list_schemas`, `get_schema`) so users can pick a `type`

Out of scope (remove):
- `acp run | gemini | copilot | bridge` subcommands
- the `acp/` package (bridge.go, conn.go, names.go, types.go)
- messaging (`msg send|history|channels`) — chat is not the CLI's job; revisit
  later if needed but it is not entry-related

Keep `login` (device-code OAuth) — every entry call needs the token.

## Current CLI commands (main.go)

- `login` — device-code OAuth → writes `access_token` to config. **Keep.**
- `store [text]` — POST `/api/store` with `text`, `source_id`, `metadata`. **Keep + expand** (see below).
- `search <query>` — POST `/api/search`, flags: `--limit`, `--source`. **Keep + expand.**
- `list` — GET `/admin/items`, flags: `--source`, `--limit`. **Keep + expand.**
- `msg send | history | channels` — **Remove.**
- `acp run | gemini | copilot | bridge` — **Remove.** Delete `acp/` package.

## Missing entry-related features (vs MCP)

Reference: tool list in [src/mcp.rs](src/mcp.rs).

### Entries
- `store_entry` accepts much more than the CLI exposes: `type` + `data` (typed schema), `path` (wiki tree), `tags`, `title`, `summary`, edges. CLI only sends `text`/`source_id`/`metadata`.
- `update_item` — no CLI equivalent. Need `rag entry update <id>` with flags for text/metadata/path/tags/type/data.
- `delete_item` — no CLI equivalent. `rag entry delete <id>`.
- `list_items` supports `path_prefix` and `type` filters; CLI `list` only does `source_id`.
- `browse_path` — hierarchical wiki browse; no CLI equivalent. `rag entry browse --source X --prefix features/`.
- `list_sources` — list source_id buckets + counts. `rag sources`.
- `list_memory_conventions` — show canonical namespaces/predicates. `rag conventions`.

### Search
- `search_entries` supports `rerank` toggle, `type` filter, and returns `related` items linked from the top hit. CLI `search` exposes none of this. Add `--rerank/--no-rerank`, `--type`, and print related entries.

### Schemas (typed entries)
- `list_schemas` — `rag schema list`
- `get_schema <type>` — `rag schema get <type>`
- (skip `upsert_schema` / `delete_schema` for v1 — admin-only; reconsider later)

### Graph / related nodes ← **user explicitly wants this**
- `neighborhood <id>` — return nodes/edges around an entry. `rag related <id>` (primary verb the user asked for).
- `list_edges` — `rag edges --item <id>` / `--type <edge_type>`.
- `create_manual_edge` — `rag edge add <from> <to> <predicate>`.
- (skip `delete_edge`, `update_edge`, `rebuild_edges`, `graph_status` for v1)

### Attachments
- `attach_file` — `rag attach <entry_id> <url>` (server fetches HTTP/HTTPS with SSRF guards).
- `list_attachments` — `rag attachments <entry_id>`.
- `delete_attachment` — `rag attach rm <attachment_id>`.

### Analysis ← **user explicitly wants this**
- `analyze_entry` — dry-run LLM analysis (neighbors, classification, suggested tags/edges) without writing. `rag analyze <id-or-text>`.
- `dream` — manual consolidation round. `rag dream`.

### Ops
- `health` — `rag health`.

## Proposed command tree

```
rag login
rag entry store [text]            # -s source, -m metadata, --type, --data, --path, --tags, --title
rag entry get <id>
rag entry update <id>             # any of --text/--metadata/--path/--tags/--type/--data
rag entry delete <id>
rag entry list                    # -s source, --path-prefix, --type, -n limit
rag entry browse                  # -s source, --prefix
rag entry related <id>            # neighborhood
rag entry analyze <id|-->         # analyze_entry (id OR text from stdin)
rag search <query>                # -k, -s, --type, --rerank/--no-rerank, --show-related
rag sources
rag schema list
rag schema get <type>
rag edge list                     # --item, --type
rag edge add <from> <to> <predicate>
rag attach add <entry_id> <url>
rag attach list <entry_id>
rag attach rm <attachment_id>
rag dream
rag health
rag conventions
```

(Alias top-level `rag store`, `rag search`, `rag list` to the `entry` variants
for backwards compat if needed.)

## Implementation notes

- All endpoints already exist on the HTTP API — the MCP layer just wraps them. Grep [src/api](src/api/) and [src/mcp.rs](src/mcp.rs) for the canonical request shapes before adding CLI flags.
- Most new commands are thin POST/GET wrappers like the existing `store`/`search`. The hard part is `entry update` (many optional fields → use a small struct + `omitempty`).
- Output formatting: keep current human-readable default; consider a `--json` global flag for scripting (many existing commands would benefit).
- After dropping `acp/`, `go.mod` will lose dependencies — run `go mod tidy`.

## Order of work (suggested)

1. Rip out `acp/` package, `acpCmd` + subcommands, `msgCmd` + subcommands. `go mod tidy`. Update `README.md`.
2. Restructure under `rag entry ...` group; keep top-level `store`/`search`/`list` as aliases.
3. Add `entry get`, `entry update`, `entry delete`, `entry related`, `entry analyze`.
4. Add attachments group.
5. Add schemas + sources + conventions + edges + health + dream.
6. Add `--json` global flag for machine-readable output.
