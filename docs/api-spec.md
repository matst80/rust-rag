# API Spec

The HTTP API ships a machine-readable **OpenAPI 3.1** document for
integration codegen and explorability:

- **Spec**: `GET /api/openapi.json` (alias `/openapi.json`) — unauthenticated;
  it describes the API shape and grants nothing.
- **Swagger UI**: `GET /api/docs` — interactive viewer loading the spec above.

## How it stays correct

Request/response schemas are generated at runtime from the handlers' own
`schemars::JsonSchema` derives (`src/api/openapi.rs`), so the spec cannot
drift from the wire format. The route table mirrors `src/api/router.rs`; a
unit test (`api::openapi::tests`) asserts the route set is complete and every
`$ref` resolves to a registered component.

Conventions:

- Every documented path also exists without the `/api` prefix (legacy form),
  except where `x-aliases` lists other aliases.
- Authentication per operation: `x-api-key` header, `Authorization: Bearer`
  (API keys / MCP tokens), or the `rag_session` cookie — any one of them.
  `/healthz` and the spec endpoints are public.
- WebSockets (`/api/acp/ws`, `/api/whisper/ws`) and the MCP transport
  (`/mcp`) are described descriptively; OpenAPI does not model either. The
  MCP tool schemas are self-describing via the MCP protocol itself.

Regenerating a client, e.g. with openapi-typescript:

```bash
curl -s http://localhost:4001/api/openapi.json | \
  npx openapi-typescript -o ./src/lib/api-schema.d.ts
```
