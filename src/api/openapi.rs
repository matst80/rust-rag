//! OpenAPI 3.1 document for the HTTP API, served (unauthenticated) at
//! `/api/openapi.json` with a Swagger UI at `/api/docs`.
//!
//! Request/response schemas come from the handlers' own `JsonSchema` derives
//! via `schemars::schema_for!`, so the spec cannot drift from the wire
//! format. The route table mirrors `router.rs`; legacy un-prefixed aliases
//! (e.g. `/store`, `/search`) are recorded per operation in `x-aliases`.

use serde_json::{Value, json};

type Map = serde_json::Map<String, Value>;

use super::acp::{
    AcpInstancesResponse, HeartbeatAcpInstanceRequest, HeartbeatAcpResponse,
    RegisterAcpInstanceRequest, SelectAcpInstanceRequest,
};
use super::attachments::{
    AttachUrlRequest, AttachmentSummary, AttachmentsResponse, EntriesPathsResponse,
    EntriesTreeResponse,
};
use super::cms::CmsTreeResponse;
use super::code::{CodeDeleteResponse, CodeSearchRequest};
use super::graph::{
    CreateManualEdgeRequest, GraphEdgePayload, GraphEdgesResponse,
    GraphNeighborhoodResponse, GraphRebuildResponse, GraphStatusResponse,
    UpdateGraphEdgeRequest,
};
use super::harness::HarnessTreeResponse;
use super::health::HealthResponse;
use super::ingest_url::IngestUrlRequest;
use super::items::{
    AdminItemPayload, AdminItemsResponse, CategoriesResponse, UpdateItemRequest,
};
use super::messages::{
    ChannelsResponse, ClearChannelResponse, MessagePayload, MessagesResponse, SendMessageRequest,
    UpdateMessageRequest,
};
use super::push::{
    DeleteResponse as PushDeleteResponse, ListSubscriptionsResponse, NotifyRequest,
    SubscribeRequest, VapidPublicKeyResponse,
};
use super::query::AssistedQueryRequest;
use super::schemas::{
    DeleteSchemaResponse, SchemaListResponse, SchemaPayload, UpsertSchemaRequest,
};
use super::store_search::{
    CountTokensRequest, CountTokensResponse, DeleteResponse as ItemDeleteResponse,
    LlmRechunkRequest, RechunkRequest, SmartStoreRequest, SmartStoreResponse,
};
use crate::api::{
    AnalyzeEntryParams, SearchRequest, SearchResponse, StoreAnalysis, StoreRequest, StoreResponse,
};
use crate::db::{
    CodeFileDetail, CodeFileMeta, CodeRepoSummary, CodeSearchHit, CodeUploadStats,
    DuplicateEdgeGroup,
};
use crate::notify::SendResult;
use crate::ontology::OntologyRunReport;
use crate::projection::MapPoint;

macro_rules! sref {
    ($components:expr, $name:literal, $T:ty) => {{
        let schema = serde_json::to_value(schemars::schema_for!($T))
            .expect("schema serializes");
        $components.insert($name.to_owned(), schema);
        json!({ "$ref": concat!("#/components/schemas/", $name) })
    }};
}

struct Op {
    id: &'static str,
    method: &'static str,
    summary: &'static str,
    tag: &'static str,
    /// Query parameters. Path parameters are derived automatically from `{…}`
    /// segments in the path.
    params: Vec<Value>,
    request: Option<Value>,
    request_content: &'static str,
    response: Option<Value>,
    response_code: u16,
    response_desc: &'static str,
    public: bool,
    aliases: &'static [&'static str],
    description: Option<&'static str>,
}

fn op(id: &'static str, method: &'static str, summary: &'static str, tag: &'static str) -> Op {
    Op {
        id,
        method,
        summary,
        tag,
        params: Vec::new(),
        request: None,
        request_content: "application/json",
        response: None,
        response_code: 200,
        response_desc: "OK",
        public: false,
        aliases: &[],
        description: None,
    }
}

fn q(name: &str, desc: &str) -> Value {
    json!({
        "name": name,
        "in": "query",
        "required": false,
        "description": desc,
        "schema": { "type": "string" }
    })
}

fn qnum(name: &str, desc: &str) -> Value {
    json!({
        "name": name,
        "in": "query",
        "required": false,
        "description": desc,
        "schema": { "type": "integer" }
    })
}

fn path_param(name: &str, desc: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "description": desc,
        "schema": { "type": "string" }
    })
}

fn add(paths: &mut Map, path: &'static str, op: Op, components: &mut Map) {
    let mut success = json!({ "description": op.response_desc });
    if let Some(schema) = op.response {
        success["content"] = json!({ "application/json": { "schema": schema } });
    }
    let mut operation = json!({
        "operationId": op.id,
        "summary": op.summary,
        "tags": [op.tag],
        "parameters": op.params,
        "responses": {
            op.response_code.to_string(): success
        }
    });
    if let Some(description) = op.description {
        operation["description"] = json!(description);
    }
    if !op.aliases.is_empty() {
        operation["x-aliases"] = json!(op.aliases);
    }
    if op.public {
        operation["security"] = json!([]);
    } else {
        operation["security"] = json!([
            { "ApiKeyAuth": [] },
            { "BearerAuth": [] },
            { "SessionCookieAuth": [] }
        ]);
    }
    if let Some(request) = op.request {
        operation["requestBody"] = json!({
            "required": true,
            "content": { op.request_content: { "schema": request } }
        });
    }
    let entry = paths.entry(path.to_owned()).or_insert_with(|| json!({}));
    entry[op.method] = operation;
    // touch components so the borrow checker sees the registration happen
    // before the paths map is returned
    let _ = &components;
}

pub fn openapi_document() -> Value {
    let mut components: Map = Map::new();
    let mut paths: Map = Map::new();

    // -- Entries: store & search -----------------------------------------
    add(
        &mut paths,
        "/api/store",
        Op {
            request: Some(sref!(components, "StoreRequest", StoreRequest)),
            response: Some(sref!(components, "StoreResponse", StoreResponse)),
            response_code: 201,
            response_desc: "Created (or upserted).",
            aliases: &["/store"],
            ..op(
                "store_entry",
                "post",
                "Store an entry (upsert by id)",
                "Entries",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/store/smart",
        Op {
            request: Some(sref!(components, "SmartStoreRequest", SmartStoreRequest)),
            response: Some(sref!(components, "SmartStoreResponse", SmartStoreResponse)),
            response_code: 200,
            response_desc: "OK",
            ..op(
                "smart_store",
                "post",
                "Store an entry with LLM-assisted analysis",
                "Entries",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/store/analyze",
        Op {
            request: Some(sref!(components, "AnalyzeEntryParams", AnalyzeEntryParams)),
            response: Some(sref!(components, "StoreAnalysis", StoreAnalysis)),
            response_code: 200,
            response_desc: "OK",
            ..op(
                "analyze_entry",
                "post",
                "Dry-run LLM analysis of a candidate entry",
                "Entries",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/search",
        Op {
            request: Some(sref!(components, "SearchRequest", SearchRequest)),
            response: Some(sref!(components, "SearchResponse", SearchResponse)),
            response_code: 200,
            response_desc: "OK",
            aliases: &["/search"],
            ..op("search", "post", "Dense/hybrid semantic search", "Entries")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/openai/v1/chat/completions",
        Op {
            request: Some(json!({
                "type": "object",
                "description": "OpenAI-compatible chat completion request (`model`, `messages`, `stream`, `tools`, …). Tool results drive the server-side RAG tool surface.",
                "properties": {
                    "model": { "type": "string" },
                    "messages": { "type": "array", "items": { "type": "object" } },
                    "stream": { "type": "boolean" }
                }
            })),
            response: Some(json!({
                "oneOf": [
                    { "type": "object", "description": "Chat completion object when `stream` is false" },
                    { "type": "string", "description": "Server-sent events stream when `stream` is true" }
                ]
            })),
            response_desc: "Chat completion (JSON) or SSE stream",
            ..op(
                "openai_chat_completions",
                "post",
                "OpenAI-compatible chat completions proxy",
                "Chat",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/query/assisted",
        Op {
            params: vec![],
            request: Some(sref!(
                components,
                "AssistedQueryRequest",
                AssistedQueryRequest
            )),
            response: Some(json!({
                "type": "string",
                "description": "Server-sent events: `queries`, `result`, `merged` events."
            })),
            response_desc: "SSE stream",
            ..op(
                "assisted_query",
                "post",
                "LLM-assisted multi-query search (SSE)",
                "Search",
            )
        },
        &mut components,
    );

    // -- Graph -------------------------------------------------------------
    add(
        &mut paths,
        "/api/graph/status",
        Op {
            response: Some(sref!(
                components,
                "GraphStatusResponse",
                GraphStatusResponse
            )),
            aliases: &["/graph/status"],
            ..op(
                "graph_status",
                "get",
                "Graph build status and configuration",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/graph/edges",
        Op {
            params: vec![
                q("item_id", "Only edges touching this item id."),
                q("edge_type", "`similarity` or `manual`."),
                q("status", "Filter by edge metadata status."),
            ],
            response: Some(sref!(components, "GraphEdgesResponse", GraphEdgesResponse)),
            aliases: &["/graph/edges"],
            ..op("list_graph_edges", "get", "List graph edges", "Graph")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/graph/neighborhood/{id}",
        Op {
            params: vec![
                path_param("id", "Center item id."),
                qnum("depth", "Traversal depth in hops (default 1, max 5)."),
                qnum("limit", "Maximum nodes returned (default 100)."),
                q("edge_type", "`similarity`, `manual`, or omit for both."),
            ],
            response: Some(sref!(
                components,
                "GraphNeighborhoodResponse",
                GraphNeighborhoodResponse
            )),
            aliases: &["/graph/neighborhood/{id}"],
            ..op(
                "graph_neighborhood",
                "get",
                "Depth-limited traversal around a node",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/harness/tree",
        Op {
            params: vec![q(
                "source_id",
                "Restrict the tree to one namespace. Omit to assemble across all.",
            )],
            response: Some(sref!(
                components,
                "HarnessTreeResponse",
                HarnessTreeResponse
            )),
            aliases: &["/harness/tree"],
            description: Some(
                "Assembles the agent-harness graph (plan/sprint/todo/docs/POC sessions) with the latest AUDITED verdict and status badge per node. Requires a Postgres-backed store; SQLite returns 503.",
            ),
            ..op(
                "harness_tree",
                "get",
                "Harness graph tree with verdicts + badges",
                "Harness",
            )
        },
        &mut components,
    );

    // -- Admin: items ------------------------------------------------------
    add(
        &mut paths,
        "/admin/categories",
        Op {
            response: Some(sref!(components, "CategoriesResponse", CategoriesResponse)),
            ..op(
                "list_categories",
                "get",
                "List namespaces (source_id) with entry counts",
                "Items",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/items",
        Op {
            params: vec![
                q("source_id", "Restrict to one namespace."),
                q("type", "Restrict to one typed-entry schema."),
                qnum("limit", "Page size."),
                qnum("offset", "Page offset."),
                q("sort_order", "`asc` or `desc` by created_at."),
                q("path_prefix", "Restrict to a wiki path prefix."),
                qnum("min_created_at", "Inclusive lower bound (epoch ms)."),
                qnum("max_created_at", "Inclusive upper bound (epoch ms)."),
            ],
            response: Some(sref!(components, "AdminItemsResponse", AdminItemsResponse)),
            ..op("list_items", "get", "List entries (paged)", "Items")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/tokens/count",
        Op {
            request: Some(sref!(components, "CountTokensRequest", CountTokensRequest)),
            response: Some(sref!(
                components,
                "CountTokensResponse",
                CountTokensResponse
            )),
            ..op(
                "count_tokens",
                "post",
                "Count tokens with the local tokenizer",
                "Items",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/items/{id}",
        Op {
            params: vec![path_param("id", "Entry id.")],
            response: Some(sref!(components, "AdminItemPayload", AdminItemPayload)),
            ..op("get_item", "get", "Fetch one entry", "Items")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/items/{id}",
        Op {
            params: vec![path_param("id", "Entry id.")],
            request: Some(sref!(components, "UpdateItemRequest", UpdateItemRequest)),
            response: Some(sref!(components, "AdminItemPayload", AdminItemPayload)),
            ..op("update_item", "put", "Update an entry (re-embeds)", "Items")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/items/{id}",
        Op {
            params: vec![path_param("id", "Entry id.")],
            response: Some(sref!(components, "ItemDeleteResponse", ItemDeleteResponse)),
            ..op("delete_item", "delete", "Delete an entry", "Items")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/items/{id}/reanalyze",
        Op {
            params: vec![path_param("id", "Entry id.")],
            response: Some(sref!(components, "AdminItemPayload", AdminItemPayload)),
            ..op(
                "reanalyze_item",
                "post",
                "Re-run the LLM analysis pass",
                "Items",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/items/{id}/rechunk",
        Op {
            params: vec![path_param("id", "Entry id.")],
            request: Some(sref!(components, "RechunkRequest", RechunkRequest)),
            response: Some(sref!(components, "StoreResponse", StoreResponse)),
            ..op(
                "rechunk_item",
                "post",
                "Re-chunk and re-embed an entry",
                "Items",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/items/{id}/llm-rechunk",
        Op {
            params: vec![path_param("id", "Entry id.")],
            request: Some(sref!(components, "LlmRechunkRequest", LlmRechunkRequest)),
            response: Some(sref!(components, "StoreResponse", StoreResponse)),
            ..op(
                "llm_rechunk_item",
                "post",
                "LLM-guided re-chunking of an entry",
                "Items",
            )
        },
        &mut components,
    );

    // -- Admin: graph ------------------------------------------------------
    add(
        &mut paths,
        "/admin/graph/rebuild",
        Op {
            response: Some(sref!(
                components,
                "GraphRebuildResponse",
                GraphRebuildResponse
            )),
            ..op(
                "rebuild_graph",
                "post",
                "Rebuild the similarity graph",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/graph/duplicates",
        Op {
            response: Some(json!({
                "type": "array",
                "items": sref!(components, "DuplicateEdgeGroup", DuplicateEdgeGroup)
            })),
            ..op(
                "list_duplicate_edges",
                "get",
                "List duplicate edge groups",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/graph/edges",
        Op {
            request: Some(sref!(
                components,
                "CreateManualEdgeRequest",
                CreateManualEdgeRequest
            )),
            response: Some(sref!(components, "GraphEdgePayload", GraphEdgePayload)),
            response_code: 201,
            response_desc: "Created (deduplicated).",
            ..op(
                "create_manual_edge",
                "post",
                "Create a manual (structural) edge",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/graph/edges/{id}",
        Op {
            params: vec![path_param("id", "Edge id.")],
            request: Some(sref!(
                components,
                "UpdateGraphEdgeRequest",
                UpdateGraphEdgeRequest
            )),
            response: Some(sref!(components, "GraphEdgePayload", GraphEdgePayload)),
            ..op(
                "update_graph_edge",
                "patch",
                "Update an edge (relation, weight, status)",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/graph/edges/{id}",
        Op {
            params: vec![path_param("id", "Edge id.")],
            response: Some(sref!(components, "GraphEdgePayload", GraphEdgePayload)),
            ..op("delete_graph_edge", "delete", "Delete an edge", "Graph")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/ontology/run",
        Op {
            response: Some(sref!(components, "OntologyRunReport", OntologyRunReport)),
            ..op(
                "ontology_run_batch",
                "post",
                "One-shot LLM ontology edge extraction run",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/ontology/run/{id}",
        Op {
            params: vec![path_param("id", "Item id to run the ontology worker on.")],
            response: Some(sref!(components, "OntologyRunReport", OntologyRunReport)),
            ..op(
                "ontology_run_for_item",
                "post",
                "Run the ontology worker for one item",
                "Graph",
            )
        },
        &mut components,
    );

    // -- CMS / map ----------------------------------------------------------
    add(
        &mut paths,
        "/api/cms/tree/{id}",
        Op {
            params: vec![path_param("id", "Root item id.")],
            response: Some(sref!(components, "CmsTreeResponse", CmsTreeResponse)),
            ..op(
                "get_cms_tree",
                "get",
                "Structural (CMS) tree under an item",
                "Graph",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/map",
        Op {
            response: Some(json!({
                "type": "array",
                "items": sref!(components, "MapPoint", MapPoint)
            })),
            ..op("get_map", "get", "Embedding-space map points", "Graph")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/admin/map/rebuild",
        Op {
            response: Some(json!({ "type": "object", "description": "Rebuild summary." })),
            ..op(
                "rebuild_map",
                "post",
                "Rebuild the embedding map projection",
                "Graph",
            )
        },
        &mut components,
    );

    // -- Ingest --------------------------------------------------------------
    add(
        &mut paths,
        "/api/ingest/image",
        Op {
            request: Some(multipart_fields(json!({
                "file": { "type": "string", "format": "binary" },
                "source_id": { "type": "string", "default": "images" },
                "metadata": { "type": "string", "description": "JSON-encoded metadata object" }
            }))),
            request_content: "multipart/form-data",
            response: Some(sref!(components, "StoreResponse", StoreResponse)),
            response_code: 201,
            response_desc: "Created.",
            ..op(
                "ingest_image",
                "post",
                "Ingest an image through the vision LLM",
                "Ingest",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/ingest/url",
        Op {
            request: Some(sref!(components, "IngestUrlRequest", IngestUrlRequest)),
            response: Some(sref!(components, "StoreResponse", StoreResponse)),
            response_code: 201,
            response_desc: "Created.",
            ..op(
                "ingest_url",
                "post",
                "Fetch a URL and ingest its content",
                "Ingest",
            )
        },
        &mut components,
    );

    // -- Code search -------------------------------------------------------
    add(
        &mut paths,
        "/api/code/upload",
        Op {
            request: Some(multipart_fields(json!({
                "repo": { "type": "string", "description": "Repo name; re-uploading replaces the snapshot" },
                "root_path": { "type": "string", "description": "Optional local repo path, for display" },
                "file": { "type": "string", "format": "binary", "description": ".codesearch/codesearch.db snapshot (sqlite + sqlite-vec)" }
            }))),
            request_content: "multipart/form-data",
            response: Some(sref!(components, "CodeUploadStats", CodeUploadStats)),
            response_code: 201,
            response_desc: "Snapshot merged.",
            ..op("upload_codesearch_db", "post", "Upload a codesearch.db snapshot", "Code")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/code/search",
        Op {
            request: Some(sref!(components, "CodeSearchRequest", CodeSearchRequest)),
            response: Some(json!({
                "type": "array",
                "items": sref!(components, "CodeSearchHit", CodeSearchHit)
            })),
            ..op(
                "search_code",
                "post",
                "Semantic code symbol search (all-MiniLM-L6-v2)",
                "Code",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/code/repos",
        Op {
            response: Some(json!({
                "type": "array",
                "items": sref!(components, "CodeRepoSummary", CodeRepoSummary)
            })),
            ..op("list_code_repos", "get", "List indexed code repos", "Code")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/code/repos/{name}",
        Op {
            response: Some(sref!(components, "CodeDeleteResponse", CodeDeleteResponse)),
            response_desc: "Deleted.",
            ..op("delete_code_repo", "delete", "Delete a code repo snapshot", "Code")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/code/repos/{name}/files",
        Op {
            response: Some(json!({
                "type": "array",
                "items": sref!(components, "CodeFileMeta", CodeFileMeta)
            })),
            ..op("list_code_files", "get", "List files in an indexed repo", "Code")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/code/repos/{name}/files/{path}",
        Op {
            response: Some(sref!(components, "CodeFileDetail", CodeFileDetail)),
            ..op(
                "get_code_file",
                "get",
                "File detail with symbol outline",
                "Code",
            )
        },
        &mut components,
    );

    // -- Attachments / entries tree ------------------------------------------
    add(
        &mut paths,
        "/api/attachments",
        Op {
            request: Some(multipart_fields(json!({
                "file": { "type": "string", "format": "binary" },
                "item_id": { "type": "string", "description": "Entry to attach the file to" }
            }))),
            request_content: "multipart/form-data",
            response: Some(sref!(components, "AttachmentSummary", AttachmentSummary)),
            response_code: 201,
            response_desc: "Created.",
            ..op(
                "upload_attachment",
                "post",
                "Upload a file attachment",
                "Attachments",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/attachments/from-url",
        Op {
            request: Some(sref!(components, "AttachUrlRequest", AttachUrlRequest)),
            response: Some(sref!(components, "AttachmentSummary", AttachmentSummary)),
            response_code: 201,
            response_desc: "Created.",
            ..op(
                "attach_from_url",
                "post",
                "Fetch a URL and attach it to an entry",
                "Attachments",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/attachments/{id}",
        Op {
            params: vec![path_param("id", "Attachment id.")],
            response: Some(
                json!({ "type": "object", "description": "Deletion acknowledgement with the removed on-disk filename." }),
            ),
            ..op(
                "delete_attachment",
                "delete",
                "Delete a file attachment",
                "Attachments",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/items/{id}/attachments",
        Op {
            params: vec![path_param("id", "Entry id.")],
            response: Some(sref!(
                components,
                "AttachmentsResponse",
                AttachmentsResponse
            )),
            ..op(
                "list_item_attachments",
                "get",
                "List attachments bound to an entry",
                "Attachments",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/entries/tree",
        Op {
            params: vec![q("source_id", "Namespace to build the tree for.")],
            response: Some(sref!(
                components,
                "EntriesTreeResponse",
                EntriesTreeResponse
            )),
            ..op("entries_tree", "get", "Wiki-style path tree", "Attachments")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/entries/paths",
        Op {
            params: vec![q("source_id", "Optional namespace filter.")],
            response: Some(sref!(
                components,
                "EntriesPathsResponse",
                EntriesPathsResponse
            )),
            ..op(
                "entries_paths",
                "get",
                "Every distinct (source_id, path) pair",
                "Attachments",
            )
        },
        &mut components,
    );

    // -- Schemas --------------------------------------------------------------
    add(
        &mut paths,
        "/api/schemas",
        Op {
            response: Some(sref!(components, "SchemaListResponse", SchemaListResponse)),
            ..op(
                "list_schemas",
                "get",
                "List registered typed-entry schemas",
                "Schemas",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/schemas",
        Op {
            request: Some(sref!(
                components,
                "UpsertSchemaRequest",
                UpsertSchemaRequest
            )),
            response: Some(sref!(components, "SchemaPayload", SchemaPayload)),
            response_code: 200,
            response_desc: "OK",
            ..op(
                "create_schema",
                "post",
                "Register a new typed-entry schema",
                "Schemas",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/schemas/{type_name}",
        Op {
            params: vec![path_param("type_name", "Schema name.")],
            response: Some(sref!(components, "SchemaPayload", SchemaPayload)),
            ..op(
                "get_schema",
                "get",
                "Fetch one typed-entry schema",
                "Schemas",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/schemas/{type_name}",
        Op {
            params: vec![path_param("type_name", "Schema name.")],
            request: Some(sref!(
                components,
                "UpsertSchemaRequest",
                UpsertSchemaRequest
            )),
            response: Some(sref!(components, "SchemaPayload", SchemaPayload)),
            ..op(
                "upsert_schema",
                "put",
                "Replace a typed-entry schema",
                "Schemas",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/schemas/{type_name}",
        Op {
            params: vec![
                path_param("type_name", "Schema name."),
                q("force", "`true` also clears `type` on entries using it."),
            ],
            response: Some(sref!(
                components,
                "DeleteSchemaResponse",
                DeleteSchemaResponse
            )),
            ..op(
                "delete_schema",
                "delete",
                "Delete a typed-entry schema",
                "Schemas",
            )
        },
        &mut components,
    );

    // -- Messages --------------------------------------------------------------
    add(
        &mut paths,
        "/api/messages",
        Op {
            request: Some(sref!(components, "SendMessageRequest", SendMessageRequest)),
            response: Some(sref!(components, "MessagePayload", MessagePayload)),
            response_code: 201,
            response_desc: "Created.",
            ..op(
                "send_message",
                "post",
                "Send a chat message to a channel",
                "Messages",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/messages",
        Op {
            params: vec![
                q("channel", "Restrict to one channel."),
                q("sender", "Restrict to one sender."),
                q("kind", "Filter by message kind (e.g. `text`)."),
                qnum("since", "Inclusive lower bound on created_at (epoch ms)."),
                qnum("until", "Inclusive upper bound on created_at (epoch ms)."),
                qnum("limit", "Page size."),
                qnum("offset", "Page offset."),
                q("sort_order", "`asc` or `desc`."),
                q(
                    "user",
                    "Register the polling caller as active in `channel`.",
                ),
                q("user_kind", "`human` or `agent`."),
                qnum("wait", "Long-poll wait in seconds (max 30)."),
            ],
            response: Some(sref!(components, "MessagesResponse", MessagesResponse)),
            ..op(
                "list_messages",
                "get",
                "List messages (supports long-polling)",
                "Messages",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/messages/channels",
        Op {
            response: Some(sref!(components, "ChannelsResponse", ChannelsResponse)),
            ..op(
                "list_message_channels",
                "get",
                "List message channels with presence",
                "Messages",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/messages/channels/{channel}",
        Op {
            params: vec![path_param("channel", "Channel name.")],
            response: Some(sref!(
                components,
                "ClearChannelResponse",
                ClearChannelResponse
            )),
            ..op(
                "clear_message_channel",
                "delete",
                "Delete every message in a channel",
                "Messages",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/messages/{id}",
        Op {
            params: vec![path_param("id", "Message id.")],
            request: Some(sref!(
                components,
                "UpdateMessageRequest",
                UpdateMessageRequest
            )),
            response: Some(sref!(components, "MessagePayload", MessagePayload)),
            ..op("update_message", "patch", "Edit a message", "Messages")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/messages/{id}",
        Op {
            params: vec![path_param("id", "Message id.")],
            response: Some(sref!(components, "ItemDeleteResponse", ItemDeleteResponse)),
            ..op("delete_message", "delete", "Delete a message", "Messages")
        },
        &mut components,
    );

    // -- ACP (agent control plane) ------------------------------------------------
    add(
        &mut paths,
        "/api/acp/instances",
        Op {
            response: Some(sref!(
                components,
                "AcpInstancesResponse",
                AcpInstancesResponse
            )),
            ..op(
                "list_acp_instances",
                "get",
                "List discovered/registered agent instances",
                "Agents",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/acp/select",
        Op {
            request: Some(sref!(
                components,
                "SelectAcpInstanceRequest",
                SelectAcpInstanceRequest
            )),
            response: Some(json!({
                "type": "object",
                "description": "The selected `AcpInstance`."
            })),
            ..op(
                "select_acp_instance",
                "post",
                "Select the active agent instance",
                "Agents",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/acp/register",
        Op {
            request: Some(sref!(
                components,
                "RegisterAcpInstanceRequest",
                RegisterAcpInstanceRequest
            )),
            response: Some(json!({
                "type": "object",
                "description": "The stored `AcpInstance`."
            })),
            ..op(
                "register_acp_instance",
                "post",
                "Register an agent daemon (mDNS fallback)",
                "Agents",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/acp/heartbeat",
        Op {
            request: Some(sref!(
                components,
                "HeartbeatAcpInstanceRequest",
                HeartbeatAcpInstanceRequest
            )),
            response: Some(sref!(
                components,
                "HeartbeatAcpResponse",
                HeartbeatAcpResponse
            )),
            ..op(
                "heartbeat_acp_instance",
                "post",
                "Refresh an agent instance registration",
                "Agents",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/acp/register/{name}",
        Op {
            params: vec![path_param("name", "Instance name.")],
            response: Some(json!({ "type": "object", "description": "Deletion acknowledgement." })),
            ..op(
                "unregister_acp_instance",
                "delete",
                "Unregister an agent instance",
                "Agents",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/acp/ws",
        Op {
            response: None,
            response_desc: "WebSocket upgrade (protocol `Telegram-ACP WS v1.3.0`).",
            description: Some(
                "Bidirectional agent WebSocket: sessions, terminal streams, permission requests. Cookie-authenticated on upgrade.",
            ),
            ..op("acp_ws", "get", "Agent control-plane WebSocket", "Agents")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/whisper/ws",
        Op {
            response: None,
            response_desc: "WebSocket upgrade (audio streaming proxy).",
            description: Some(
                "Proxy to the configured Whisper transcription service (`RAG_WHISPER_WS_URL`).",
            ),
            ..op(
                "whisper_ws",
                "get",
                "Whisper transcription WebSocket proxy",
                "Agents",
            )
        },
        &mut components,
    );

    // -- Push / notifications -------------------------------------------------------
    add(
        &mut paths,
        "/api/push/vapid-public-key",
        Op {
            response: Some(sref!(
                components,
                "VapidPublicKeyResponse",
                VapidPublicKeyResponse
            )),
            ..op(
                "vapid_public_key",
                "get",
                "Web-push VAPID public key",
                "Push",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/push/subscribe",
        Op {
            request: Some(sref!(components, "SubscribeRequest", SubscribeRequest)),
            response: Some(
                json!({ "type": "object", "description": "Subscription acknowledgement." }),
            ),
            ..op(
                "push_subscribe",
                "post",
                "Register a web-push subscription",
                "Push",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/push/subscriptions",
        Op {
            response: Some(sref!(
                components,
                "ListSubscriptionsResponse",
                ListSubscriptionsResponse
            )),
            ..op(
                "list_push_subscriptions",
                "get",
                "List web-push subscriptions",
                "Push",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/push/subscriptions/{id}",
        Op {
            params: vec![path_param("id", "Subscription id.")],
            response: Some(sref!(components, "PushDeleteResponse", PushDeleteResponse)),
            ..op(
                "delete_push_subscription",
                "delete",
                "Remove a web-push subscription",
                "Push",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/notify",
        Op {
            request: Some(sref!(components, "NotifyRequest", NotifyRequest)),
            response: Some(sref!(components, "SendResult", SendResult)),
            ..op("notify", "post", "Send a web-push notification", "Push")
        },
        &mut components,
    );

    // -- Integrations: Google ---------------------------------------------------
    add(
        &mut paths,
        "/api/integrations/google/status",
        Op {
            response: Some(
                json!({ "type": "object", "description": "Connection status + granted scopes." }),
            ),
            ..op(
                "google_status",
                "get",
                "Google OAuth connection status",
                "Integrations",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/integrations/google/start",
        Op {
            response: None,
            response_desc: "Redirect to Google's consent screen.",
            ..op(
                "google_start",
                "get",
                "Start the Google OAuth flow",
                "Integrations",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/integrations/google/callback",
        Op {
            response: None,
            response_desc: "Redirect after consent.",
            ..op(
                "google_callback",
                "get",
                "Google OAuth callback",
                "Integrations",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/integrations/google/disconnect",
        Op {
            response: Some(
                json!({ "type": "object", "description": "Disconnection acknowledgement." }),
            ),
            ..op(
                "google_disconnect",
                "post",
                "Disconnect the Google integration",
                "Integrations",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/integrations/google/drive/search",
        Op {
            params: vec![
                q("q", "Drive search query."),
                q("mime_type", "Optional MIME type filter."),
                qnum("page_size", "Maximum results."),
            ],
            response: Some(json!({ "type": "object", "description": "Drive search results." })),
            ..op(
                "google_drive_search",
                "get",
                "Search Google Drive",
                "Integrations",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/api/integrations/google/drive/fetch/{id}",
        Op {
            params: vec![path_param("id", "Drive file id.")],
            response: Some(
                json!({ "type": "object", "description": "Fetched Drive document content." }),
            ),
            ..op(
                "google_drive_fetch",
                "get",
                "Fetch a Google Drive document",
                "Integrations",
            )
        },
        &mut components,
    );

    // -- Misc -------------------------------------------------------------------
    add(
        &mut paths,
        "/api/dream",
        Op {
            response: None,
            response_desc: "202 Accepted (worker runs in the background).",
            ..op(
                "dream",
                "post",
                "Trigger one dreaming (memory consolidation) round",
                "Entries",
            )
        },
        &mut components,
    );
    add(
        &mut paths,
        "/healthz",
        Op {
            response: Some(sref!(components, "HealthResponse", HealthResponse)),
            public: true,
            ..op("health", "get", "Liveness/readiness probe", "Health")
        },
        &mut components,
    );
    add(
        &mut paths,
        "/mcp",
        Op {
            response: None,
            response_desc: "MCP streamable-HTTP responses.",
            description: Some(
                "Model Context Protocol endpoint (tool surface: store_entry, search_entries, graph_neighborhood, create_manual_edge, harness_tree, …). Discovers via `/.well-known/oauth-protected-resource`. See the MCP docs for the tool schemas; this REST spec does not duplicate them.",
            ),
            ..op(
                "mcp_endpoint",
                "post",
                "MCP streamable-HTTP transport",
                "MCP",
            )
        },
        &mut components,
    );

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "rust-rag API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Graph RAG backend: entries, hybrid search, harness graph, messages, agent control plane. Auth: `x-api-key` header or `Authorization: Bearer` (API keys / MCP tokens) or the `rag_session` cookie minted by the frontend. Every `/api/...` path below also exists without the prefix unless listed under `x-aliases` — the un-prefixed form is legacy.",
            "contact": { "url": "https://github.com/matst80/rust-rag" }
        },
        "servers": [ { "url": "/" } ],
        "tags": [
            { "name": "Entries" },
            { "name": "Search" },
            { "name": "Graph" },
            { "name": "Harness" },
            { "name": "Items" },
            { "name": "Schemas" },
            { "name": "Messages" },
            { "name": "Agents" },
            { "name": "Attachments" },
            { "name": "Ingest" },
            { "name": "Push" },
            { "name": "Integrations" },
            { "name": "Chat" },
            { "name": "MCP" },
            { "name": "Health" }
        ],
        "paths": Value::Object(paths),
        "components": {
            "securitySchemes": {
                "ApiKeyAuth": { "type": "apiKey", "in": "header", "name": "x-api-key" },
                "BearerAuth": { "type": "http", "scheme": "bearer" },
                "SessionCookieAuth": { "type": "apiKey", "in": "cookie", "name": "rag_session" }
            },
            "schemas": Value::Object(components)
        }
    })
}

fn multipart_fields(fields: Value) -> Value {
    json!({
        "type": "object",
        "properties": fields,
        "required": ["file"]
    })
}

/// `GET /api/openapi.json` — public, no auth: the spec describes the API
/// shape, it grants nothing.
pub async fn openapi_endpoint() -> Json<Value> {
    Json(openapi_document())
}

/// `GET /api/docs` — public Swagger UI loading the spec above.
pub async fn swagger_ui_endpoint() -> Html<&'static str> {
    Html(swagger_ui_html())
}

use axum::{Json, response::Html};

/// Minimal Swagger UI page loading assets from a CDN. Self-contained; no
/// build step needed.
pub fn swagger_ui_html() -> &'static str {
    r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <title>rust-rag API — Swagger UI</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
  <script>
    window.addEventListener("load", () =>
      window.ui = SwaggerUIBundle({ url: "/api/openapi.json", dom_id: "#swagger-ui" })
    );
  </script>
</body>
</html>
"##
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_is_wellformed_and_refs_resolve() {
        let doc = openapi_document();
        let paths = doc["paths"].as_object().expect("paths object");
        assert!(
            paths.len() >= 45,
            "expected the full route table, got {}",
            paths.len()
        );

        let schemas = doc["components"]["schemas"]
            .as_object()
            .expect("components.schemas object");
        assert!(
            schemas.len() >= 30,
            "expected the full schema set, got {}",
            schemas.len()
        );

        // Every operation must carry security (or be explicitly public).
        let mut operations = 0;
        for (path, item) in paths {
            assert!(path.starts_with('/'), "path {path} must start with /");
            for (method, op) in item.as_object().unwrap() {
                operations += 1;
                assert!(
                    ["get", "post", "put", "patch", "delete"].contains(&method.as_str()),
                    "{path}: unexpected method {method}"
                );
                let op = op.as_object().unwrap();
                assert!(
                    op.contains_key("security"),
                    "{method} {path}: missing security"
                );
                assert!(
                    op["responses"].as_object().is_some_and(|r| !r.is_empty()),
                    "{method} {path}: missing responses"
                );
            }
        }
        assert!(
            operations >= 55,
            "expected the full operation set, got {operations}"
        );

        // Every $ref used anywhere must point at a registered component.
        let raw = serde_json::to_string(&doc).unwrap();
        for chunk in raw.split("#/components/schemas/").skip(1) {
            let name: String = chunk
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert!(
                schemas.contains_key(&name),
                "dangling $ref: #/components/schemas/{name}"
            );
        }
    }

    #[test]
    fn core_integration_endpoints_are_documented() {
        let doc = openapi_document();
        let paths = doc["paths"].as_object().unwrap();
        for expected in [
            "/api/store",
            "/api/search",
            "/api/graph/neighborhood/{id}",
            "/api/harness/tree",
            "/admin/graph/edges",
            "/admin/items",
            "/api/schemas",
            "/api/messages",
            "/api/openai/v1/chat/completions",
            "/healthz",
            "/mcp",
        ] {
            assert!(paths.contains_key(expected), "missing {expected}");
        }
    }
}
