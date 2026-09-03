# rust-rag Simple API Guide

A quick integration guide with `curl` examples for storing and searching entries using the `rust-rag` HTTP API.

---

## Authentication

When authentication is enabled on the server, pass your API key or token using either of the following headers:

- **Header**: `-H "x-api-key: <YOUR_API_KEY_OR_TOKEN>"`
- **Authorization Bearer**: `-H "Authorization: Bearer <YOUR_API_KEY_OR_TOKEN>"`

*(If running locally without authentication enabled, the auth header can be omitted.)*

---

## 1. Store an Entry (`POST /api/store`)

### Basic Example

Stores an entry in a given namespace (`source_id`).

```bash
curl -X POST "http://localhost:4001/api/store" \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-api-key-here" \
  -d '{
    "text": "Postgres connection pool max_connections is configured to 50 in production.",
    "source_id": "knowledge",
    "metadata": {
      "author": "backend-team",
      "environment": "production"
    }
  }'
```

### Advanced Example

Supports an optional custom `id`, hierarchical wiki `path`, and text chunking (`chunk`).

```bash
curl -X POST "http://localhost:4001/api/store" \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-api-key-here" \
  -d '{
    "id": "runbook-db-pool",
    "text": "Detailed operational procedure for database connection pooling...",
    "source_id": "project:my-app:knowledge",
    "path": "engineering/runbooks/db",
    "metadata": {
      "tags": ["runbook", "database", "ops"],
      "priority": "high"
    },
    "chunk": {
      "max_chars": 1536,
      "overlap_chars": 200
    }
  }'
```

### Response (`201 Created`)

```json
{
  "id": "runbook-db-pool",
  "source_id": "project:my-app:knowledge",
  "created_at": 1725347400000
}
```

---

## 2. Search for Entries (`POST /api/search`)

### Basic Semantic Search

```bash
curl -X POST "http://localhost:4001/api/search" \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-api-key-here" \
  -d '{
    "query": "How many database connections in prod?",
    "top_k": 5
  }'
```

### Filtered & Hybrid Search with Reranking

```bash
curl -X POST "http://localhost:4001/api/search" \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-api-key-here" \
  -d '{
    "query": "database connection pool limits",
    "source_id": "knowledge",
    "top_k": 3,
    "hybrid": true,
    "rerank": true,
    "max_distance": 0.75
  }'
```

### Response (`200 OK`)

```json
{
  "results": [
    {
      "id": "runbook-db-pool",
      "text": "Postgres connection pool max_connections is configured to 50 in production.",
      "metadata": {
        "author": "backend-team",
        "environment": "production"
      },
      "source_id": "knowledge",
      "created_at": 1725347400000,
      "updated_at": 1725347400000,
      "distance": 0.18,
      "retrievers": ["dense", "sparse"],
      "path": "engineering/runbooks/db",
      "section_path": []
    }
  ],
  "related": []
}
```

---

## Request Fields Reference

### `POST /api/store`

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | **Yes** | Natural-language text content to embed and store. |
| `source_id` | string | **Yes** | Logical category/namespace (e.g., `knowledge`, `memory`, `project:xyz:notes`). |
| `metadata` | object | **Yes** | Arbitrary JSON object containing tags or metadata attributes. |
| `id` | string | No | Stable unique ID. If omitted, a UUIDv7 is automatically generated. |
| `path` | string | No | Slash-separated hierarchical wiki path (e.g., `engineering/runbooks/db`). |
| `chunk` | object | No | Optional `{ "max_chars": 1536, "overlap_chars": 200 }` chunking config for long text. |
| `type` | string | No | Schema type name (for typed entries validated against registered schemas). |
| `data` | object | No | Typed JSON payload (required if `type` is set). |

### `POST /api/search`

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `query` | string | **Yes** | — | Search string. Embedded and compared against entries. |
| `source_id` | string | No | `null` | Limit search to a specific namespace. Omit to search all. |
| `top_k` | number | No | `5` | Maximum number of results to return. |
| `hybrid` | boolean | No | `true` | Combine dense vector and sparse keyword matching. |
| `rerank` | boolean | No | `null` | Run cross-encoder reranker on candidate hits if loaded. |
| `max_distance` | number | No | `0.8` | Maximum distance cutoff threshold for hits. |
| `type` | string | No | `null` | Filter results by schema type name. |
