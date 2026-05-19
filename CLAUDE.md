# CLAUDE.md — rust-rag

Guidance for Claude Code (and other agents) working in this repository.

## What this repo is

`rust-rag` — self-hosted retrieval + agent-collaboration backend. Axum HTTP API, Postgres + `pgvector` in prod (SQLite + `sqlite-vec` for local dev), ONNX embeddings, MCP surface (in-process at `/mcp`), Next.js frontend in `frontend/`.

Full architecture lives in entry `rust_rag_project_overview` (source `knowledge`). Read it before non-trivial work.

## Layout

- `src/` — main Rust API server. Entry point default bind: `http://127.0.0.1:4001`.
- `src/mcp.rs` — in-process MCP server mounted at `/mcp`.

- `frontend/` — Next.js app (server-side Zitadel OAuth, signed session cookie, proxies to Rust API).
- `assets/` — ONNX model files baked into Docker image.
- `deploy/kubernetes/` — k8s manifests (frontend only in prod).
- `docs/` — `setup-guide.md`, `mcp-setup.md`.

## Build / run

```bash
make run          # run main API server

cargo check       # type-check workspace
make docker-build # build server image
make k8s-apply    # apply k8s manifests
```



## Prod topology

Backend runs in-cluster as the CUDA Deployment (`deploy/kubernetes/rust-rag-cuda.yaml`), pinned to the GPU node `midi` via the NVIDIA device plugin. The Service `rag-service-cuda` fronts it; ingresses point there.

**Storage**: Postgres + `pgvector` is the authoritative store in prod. The connection string is injected via the `rust-rag-postgres` Secret as `RAG_DATABASE_URL`. When that env var is set, vector/message/auth/user-memory/oauth-credentials all route to Postgres; the SQLite handle at `/app/data/rag.db` (PVC `rust-rag-cuda-data`) is still opened on boot for backward-compat but is not the source of truth. Migrations are embedded (`migrations/*.sql`) and run automatically at startup.

For local development without `RAG_DATABASE_URL`, all stores fall back to SQLite — schema is identical in shape but kept in-sync manually between `src/db/schema.rs` and `migrations/*.sql`.

The legacy bare-metal host (`10.10.11.135`) and the selector-less `rag-service` DNS shim are retired — assume in-cluster.

## Shared memory: use this rust-rag instance

This project uses its own MCP server as durable cross-session, cross-agent memory. Default to reading and writing here.

**Project slug**: `rust-rag`
**Namespaces**: `project:rust-rag:knowledge`, `project:rust-rag:todos`. Cross-project evergreen facts go in `knowledge`.

### The RAG Bootstrap (Once per session)

1. `list_memory_conventions` — Fetch project taxonomy, metadata rules, and edge predicates.
2. `list_schemas` — Discover available typed-entry schemas (decision, todo, etc.).
3. `search_entries` — Query with `rerank: true`. Omit `source_id` first for global context, then narrow to project namespaces.
4. Read reference entries: `agent_collaboration_guide`, `rust_rag_project_overview`.

### Storage & Hand-off

1. **Structured First**: Use `type` + `data` in `store_entry` if a schema fits.
2. **Wiki Paths**: Use `path` (e.g., `features/auth`) for tree organization.
3. **Graphing**: Link entries via `create_manual_edge` using canonical predicates.
4. **Handoff**: `store_entry` outcomes with a stable ID, then `send_message` citing that ID.

### Reference entries (read once, trust them)

- `agent_collaboration_guide` — full collaboration protocol.
- `rust_rag_project_overview` — system architecture.
- `rust_rag_usage_guide_for_all_projects` — namespace conventions + standard loop.
- `rust_rag_claude_md_snippet` — template for other projects.

### Do NOT store

- Secrets, tokens, PII.
- Anything trivially derivable from `git log` or current source.
- Ephemeral conversation state.

## Conventions

- Update existing entries with `update_item` over creating duplicates.
- Tag liberally; tags drive future search narrowing.
- Link related entries with `create_manual_edge`.


## Observability — OpenTelemetry

In-cluster: traces ship to `otel-debug-service.monitoring.svc.cluster.local:4317`
(OTLP gRPC) when `RAG_OTEL_ENABLED=true` (default in cuda manifest). Layered
on top of the existing `tracing` fmt subscriber via `tracing-opentelemetry`,
filtered separately so console stays terse while exports stay rich.

Key knobs (cuda Deployment):

- `RAG_OTEL_ENABLED=true`
- `OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-debug-service.monitoring.svc.cluster.local:4317`
- `OTEL_SERVICE_NAME=rust-rag` (resource attribute; surfaces in queries)
- `RAG_OTEL_FILTER` — overrides the trace filter. Default
  `rust_rag=info,axum=info,tower_http=debug` (tower-http request spans live at
  DEBUG; exclude that and there are no spans to export).

### Inspecting traces

The collector is `matst80/otel-debug` — minimal viewer + JSON history API.
Public ingress at `https://otel.k6n.net`.

History endpoints (port 8080 internally):

| Endpoint | Use |
|---|---|
| `GET /api/history` | Every signal type |
| `GET /api/history/traces` | Traces only |
| `GET /api/history/metrics` | Metrics only |
| `GET /api/history/logs` | Logs only |
| `GET /api/history/search?q=<text>` | Full-text search across signals |
| `GET /api/history/wait?q=<text>&timeout=<sec>` | Block until matching signal arrives, or `timeout waiting for signal` |

All accept `?limit=N`. `wait` accepts `?timeout=<sec>` (default short).

Filter rust-rag spans:

```bash
curl -s 'https://otel.k6n.net/api/history/traces?limit=500' \
  | jq '[.[] | select(.data.resourceSpans[].resource.attributes[]
        | select(.key=="service.name" and .value.stringValue=="rust-rag"))]'
```

Wait for a specific span to appear (handy for live debugging):

```bash
curl -sS --max-time 30 \
  'https://otel.k6n.net/api/history/wait?q=POST%20/api/store&timeout=25'
```

UI at `https://otel.k6n.net/`. The viewer is a SPA; trace data lives behind
`/api/history/*`, not on the SPA paths.

### Adding spans

`#[tracing::instrument]` on hot paths gets you nested children inside the
tower-http request span. The OTel layer auto-converts every `tracing::span!`
to an OTel span and bridges `info!`/`warn!` events as span events.
