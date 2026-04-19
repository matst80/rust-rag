# rust-rag

`rust-rag` is a local retrieval backend with an Axum HTTP API, SQLite/sqlite-vec storage, and an ONNX embedding pipeline.

## Documentation

- [Start Guide](docs/setup-guide.md) - Product overview and search workflow.
- [MCP Setup](docs/mcp-setup.md) - Agent integration and bridge configuration.


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