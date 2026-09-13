# Graph RAG "Grill" Harness — Revised Plan (reuse-first, no duplicate endpoints)

## 1. Coverage map — what already exists (no new endpoints for these)

| Need | Existing endpoint / MCP tool |
|---|---|
| Upsert node with stable id, chunk + ONNX embed | `POST /api/store` (`id`, `text`, `type`, `data`) / MCP `store_entry` |
| Structural edges (GOVERNED_BY, BREAKS_INTO, …) | `POST /admin/graph/edges` (manual directed edge + relation) / MCP `create_manual_edge` |
| Dense/hybrid vector search | `POST /api/search` / MCP `search_entries` |
| Graph traversal up to N hops | `GET /api/graph/neighborhood/{id}?depth=` (returns full nodes + edges) / MCP `graph_neighborhood` |
| Amend ADR / apply edit / record exemption | `PATCH /admin/items/{id}` / MCP `update_item` / `append_to_entry` |
| List nodes by type/namespace | `GET /admin/items?source_id=&type_name=` / MCP `list_items` |
| Token counting for context slices | `POST /admin/tokens/count` |
| Post grill verdicts (done externally by harness) | `POST /api/store` with new `harness_audit` type + `POST /admin/graph/edges` with `AUDITED` relation |

**Dropped from the previous plan:** `/v1/harness/nodes`, `/v1/harness/edges`, `/v1/graph/retrieve`, `/v1/grill/audit`, `/v1/agent/context-slice` — all are compositions of the above. The grill audit LLM step moves to the harness; we store its results.

## 2. Genuine gaps — the only backend additions

1. **Harness node types (typed entries)** — register 7 JSON schemas (`assets/schemas/` + schemars Rust types in a new `src/api/harness.rs` module) so `/api/store` validates them:
   - `harness_doc` (doc_type ADR|SPEC|INVARIANT, title, version, status), `harness_plan` (state PROPOSED|GRILLED|APPROVED), `harness_sprint` (plan_id, cadence, state, goal), `harness_todo` (sprint_id, action_spec, state, sequence_order), `harness_agent` (role, capabilities[]), `harness_stream` (stream_name, aggregate_type, schema_definition)
   - **`harness_audit`** (the new node type you asked for): `{target_id, target_type, passed, score, violations: [{doc_id, title, violation_reason, severity}], unanchored_assumptions[], auditor, audited_at}`. The harness posts audit results as audit nodes and links them to the target with a directed `AUDITED` edge — latest verdict per node = newest AUDITED edge. Relation conventions (7 structural + AUDITED) documented and grouped in code; no schema change to `graph_edges`.
2. **Multi-type search filter** — extend `SearchRequest` (`src/api/store_search.rs`) and MCP `search_entries` with `type_names: Option<Vec<String>>` alongside the existing single `type_name` (backward compatible). This + `graph_neighborhood` = the full "retrieve" flow with no new endpoint.
3. **One new composite read: `GET /api/harness/tree`** — assembles Plan→Sprint→Todo hierarchy + attached docs/streams/agents + latest `harness_audit` verdict summary per node (red/yellow/green badge state) in one Postgres query (new SQL in `src/db/postgres.rs` behind a default-errored `VectorStore` method → clean 503 on SQLite). Also exposed as MCP tool `harness_tree`. Not duplicative: nothing today assembles the hierarchy with verdict badges; composing it client-side would ship all node content and N+1 calls.

## 3. Grill Cockpit UI (`/grill`, Next.js — the priority deliverable)

- `frontend/app/grill/page.tsx` + `frontend/components/grill/`, reusing react-resizable-panels, reagraph force-graph (already a dep), shadcn Badge/Tabs/ScrollArea, SWR, and the existing pure-black/cyan JetBrains-Mono dark theme.
- **Left pane** `plan-tree.tsx`: hierarchy from `GET /api/harness/tree`; red (blocking violation) / yellow (missing anchor or untracked mutation) / green (verified) badges from verdict summaries; selection via `?node=<id>` URL param.
- **Center pane** `audit-panel.tsx`: compiler-error-style cards built from the focused node's `harness_audit` violations (assumption vs. invariant quote, doc id/title link, severity); actions: Apply Suggested Edit (`PATCH /admin/items/{id}`), Amend ADR (deep-link to entries edit), Record Manual Exemption (writes exemption into node data). Tab toggle to `graph-tab.tsx`: reagraph 2-hop neighborhood of the focused node (reuses `embedded-graph.tsx` + `useGraphNeighborhood`).
- **Right pane** `context-pin.tsx`: composes the context slice **client-side** from one `/api/graph/neighborhood/{todo_id}?depth=2` call (todo spec + GOVERNED_BY docs + MUTATES_STREAM schemas + DELEGATES_TO agent), renders the exact Markdown slice with token count via `/admin/tokens/count`; agent role + capability tags from the agent node.
- **Bottom bar** `timeline-bar.tsx`: chronological audit nodes (the `harness_audit` stream) with per-node filter; clicking one loads that verdict into the center pane (time-travel over audit results).
- Wire-up: typed fns + SWR hooks in `lib/api/client.ts`/`hooks.ts`; `/grill` added to `frontend/proxy.ts` matcher + `isProtectedAppRoute`; nav link in `app-header.tsx`.

## 4. Tests

- Unit (existing `axum-test` + `MockStore` pattern): schema validation for all 7 harness types (valid/invalid payloads through `/api/store`), multi-type search filter, tree endpoint hierarchy + badge computation, audit-edge "latest verdict" resolution.
- Integration: add dev-deps `testcontainers` + `testcontainers-modules` (postgres); integration tests running the embedded migrations against a container: harness node upsert + typed search on Postgres, tree assembly SQL, 2-hop traversal over harness edges. `make test-integration` target; unit tests stay docker-free.
- Docs: `docs/harness.md` (node types, relation conventions, how the harness posts audits) + `http/harness.http` request collection.

**Out of scope (harness's job):** event ingestion (DocCommitted/PlanProposed/SprintScheduled/TaskDelegated consumers), the audit LLM verification itself, event-sourced time travel.