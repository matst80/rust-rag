# Harness Graph (Grill)

The agent-harness graph models plans, sprints, todos, governing documents,
sub-agents, and event streams — plus grill audit verdicts — on top of the
regular RAG store. **There is no separate harness API surface**: nodes and
edges reuse the existing document/edge endpoints and MCP tools, so the ONNX
embedding pipeline, hybrid search, and graph traversal all work on harness
nodes with zero extra machinery.

## Node types (typed entries)

Nodes are documents stored via `POST /api/store` (or MCP `store_entry`) with
`type` + schema-validated `data` and a **stable `id`** so re-projection is an
upsert. Schemas live in `assets/schemas/harness_*.json` and are seeded at
startup. The `text` field holds the human content (it is what gets embedded
and chunked); `data` holds the structured payload.

| type | data fields |
|---|---|
| `harness_doc` | `doc_type` (ADR\|SPEC\|INVARIANT), `title`, `version`, `status` (DRAFT\|ACTIVE\|SUPERSEDED\|DEPRECATED) |
| `harness_plan` | `title`, `state` (PROPOSED\|GRILLED\|APPROVED) |
| `harness_sprint` | `plan_id`, `cadence`, `goal`, `state` (PLANNED\|ACTIVE\|COMPLETED) |
| `harness_todo` | `sprint_id`, `title`, `action_spec` (object), `state`, `sequence_order` |
| `harness_agent` | `role`, `capabilities` (string list) |
| `harness_stream` | `stream_name`, `aggregate_type`, `schema_definition` (JSON Schema object) |
| `harness_audit` | `target_id`, `target_type`, `passed`, `score`, `violations[]`, `unanchored_assumptions[]`, `suggested_edits[]`, `auditor`, `audited_at` |

### POC-session domain

Promoted memories from POC/research sessions get one node per item instead of
undifferentiated text chunks. A single repo node aggregates every session run
against it.

| type | data fields |
|---|---|
| `harness_repo` | `name`, `url?`, `default_branch?` |
| `harness_poc` | `repo`, `summary`, `rollout_recommendation?`, `timestamp` (epoch ms) |
| `harness_decision` | `title`, `supersedes?`, `status?` (proposed\|accepted\|rejected) |
| `harness_risk` | `title`, `severity` (low\|medium\|high\|critical), `mitigation?` |
| `harness_compliance` | `title`, `framework` (GDPR, SOC2, …), `requirement?` |
| `harness_resource` | `title`, `kind`, `provisioned?` |
| `harness_scaling` | `title`, `bottleneck?`, `measured_metric?` |
| `harness_validation` | `title`, `status` (needed\|done), `how?`, `result?` |
| `harness_rollout` | `title`, `phases[]` ({name, description?, gate?}), `prerequisites[]` |

## Edge relations (manual directed edges)

Structural relationships are manual directed graph edges (`directed: true`),
created via `POST /admin/graph/edges` (or MCP `create_manual_edge`):

```
(:harness_plan)-[:GOVERNED_BY]->(:harness_doc)
(:harness_plan)-[:BREAKS_INTO]->(:harness_sprint)
(:harness_sprint)-[:CONTAINS_TODO]->(:harness_todo)
(:harness_todo)-[:ENFORCES_DOC]->(:harness_doc)
(:harness_todo)-[:DELEGATES_TO]->(:harness_agent)
(:harness_todo)-[:MUTATES_STREAM]->(:harness_stream)
(:harness_doc)-[:CONFLICTS_WITH]->(:harness_doc)
(:harness_audit)-[:AUDITED]->(:harness_plan | harness_sprint | harness_todo | harness_doc)

(:harness_repo)-[:HAD_POC]->(:harness_poc)
(:harness_poc)-[:RAISED]->(:harness_decision | harness_risk | harness_compliance | harness_scaling | harness_validation | harness_rollout)
(:harness_risk)-[:ADDRESSED_BY]->(:harness_validation)
(:harness_rollout)-[:REQUIRES]->(:harness_resource)
(:harness_decision)-[:SUPERSEDES]->(:harness_decision)
```

## Posting grill audits

The harness runs the adversarial verification (LLM comparison of plan/todo
assumptions against retrieved invariant/doc text) and posts the **result**:

1. `POST /api/store` with `type: "harness_audit"` and the verdict payload.
2. `POST /admin/graph/edges` with `from_item_id = <audit id>`,
   `to_item_id = <audited node id>`, `relation: "AUDITED"`,
   `directed: true`.

The newest `AUDITED` edge (by audit `updated_at`) is a node's current
verdict. Verdict shape:

```json
{
  "passed": false,
  "score": 0.4,
  "violations": [
    {"doc_id": "adr-7", "title": "ADR 7", "violation_reason": "…", "severity": "BLOCKING"}
  ],
  "unanchored_assumptions": ["cache is per-process"]
}
```

## Retrieval (search + traversal)

- **Dense/hybrid search scoped to node types**: `POST /api/search` accepts
  `type_names: ["harness_plan", "harness_todo", …]` (multi-type filter,
  merged per type before the top-K cut) in addition to the single `type`.
- **Graph expansion**: `GET /api/graph/neighborhood/{id}?depth=2&limit=100`
  returns the full nodes + edges around a node.
- Both are exposed as MCP tools (`search_entries`, `graph_neighborhood`).

## Cockpit tree endpoint

`GET /api/harness/tree[?source_id=]` assembles all harness nodes + the edges
between them in one call and resolves, per node:

- the **latest audit verdict** (verdict + audit id + score),
- a **badge**: `red` (blocking violation), `yellow` (unanchored plan/sprint/todo),
  `green` (passed audit), `null` (no badge — anchored-but-unaudited nodes,
  docs without audits, agents, streams).

Also available as MCP tool `harness_tree`. Requires a Postgres-backed store
(SQLite deployments get a 503 for this endpoint; the rest works on SQLite).

## Grill Cockpit UI

`/grill` — dark, monospace operator UI:

- **Left**: Plan → Sprint → Todo tree plus a POC-sessions section
  (Repo → PocSession → raised decisions/risks/validations/rollout, with
  supersedes chains nested under the superseding decision) with red/yellow/green badges.
- **Center**: compiler-error style violation cards per node (assumption vs.
  invariant, quote + doc link) with remediation actions ([Apply Suggested
  Edit], [Amend ADR], [Record Manual Exemption]) and a tab with the 2-hop
  reagraph neighborhood.
- **Right**: sub-agent context pinning — agent role/capabilities, allowed
  event streams, and the exact Markdown context slice with token count.
- **Bottom**: chronological audit timeline; clicking an audit loads that
  verdict.

## Tests

- Unit tests live in `src/api/harness.rs` and `src/api/mod.rs` (docker-free).
- Postgres integration tests: `tests/harness_postgres.rs` (testcontainers +
  pgvector image, `#[ignore]`d) — run with `make test-integration`.
- `src/validation.rs` compiles every bundled schema file at test time so a
  broken harness schema cannot slip in silently.
