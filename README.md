# rust-rag

`rust-rag` is a local retrieval backend with an Axum HTTP API, SQLite/sqlite-vec storage, and an ONNX embedding pipeline.

## Documentation

- [Start Guide](docs/setup-guide.md) - Product overview and search workflow.
- [MCP Setup](docs/mcp-setup.md) - Agent integration and bridge configuration.

## Authentication

The web app now uses a server-side authorization-code flow against Zitadel. The browser never talks directly to the Rust API anymore; the Next.js server exchanges the code, stores a signed session cookie, and proxies authenticated requests upstream with an internal API key.

Frontend environment variables:

- `APP_BASE_URL` - public URL of the Next.js app, for example `http://127.0.0.1:3000`
- `AUTH_SESSION_SECRET` - shared secret used to sign the session cookie
- `ZITADEL_ISSUER` - Zitadel issuer URL
- `ZITADEL_CLIENT_ID` - Zitadel application client ID
- `ZITADEL_CLIENT_SECRET` - Zitadel application client secret
- `ZITADEL_REDIRECT_URI` - optional override for the callback URL. Default: `${APP_BASE_URL}/auth/callback`
- `ZITADEL_SCOPES` - optional scopes. Default: `openid profile email`
- `RAG_API_URL` - Rust API base URL used by the Next.js proxy. Default: `http://127.0.0.1:4001`
- `RAG_FRONTEND_API_KEY` - shared key the Next.js proxy sends to the Rust API

Rust API environment variables:

- `RAG_AUTH_ENABLED` - optional explicit toggle. If omitted, auth is enabled automatically when any API key is configured
- `RAG_FRONTEND_API_KEY` - shared key accepted from the authenticated Next.js proxy
- `RAG_API_KEYS` - optional comma-separated direct access keys for MCP or external clients. Each entry can be `name:value` or just `value`

Direct API and MCP clients can authenticate with either `x-api-key: <key>` or `Authorization: Bearer <key>`.


## HTTP server

Start the main API server with the existing environment variables described in `src/config/mod.rs`:

```bash
cargo run
```

The default bind address is `http://127.0.0.1:4001`.

Local helper targets are available in the Makefile:

```bash
make run
make run-mcp
```

## MCP stdio bridge

The workspace also includes `mcp-stdio`, a stdio MCP server that forwards tool calls to the HTTP API.

```bash
cargo run -p mcp-stdio
```

Supported bridge environment variables:

- `RAG_MCP_API_BASE_URL` - rust-rag HTTP base URL. Default: `http://127.0.0.1:4001`
- `RAG_MCP_TIMEOUT_SECS` - outbound HTTP timeout in seconds. Default: `30`
- `RAG_MCP_TOOL_GROUPS` - comma-separated tool groups to expose: `core`, `admin`, `graph`
- `RAG_MCP_AUTH_BEARER` - optional bearer token added to upstream HTTP requests
- `RAG_MCP_HEADERS` - optional semicolon-separated extra headers as `Name=Value;Other=Value`
- `RAG_MCP_SERVER_NAME` - MCP server name reported during initialization
- `RAG_MCP_SERVER_VERSION` - MCP server version reported during initialization
- `RAG_MCP_SERVER_INSTRUCTIONS` - optional MCP server instructions text

When the Rust API is protected with `RAG_API_KEYS`, configure MCP with either `RAG_MCP_AUTH_BEARER=<api-key>` or `RAG_MCP_HEADERS=x-api-key=<api-key>`.

Tool groups map directly to the existing HTTP surface:

- `core`: health, store, search
- `admin`: categories, list/update/delete items
- `graph`: graph status, edge listing, neighborhood lookup, rebuild, manual edge create/delete

### Release workflow

GitHub Actions publishes `mcp-stdio` release archives for Linux `amd64` and `arm64` only when a tag matching `mcp-stdio-v*` is pushed.

Create the expected annotated tag locally with:

```bash
make tag-mcp-stdio VERSION=0.1.0
git push origin mcp-stdio-v0.1.0
```

That tag triggers `.github/workflows/release-mcp-stdio.yml`, which builds the `mcp-stdio` binary for both architectures and attaches `.tar.gz` archives plus SHA-256 checksum files to the GitHub release.

## Container image

Build the server container image from the repo root:

```bash
make docker-build
```

Run it locally with the SQLite data directory mounted from the host:

```bash
make docker-run
```

The image bakes in the model assets from `assets/` and persists the database under `/app/data/rag.db`.

## Kubernetes

The server deployment manifest is in `deploy/kubernetes/rust-rag.yaml`. It includes:

- a `ConfigMap` for server environment variables
- a `PersistentVolumeClaim` for the SQLite database
- a single-replica `Deployment` with startup, readiness, and liveness probes
- a `ClusterIP` `Service` on port `4001`

Apply or remove it with:

```bash
make k8s-apply
make k8s-delete
```

Before applying in a real cluster, set the `image:` field in the manifest to the registry image you actually publish.