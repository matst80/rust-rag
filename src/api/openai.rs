use super::{
    AdminItemPayload, AdminItemsResponse, ApiError, AppState, CategoriesResponse,
    GraphNeighborhoodResponse, GraphStatusResponse, SearchResultPayload, current_timestamp_millis,
    map_graph_error, resolve_store_id, validate_graph_depth, validate_graph_limit,
    validate_metadata, validate_non_empty, validate_source_id,
    ingest_url::{fetch_with_cdp, fetch_with_reqwest},
};
use crate::db::{ItemRecord, ListItemsRequest, SortOrder};
use anyhow::{Context, anyhow};
use async_stream::stream;
use axum::{
    Json,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::Response,
};
use futures_util::StreamExt;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{borrow::Cow, collections::HashMap, convert::Infallible};

const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const SEARCH_ENTRIES_TOOL: &str = "search_entries";
const STORE_ENTRY_TOOL: &str = "store_entry";
const LIST_CATEGORIES_TOOL: &str = "list_categories";
const LIST_ITEMS_TOOL: &str = "list_items";
const GET_ITEM_TOOL: &str = "get_item";
const UPDATE_ITEM_TOOL: &str = "update_item";
const DELETE_ITEM_TOOL: &str = "delete_item";
const GRAPH_STATUS_TOOL: &str = "graph_status";
const GRAPH_NEIGHBORHOOD_TOOL: &str = "graph_neighborhood";
const REBUILD_GRAPH_TOOL: &str = "rebuild_graph";
const CREATE_GRAPH_EDGE_TOOL: &str = "create_graph_edge";
const DELETE_GRAPH_EDGE_TOOL: &str = "delete_graph_edge";
const INGEST_WEB_CONTENT_TOOL: &str = "ingest_web_content";
const READ_FILE_RANGE_TOOL: &str = "read_file_range";
const SPLIT_ENTRY_TOOL: &str = "split_entry_to_wiki";

#[derive(Debug, Deserialize)]
struct ReadFileRangeArguments {
    file_id: String,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ChatToolDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AssistantToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ChatFunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatFunctionDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: AssistantToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    delta: ChatCompletionDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ChatCompletionDelta {
    #[serde(rename = "role")]
    _role: Option<String>,
    content: Option<String>,
    #[serde(alias = "reasoning")]
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ChatCompletionToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionToolCallDelta {
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    function: Option<ChatCompletionToolFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionToolFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct SseEventBuffer {
    buffer: String,
}

#[derive(Debug, Default)]
struct ToolCallAccumulator {
    calls: HashMap<usize, PartialToolCall>,
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: String,
    kind: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct SearchEntriesArguments {
    query: String,
    top_k: Option<usize>,
    source_id: Option<String>,
    hybrid: Option<bool>,
    max_distance: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct StoreEntryArguments {
    id: Option<String>,
    text: String,
    metadata: Value,
    source_id: String,
    /// Optional wiki path (slash-separated) for the entry; surfaces the entry
    /// in the wiki tree at this location.
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListItemsArguments {
    source_id: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    sort_order: Option<String>,
    min_created_at: Option<i64>,
    max_created_at: Option<i64>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct GetItemArguments {
    id: String,
}

#[derive(Debug, Deserialize)]
struct UpdateItemArguments {
    id: String,
    text: String,
    metadata: Value,
    source_id: String,
    /// Wiki path. Omit to keep the existing path unchanged. Pass an empty
    /// string to clear it.
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SplitEntryArguments {
    /// Source entry to split. Read its full text + path before deciding parts.
    parent_id: String,
    /// Optional override; defaults to the parent's source_id.
    source_id: Option<String>,
    /// When true, leaves the parent in place (e.g. as an index). When false
    /// (default), deletes the parent after the children are written.
    #[serde(default)]
    keep_parent: bool,
    /// Each part becomes its own entry. `path` may be absolute (starts with
    /// `/`) or relative — relative slugs are joined under the parent's path.
    parts: Vec<SplitPart>,
}

#[derive(Debug, Deserialize)]
struct SplitPart {
    /// Stable id for the new entry. If omitted, a UUIDv7 is generated.
    id: Option<String>,
    text: String,
    /// Optional path. If missing and parent has one, the slug is appended.
    path: Option<String>,
    /// Used to derive a path under the parent when `path` itself is omitted.
    slug: Option<String>,
    /// Per-part metadata; merges over a `{ split_from: parent_id }` default.
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct DeleteItemArguments {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GraphNeighborhoodArguments {
    id: String,
    depth: Option<usize>,
    limit: Option<usize>,
    edge_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateGraphEdgeArguments {
    from_item_id: String,
    to_item_id: String,
    relation: Option<String>,
    weight: Option<f32>,
    directed: Option<bool>,
    metadata: Value,
}

#[derive(Debug, Deserialize)]
struct DeleteGraphEdgeArguments {
    id: String,
}

#[derive(Debug, Deserialize)]
struct IngestWebContentArguments {
    url: String,
    source_id: String,
    metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
struct SearchToolResponse {
    generated_at: i64,
    results: Vec<SearchResultPayload>,
}

pub(super) async fn chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionsRequest>,
) -> std::result::Result<Response, ApiError> {
    if request.stream != Some(true) {
        return Err(ApiError::BadRequest(
            "only stream=true is supported on this endpoint".to_owned(),
        ));
    }

    if request.messages.is_empty() {
        return Err(ApiError::BadRequest(
            "messages must not be empty".to_owned(),
        ));
    }

    let openai_config = state.openai_chat.clone();
    if !openai_config.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "upstream OpenAI chat configuration is not set".to_owned(),
        ));
    }

    let model = request
        .model
        .clone()
        .or_else(|| openai_config.default_model.clone())
        .ok_or_else(|| {
            ApiError::BadRequest(
                "model is required when no RAG_OPENAI_MODEL default is configured".to_owned(),
            )
        })?;

    let mut messages: Vec<ChatMessage> = request
        .messages
        .iter()
        .filter(|message| message.role != "system")
        .cloned()
        .collect();
    messages.insert(
        0,
        ChatMessage {
            role: "system".to_owned(),
            content: Some(Value::String(openai_config.retrieval_system_prompt.clone())),
            reasoning_content: None,
            name: None,
            tool_call_id: None,
            tool_calls: None,
        },
    );

    let body_stream = Body::from_stream(stream! {
        let mut messages = messages;

        loop {
            let upstream_payload = match build_upstream_payload(&request, &messages, &model) {
                Ok(payload) => payload,
                Err(error) => {
                    yield Ok::<_, Infallible>(encode_error_event(&error.to_string()));
                    yield Ok::<_, Infallible>(encode_done_event());
                    break;
                }
            };

            let upstream_request = state
                .http_client
                .post(format!(
                    "{}{}",
                    openai_config
                        .base_url
                        .as_deref()
                        .expect("config already validated")
                        .trim_end_matches('/'),
                    CHAT_COMPLETIONS_PATH,
                ))
                .json(&upstream_payload);

            let upstream_request = if let Some(api_key) = openai_config.api_key.as_deref() {
                upstream_request.bearer_auth(api_key)
            } else {
                upstream_request
            };

            let upstream_response = match upstream_request.send().await
            {
                Ok(response) => response,
                Err(error) => {
                    yield Ok::<_, Infallible>(encode_error_event(&format!(
                        "failed to call upstream chat provider: {error}"
                    )));
                    yield Ok::<_, Infallible>(encode_done_event());
                    break;
                }
            };

            if !upstream_response.status().is_success() {
                let status = upstream_response.status();
                let message = upstream_response
                    .text()
                    .await
                    .unwrap_or_else(|_| status.to_string());
                yield Ok::<_, Infallible>(encode_error_event(&format!(
                    "upstream chat provider returned {status}: {message}"
                )));
                yield Ok::<_, Infallible>(encode_done_event());
                break;
            }

            let mut event_buffer = SseEventBuffer::default();
            let mut tool_calls = ToolCallAccumulator::default();
            let mut assistant_content = String::new();
            let mut assistant_reasoning = String::new();
            let mut tool_loop_requested = false;
            let mut upstream_failed = false;
            let mut upstream_stream = upstream_response.bytes_stream();

            while let Some(chunk_result) = upstream_stream.next().await {
                let chunk = match chunk_result {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Ok::<_, Infallible>(encode_error_event(&format!(
                            "failed while reading upstream stream: {error}"
                        )));
                        upstream_failed = true;
                        break;
                    }
                };

                for event in event_buffer.push(std::str::from_utf8(&chunk).unwrap_or_default()) {
                    if event == "[DONE]" {
                        break;
                    }

                    let parsed = match serde_json::from_str::<ChatCompletionChunk>(&event) {
                        Ok(parsed) => parsed,
                        Err(error) => {
                            yield Ok::<_, Infallible>(encode_error_event(&format!(
                                "failed to decode upstream event: {error}"
                            )));
                            upstream_failed = true;
                            break;
                        }
                    };

                    for choice in &parsed.choices {
                        if let Some(content) = &choice.delta.content {
                            assistant_content.push_str(content);
                        }

                        if let Some(reasoning) = &choice.delta.reasoning_content {
                            assistant_reasoning.push_str(reasoning);
                        }

                        if let Some(tool_call_deltas) = &choice.delta.tool_calls {
                            tool_calls.apply(tool_call_deltas);
                        }

                        if choice.finish_reason.as_deref() == Some("tool_calls") {
                            tool_loop_requested = true;
                        }
                    }

                    yield Ok::<_, Infallible>(encode_data_event(&event));
                }

                if upstream_failed {
                    break;
                }
            }

            if upstream_failed {
                yield Ok::<_, Infallible>(encode_done_event());
                break;
            }

            if !tool_loop_requested {
                yield Ok::<_, Infallible>(encode_done_event());
                break;
            }

            let finalized_tool_calls = match tool_calls.finish() {
                Ok(tool_calls) => tool_calls,
                Err(error) => {
                    yield Ok::<_, Infallible>(encode_error_event(&error.to_string()));
                    yield Ok::<_, Infallible>(encode_done_event());
                    break;
                }
            };

            messages.push(ChatMessage {
                role: "assistant".to_owned(),
                content: if assistant_content.is_empty() {
                    None
                } else {
                    Some(Value::String(assistant_content))
                },
                reasoning_content: if assistant_reasoning.is_empty() {
                    None
                } else {
                    Some(assistant_reasoning)
                },
                name: None,
                tool_call_id: None,
                tool_calls: Some(finalized_tool_calls.clone()),
            });

            for tool_call in finalized_tool_calls {
                let tool_output = execute_server_tool(&state, &tool_call).await;
                yield Ok::<_, Infallible>(encode_tool_result_event(&tool_call.id, &tool_call.function.name, &tool_output));

                messages.push(ChatMessage {
                    role: "tool".to_owned(),
                    content: Some(Value::String(tool_output)),
                    reasoning_content: None,
                    name: Some(tool_call.function.name.clone()),
                    tool_call_id: Some(tool_call.id.clone()),
                    tool_calls: None,
                });
            }
        }
    });

    let mut response = Response::new(body_stream);
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    Ok(response)
}

fn build_upstream_payload(
    request: &ChatCompletionsRequest,
    messages: &[ChatMessage],
    model: &str,
) -> std::result::Result<Value, ApiError> {
    let mut payload = serde_json::to_value(request)
        .map_err(|error| ApiError::Internal(anyhow!(error).context("failed to encode request")))?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| ApiError::Internal(anyhow!("encoded request was not an object")))?;

    object.insert("model".to_owned(), Value::String(model.to_owned()));
    object.insert(
        "messages".to_owned(),
        serde_json::to_value(messages).map_err(|error| {
            ApiError::Internal(anyhow!(error).context("failed to encode messages"))
        })?,
    );
    object.insert("stream".to_owned(), Value::Bool(true));
    object.insert(
        "tools".to_owned(),
        serde_json::to_value(server_tool_definitions()).map_err(|error| {
            ApiError::Internal(anyhow!(error).context("failed to encode tools"))
        })?,
    );

    match sanitize_tool_choice(request.tool_choice.clone()) {
        Some(tool_choice) => {
            object.insert("tool_choice".to_owned(), tool_choice);
        }
        None => {
            object.remove("tool_choice");
        }
    }

    Ok(payload)
}

fn sanitize_tool_choice(tool_choice: Option<Value>) -> Option<Value> {
    tool_choice
}

fn server_tool_definitions() -> Vec<ChatToolDefinition> {
    vec![
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: SEARCH_ENTRIES_TOOL.to_owned(),
                description: Some(
                    "Search stored RAG entries by semantic similarity and optional source_id filter."
                        .to_owned(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Natural-language search query." },
                        "top_k": { "type": "integer", "minimum": 1, "maximum": 25, "default": 5 },
                        "source_id": { "type": "string" },
                        "hybrid": { "type": "boolean", "default": true },
                        "max_distance": { "type": "number", "default": 0.8 }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: STORE_ENTRY_TOOL.to_owned(),
                description: Some(
                    "Store a new RAG entry. Long markdown is automatically chunked at the configured token budget; pass a `path` (slash-separated, e.g. `architecture/auth/oauth`) to surface the entry in the wiki tree."
                        .to_owned(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Optional stable identifier." },
                        "text": { "type": "string", "description": "The content to store." },
                        "metadata": { "type": "object", "description": "JSON metadata object." },
                        "source_id": { "type": "string", "description": "Namespace/category (e.g., 'notes', 'knowledge')." },
                        "path": { "type": "string", "description": "Wiki path (slash-separated). Omit for entries without a wiki location." }
                    },
                    "required": ["text", "metadata", "source_id"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: LIST_CATEGORIES_TOOL.to_owned(),
                description: Some("List all available source_id categories and their item counts.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: LIST_ITEMS_TOOL.to_owned(),
                description: Some("List items with optional filtering and pagination.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "source_id": { "type": "string" },
                        "limit": { "type": "integer", "default": 10 },
                        "offset": { "type": "integer", "default": 0 },
                        "sort_order": { "type": "string", "enum": ["Asc", "Desc"], "default": "Desc" },
                        "min_created_at": { "type": "integer", "description": "Filter by minimum creation timestamp (ms)." },
                        "max_created_at": { "type": "integer", "description": "Filter by maximum creation timestamp (ms)." },
                        "metadata": { "type": "object", "description": "Key-value pairs to match in entry metadata." }
                    },
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: GET_ITEM_TOOL.to_owned(),
                description: Some(
                    "Get a single item by its ID, returning the full text, metadata, source_id, and wiki path. Use this before splitting a long entry into wiki pages."
                        .to_owned(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: UPDATE_ITEM_TOOL.to_owned(),
                description: Some(
                    "Replace an existing item's text/metadata/source_id (and optionally its wiki path). Re-chunks and re-embeds the new content."
                        .to_owned(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "text": { "type": "string" },
                        "metadata": { "type": "object" },
                        "source_id": { "type": "string" },
                        "path": { "type": "string", "description": "Wiki path. Omit to keep the existing path; pass an empty string to clear it." }
                    },
                    "required": ["id", "text", "metadata", "source_id"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: DELETE_ITEM_TOOL.to_owned(),
                description: Some("Delete an item by its ID.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: GRAPH_STATUS_TOOL.to_owned(),
                description: Some("Get the status of the similarity graph.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: GRAPH_NEIGHBORHOOD_TOOL.to_owned(),
                description: Some("Get the graph neighborhood for a specific item.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "depth": { "type": "integer", "default": 1 },
                        "limit": { "type": "integer", "default": 50 },
                        "edge_type": { "type": "string", "enum": ["Similarity", "Manual"] }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: REBUILD_GRAPH_TOOL.to_owned(),
                description: Some("Trigger a rebuild of the similarity graph.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: CREATE_GRAPH_EDGE_TOOL.to_owned(),
                description: Some("Create a manual graph edge between two items.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "from_item_id": { "type": "string" },
                        "to_item_id": { "type": "string" },
                        "relation": { "type": "string" },
                        "weight": { "type": "number", "default": 1.0 },
                        "directed": { "type": "boolean", "default": false },
                        "metadata": { "type": "object" }
                    },
                    "required": ["from_item_id", "to_item_id", "metadata"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: DELETE_GRAPH_EDGE_TOOL.to_owned(),
                description: Some("Delete a manual graph edge by its ID.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: INGEST_WEB_CONTENT_TOOL.to_owned(),
                description: Some("Fetch a web page, clean it, and ingest its content as markdown into RAG.".to_owned()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The URL of the page to ingest." },
                        "source_id": { "type": "string", "description": "Namespace/category for this entry." },
                        "metadata": { "type": "object", "description": "Optional JSON metadata." }
                    },
                    "required": ["url", "source_id"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: SPLIT_ENTRY_TOOL.to_owned(),
                description: Some(
                    "Split a long entry into multiple wiki entries. Each part becomes its own entry with its own path; relative slugs are joined under the parent's path. Set keep_parent=true to leave the parent in place as an index page (default: delete it)."
                        .to_owned(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "parent_id": { "type": "string", "description": "ID of the entry to split. Fetch it with get_item first." },
                        "source_id": { "type": "string", "description": "Override source_id for the new parts. Defaults to the parent's source_id." },
                        "keep_parent": { "type": "boolean", "default": false, "description": "Keep the parent entry as an index page when true." },
                        "parts": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "text": { "type": "string" },
                                    "path": { "type": "string", "description": "Absolute path (with leading /) or a relative path. Optional if `slug` is given." },
                                    "slug": { "type": "string", "description": "Path segment to append under the parent's path when `path` is omitted." },
                                    "metadata": { "type": "object" }
                                },
                                "required": ["text"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["parent_id", "parts"],
                    "additionalProperties": false
                })),
            },
        },
        ChatToolDefinition {
            kind: "function".to_owned(),
            function: ChatFunctionDefinition {
                name: READ_FILE_RANGE_TOOL.to_owned(),
                description: Some(
                    "Read a specific line range from a large file stored on disk during research."
                        .to_owned(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "file_id": { "type": "string", "description": "The identifier of the file to read." },
                        "start_line": { "type": "integer", "minimum": 1, "description": "1-based start line number." },
                        "end_line": { "type": "integer", "minimum": 1, "description": "1-based end line number." }
                    },
                    "required": ["file_id", "start_line", "end_line"],
                    "additionalProperties": false
                })),
            },
        },
    ]
}

async fn execute_server_tool(state: &AppState, tool_call: &AssistantToolCall) -> String {
    match tool_call.function.name.as_str() {
        SEARCH_ENTRIES_TOOL => {
            match search_entries_tool(state, &tool_call.function.arguments).await {
                Ok(result) => result,
                Err(error) => json!({ "error": error.to_string() }).to_string(),
            }
        }
        STORE_ENTRY_TOOL => match store_entry_tool(state, &tool_call.function.arguments).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        LIST_CATEGORIES_TOOL => match list_categories_tool(state).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        LIST_ITEMS_TOOL => match list_items_tool(state, &tool_call.function.arguments).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        GET_ITEM_TOOL => match get_item_tool(state, &tool_call.function.arguments).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        UPDATE_ITEM_TOOL => match update_item_tool(state, &tool_call.function.arguments).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        DELETE_ITEM_TOOL => match delete_item_tool(state, &tool_call.function.arguments).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        GRAPH_STATUS_TOOL => match graph_status_tool(state).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        GRAPH_NEIGHBORHOOD_TOOL => {
            match graph_neighborhood_tool(state, &tool_call.function.arguments).await {
                Ok(result) => result,
                Err(error) => json!({ "error": error.to_string() }).to_string(),
            }
        }
        REBUILD_GRAPH_TOOL => match rebuild_graph_tool(state).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        CREATE_GRAPH_EDGE_TOOL => {
            match create_graph_edge_tool(state, &tool_call.function.arguments).await {
                Ok(result) => result,
                Err(error) => json!({ "error": error.to_string() }).to_string(),
            }
        }
        DELETE_GRAPH_EDGE_TOOL => {
            match delete_graph_edge_tool(state, &tool_call.function.arguments).await {
                Ok(result) => result,
                Err(error) => json!({ "error": error.to_string() }).to_string(),
            }
        }
        INGEST_WEB_CONTENT_TOOL => {
            match ingest_web_content_tool(state, &tool_call.function.arguments).await {
                Ok(result) => result,
                Err(error) => json!({ "error": error.to_string() }).to_string(),
            }
        }
        READ_FILE_RANGE_TOOL => {
            match read_file_range_tool(state, &tool_call.function.arguments).await {
                Ok(result) => result,
                Err(error) => json!({ "error": error.to_string() }).to_string(),
            }
        }
        SPLIT_ENTRY_TOOL => match split_entry_tool(state, &tool_call.function.arguments).await {
            Ok(result) => result,
            Err(error) => json!({ "error": error.to_string() }).to_string(),
        },
        _ => json!({
            "error": format!("unsupported server tool {}", tool_call.function.name)
        })
        .to_string(),
    }
}

async fn search_entries_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: SearchEntriesArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {SEARCH_ENTRIES_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    validate_non_empty("query", &arguments.query)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    if let Some(source_id) = arguments.source_id.as_deref() {
        validate_source_id(source_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    }

    let top_k = arguments.top_k.unwrap_or(5);
    if top_k == 0 {
        return Err(ApiError::BadRequest(
            "top_k must be greater than zero".to_owned(),
        ));
    }

    let max_distance = arguments.max_distance.unwrap_or(0.8);
    let hybrid = arguments.hybrid.unwrap_or(true);

    let embedder = state.embedder.get_ready()?;
    let store = state.store.clone();
    let query = arguments.query;
    let source_id = arguments.source_id;

    let results =
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<SearchResultPayload>> {
            let (embedding, sparse) = if hybrid {
                embedder.embed_both(&query)?
            } else {
                (embedder.embed(&query)?, Vec::new())
            };
            let hits = if hybrid {
                store.search_hybrid(
                    &query,
                    &embedding,
                    &sparse,
                    top_k,
                    source_id.as_deref(),
                )?
            } else {
                store.search(&embedding, top_k, source_id.as_deref())?
            };

            Ok(hits
                .into_iter()
                .filter(|hit| hit.distance <= max_distance)
                .map(SearchResultPayload::from)
                .collect())
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(serde_json::to_string(&SearchToolResponse {
        generated_at: current_timestamp_millis()
            .map_err(|error| ApiError::Internal(anyhow!(error.to_string())))?,
        results,
    })
    .map_err(|error| ApiError::Internal(anyhow!(error).context("failed to encode tool result")))?)
}

async fn store_entry_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: StoreEntryArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {STORE_ENTRY_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let request = crate::api::StoreRequest {
        id: arguments.id,
        text: arguments.text,
        metadata: arguments.metadata,
        source_id: arguments.source_id,
        chunk: None,
        path: arguments.path.filter(|p| !p.is_empty()),
        type_name: None,
        data: None,
    };

    let response = crate::api::store_entry_core(state, request, None).await?;
    Ok(serde_json::to_string(&response).map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn list_categories_tool(state: &AppState) -> std::result::Result<String, ApiError> {
    let store = state.store.clone();
    let categories = tokio::task::spawn_blocking(move || store.list_categories())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(serde_json::to_string(&CategoriesResponse {
        categories: categories.into_iter().map(Into::into).collect(),
    })
    .map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn list_items_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: ListItemsArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {LIST_ITEMS_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    if let Some(source_id) = arguments.source_id.as_deref() {
        validate_source_id(source_id)?;
    }

    let store = state.store.clone();
    let sort_order = match arguments.sort_order.as_deref() {
        Some("Asc") => SortOrder::Asc,
        _ => SortOrder::Desc,
    };

    let request = ListItemsRequest {
        source_id: arguments.source_id,
        limit: arguments.limit,
        offset: arguments.offset,
        sort_order,
        metadata_filter: arguments.metadata,
        min_created_at: arguments.min_created_at,
        max_created_at: arguments.max_created_at,
        path_prefix: None,
        type_name: None,
    };

    let (items, total_count) = tokio::task::spawn_blocking(move || store.list_items(request))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(serde_json::to_string(&AdminItemsResponse {
        items: items.into_iter().map(Into::into).collect(),
        total_count,
    })
    .map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn get_item_tool(state: &AppState, arguments: &str) -> std::result::Result<String, ApiError> {
    let arguments: GetItemArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {GET_ITEM_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let store = state.store.clone();
    let id = arguments.id;
    let item = tokio::task::spawn_blocking(move || store.get_item(&id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound("item not found".to_owned()))?;

    let payload: AdminItemPayload = item.into();
    Ok(serde_json::to_string(&payload).map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn update_item_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: UpdateItemArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {UPDATE_ITEM_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let store = state.store.clone();
    let lookup_id = arguments.id.clone();
    let existing = tokio::task::spawn_blocking(move || store.get_item(&lookup_id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound(format!("item {} not found", arguments.id)))?;

    // path semantics: omitted → keep existing; "" → clear; non-empty → set.
    let path = match arguments.path {
        None => existing.path.clone(),
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
    };

    let request = crate::api::StoreRequest {
        id: Some(arguments.id),
        text: arguments.text,
        metadata: arguments.metadata,
        source_id: arguments.source_id,
        chunk: None,
        path,
        type_name: None,
        data: None,
    };

    let response = crate::api::store_entry_core(state, request, None).await?;
    Ok(serde_json::to_string(&response).map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn split_entry_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: SplitEntryArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {SPLIT_ENTRY_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    if arguments.parts.is_empty() {
        return Err(ApiError::BadRequest("parts must not be empty".to_owned()));
    }

    let store = state.store.clone();
    let parent_id = arguments.parent_id.clone();
    let parent = tokio::task::spawn_blocking(move || store.get_item(&parent_id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
        .ok_or_else(|| {
            ApiError::NotFound(format!("parent {} not found", arguments.parent_id))
        })?;

    let parent_path = parent.path.clone();
    let target_source = arguments
        .source_id
        .clone()
        .unwrap_or_else(|| parent.source_id.clone());

    let mut written: Vec<crate::api::StoreResponse> = Vec::with_capacity(arguments.parts.len());
    for (idx, part) in arguments.parts.into_iter().enumerate() {
        validate_non_empty(&format!("parts[{idx}].text"), &part.text)?;

        let path = derive_split_path(&parent_path, part.path.as_deref(), part.slug.as_deref());
        if path.is_none() && parent_path.is_some() {
            return Err(ApiError::BadRequest(format!(
                "parts[{idx}] needs `path` or `slug` because the parent has a wiki path"
            )));
        }

        let metadata = match part.metadata {
            Some(Value::Object(mut m)) => {
                m.entry("split_from".to_owned())
                    .or_insert_with(|| Value::String(arguments.parent_id.clone()));
                Value::Object(m)
            }
            Some(other) => other,
            None => json!({ "split_from": arguments.parent_id }),
        };

        let req = crate::api::StoreRequest {
            id: part.id,
            text: part.text,
            metadata,
            source_id: target_source.clone(),
            chunk: None,
            path,
            type_name: None,
            data: None,
        };
        let resp = crate::api::store_entry_core(state, req, None).await?;
        written.push(resp);
    }

    let parent_deleted = if !arguments.keep_parent {
        let store = state.store.clone();
        let parent_id = arguments.parent_id.clone();
        tokio::task::spawn_blocking(move || store.delete_item(&parent_id))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?
    } else {
        false
    };

    Ok(json!({
        "parent_id": arguments.parent_id,
        "parent_deleted": parent_deleted,
        "kept_parent": arguments.keep_parent,
        "parts": written,
    })
    .to_string())
}

fn derive_split_path(
    parent_path: &Option<String>,
    explicit: Option<&str>,
    slug: Option<&str>,
) -> Option<String> {
    if let Some(p) = explicit {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix('/') {
            return Some(rest.trim_matches('/').to_owned());
        }
        return match parent_path {
            Some(parent) => Some(format!(
                "{}/{}",
                parent.trim_matches('/'),
                trimmed.trim_matches('/')
            )),
            None => Some(trimmed.trim_matches('/').to_owned()),
        };
    }
    if let Some(s) = slug {
        let s = s.trim().trim_matches('/');
        if s.is_empty() {
            return None;
        }
        return match parent_path {
            Some(parent) => Some(format!("{}/{}", parent.trim_matches('/'), s)),
            None => Some(s.to_owned()),
        };
    }
    None
}

async fn delete_item_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: DeleteItemArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {DELETE_ITEM_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let store = state.store.clone();
    let id = arguments.id;
    let id_for_task = id.clone();
    let deleted = tokio::task::spawn_blocking(move || store.delete_item(&id_for_task))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    if !deleted {
        return Err(ApiError::NotFound(format!("item {id} not found")));
    }

    Ok(json!({ "id": id, "deleted": deleted }).to_string())
}

async fn graph_status_tool(state: &AppState) -> std::result::Result<String, ApiError> {
    let store = state.store.clone();
    let status = tokio::task::spawn_blocking(move || store.graph_status())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    let payload: GraphStatusResponse = status.into();
    Ok(serde_json::to_string(&payload).map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn graph_neighborhood_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: GraphNeighborhoodArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {GRAPH_NEIGHBORHOOD_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let depth = arguments.depth.unwrap_or(1);
    let limit = arguments.limit.unwrap_or(50);
    validate_graph_depth(depth)?;
    validate_graph_limit(limit)?;

    let store = state.store.clone();
    let id = arguments.id;
    let edge_type = match arguments.edge_type.as_deref() {
        Some("Similarity") => Some(crate::db::GraphEdgeType::Similarity),
        Some("Manual") => Some(crate::db::GraphEdgeType::Manual),
        _ => None,
    };

    let neighborhood =
        tokio::task::spawn_blocking(move || store.graph_neighborhood(&id, depth, limit, edge_type))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(map_graph_error)?;

    let payload: GraphNeighborhoodResponse = neighborhood.into();
    Ok(serde_json::to_string(&payload).map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn rebuild_graph_tool(state: &AppState) -> std::result::Result<String, ApiError> {
    let store = state.store.clone();
    let rebuilt_edges = tokio::task::spawn_blocking(move || store.rebuild_similarity_graph())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(map_graph_error)?;

    Ok(json!({ "rebuilt_edges": rebuilt_edges }).to_string())
}

async fn create_graph_edge_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: CreateGraphEdgeArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {CREATE_GRAPH_EDGE_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    validate_non_empty("from_item_id", &arguments.from_item_id)?;
    validate_non_empty("to_item_id", &arguments.to_item_id)?;
    validate_metadata(&arguments.metadata)?;

    let store = state.store.clone();
    let input = crate::db::ManualEdgeInput {
        from_item_id: arguments.from_item_id,
        to_item_id: arguments.to_item_id,
        relation: arguments.relation.map(Cow::Owned),
        weight: arguments.weight.unwrap_or(1.0),
        directed: arguments.directed.unwrap_or(false),
        metadata: arguments.metadata,
    };

    let edge = tokio::task::spawn_blocking(move || store.add_manual_edge(input))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(map_graph_error)?;

    let payload: super::GraphEdgePayload = edge.into();
    Ok(serde_json::to_string(&payload).map_err(|error| ApiError::Internal(anyhow!(error)))?)
}

async fn delete_graph_edge_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: DeleteGraphEdgeArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {DELETE_GRAPH_EDGE_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let store = state.store.clone();
    let id = arguments.id;
    let id_for_task = id.clone();
    let deleted = tokio::task::spawn_blocking(move || store.delete_graph_edge(&id_for_task))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(map_graph_error)?;

    if !deleted {
        return Err(ApiError::NotFound(format!("graph edge {id} not found")));
    }

    Ok(json!({ "id": id, "deleted": deleted }).to_string())
}

async fn read_file_range_tool(
    _state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: ReadFileRangeArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {READ_FILE_RANGE_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let file_id = arguments.file_id;
    // Basic path traversal protection: only allow alphanumeric + dashes/underscores
    if !file_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::BadRequest("invalid file_id".to_owned()));
    }

    let path = format!("data/research/{}.md", file_id);
    let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ApiError::NotFound(format!("research file {} not found", file_id))
        } else {
            ApiError::Internal(anyhow!(e).context("failed to read research file"))
        }
    })?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let start = arguments.start_line.saturating_sub(1);
    let end = arguments.end_line.min(total_lines);

    if start >= total_lines || start >= end {
        return Ok(json!({
            "file_id": file_id,
            "total_lines": total_lines,
            "message": "requested range is out of bounds or empty",
            "content": ""
        })
        .to_string());
    }

    const MAX_LINES: usize = 500;
    const MAX_BYTES: usize = 40_000;

    let mut effective_end = end.min(start + MAX_LINES);
    let mut range_content = lines[start..effective_end].join("\n");
    let mut truncated_reason: Option<&str> = if effective_end < end {
        Some("line_cap")
    } else {
        None
    };

    if range_content.len() > MAX_BYTES {
        let mut byte_budget = MAX_BYTES;
        let mut new_end = start;
        for line in &lines[start..effective_end] {
            let needed = line.len() + 1;
            if needed > byte_budget {
                break;
            }
            byte_budget -= needed;
            new_end += 1;
        }
        if new_end <= start {
            new_end = start + 1;
        }
        effective_end = new_end;
        range_content = lines[start..effective_end].join("\n");
        truncated_reason = Some("byte_cap");
    }

    let mut response = json!({
        "file_id": file_id,
        "start_line": start + 1,
        "end_line": effective_end,
        "total_lines": total_lines,
        "content": range_content,
    });
    if let Some(reason) = truncated_reason {
        response["truncated"] = json!(true);
        response["truncation_reason"] = json!(reason);
        response["message"] = json!(format!(
            "Range truncated (cap: {} lines / {} bytes). Request a smaller range to continue reading from line {}.",
            MAX_LINES,
            MAX_BYTES,
            effective_end + 1
        ));
    }
    Ok(response.to_string())
}

async fn ingest_web_content_tool(
    state: &AppState,
    arguments: &str,
) -> std::result::Result<String, ApiError> {
    let arguments: IngestWebContentArguments = serde_json::from_str(arguments)
        .with_context(|| format!("invalid arguments for {INGEST_WEB_CONTENT_TOOL}"))
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    validate_source_id(&arguments.source_id)?;
    validate_non_empty("url", &arguments.url)?;

    let html_content = match fetch_with_reqwest(state, &arguments.url).await {
        Ok(html) => html,
        Err(e) => {
            if state.openai_chat.cdp_url.is_some() {
                tracing::warn!(url = %arguments.url, error = %e, "reqwest fetch failed in tool, falling back to CDP");
                fetch_with_cdp(state, &arguments.url).await?
            } else {
                return Err(e);
            }
        }
    };

    let cleaned_markdown = tokio::task::spawn_blocking(move || {
        if html_content.is_markdown {
            return html_content.content;
        }

        let document = Html::parse_document(&html_content.content);

        // Actually, html2md is quite good. Let's try to refine the HTML before passing it.
        // We can use scraper to get the main content area if possible (main, article, or body).
        let main_selector = Selector::parse("main, [role='main'], article, body").unwrap();
        let content_html = document
            .select(&main_selector)
            .next()
            .map(|el| el.html())
            .unwrap_or_else(|| document.html());

        let markdown = html2md::parse_html(&content_html);
        markdown
    })
    .await
    .map_err(ApiError::TaskJoin)?;

    let id = resolve_store_id(None);
    let (is_large, file_id) = if cleaned_markdown.len() > 20000 {
        let file_id = id.clone();
        let dir = std::path::Path::new("data/research");
        tokio::fs::create_dir_all(dir).await.map_err(|e| {
            ApiError::Internal(anyhow!(e).context("failed to create research directory"))
        })?;
        let path = dir.join(format!("{}.md", file_id));
        tokio::fs::write(&path, &cleaned_markdown)
            .await
            .map_err(|e| {
                ApiError::Internal(anyhow!(e).context("failed to save large research file"))
            })?;
        (true, Some(file_id))
    } else {
        (false, None)
    };

    // Store in RAG - if large, only store a preview/metadata
    let id_for_rag = id.clone();
    let embedder = state.embedder.get_ready()?;
    let store = state.store.clone();
    let created_at = current_timestamp_millis()?;

    let mut metadata = arguments.metadata.unwrap_or_else(|| json!({}));
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("source_url".to_owned(), json!(arguments.url));
        obj.insert("ingested_at".to_owned(), json!(created_at));
        if let Some(ref fid) = file_id {
            obj.insert("research_file_id".to_owned(), json!(fid));
        }
    }

    let text_for_rag = if is_large {
        format!(
            "Large content ingested from {}. Saved to research file: {}. \n\nPreview:\n{}",
            arguments.url,
            file_id.as_ref().unwrap(),
            &cleaned_markdown[..2000.min(cleaned_markdown.len())]
        )
    } else {
        cleaned_markdown.clone()
    };

    let item = ItemRecord {
        id: id_for_rag,
        text: text_for_rag,
        metadata,
        source_id: arguments.source_id.clone(),
        created_at,
        path: None,
        type_name: None,
        data: None,
    };

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let embedding = embedder.embed(&item.text)?;
        store.upsert_item(item, &embedding)?;
        Ok(())
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    if is_large {
        Ok(json!({
            "status": "saved_to_disk",
            "file_id": file_id.unwrap(),
            "url": arguments.url,
            "total_length": cleaned_markdown.len(),
            "message": "Content is too large for immediate context. Use server__read_file_range to examine specific lines. YOU MUST extract relevant chunks and store them using server__store_entry for better retrieval."
        }).to_string())
    } else {
        Ok(json!({
            "id": id,
            "source_id": arguments.source_id,
            "url": arguments.url,
            "content": cleaned_markdown,
            "markdown_length": cleaned_markdown.len()
        })
        .to_string())
    }
}

impl SseEventBuffer {
    fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(&chunk.replace("\r\n", "\n"));
        let mut events = Vec::new();

        while let Some(separator) = self.buffer.find("\n\n") {
            let raw_event = self.buffer[..separator].to_owned();
            self.buffer.drain(..separator + 2);

            let data_lines = raw_event
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>();

            if !data_lines.is_empty() {
                events.push(data_lines.join("\n"));
            }
        }

        events
    }
}

impl ToolCallAccumulator {
    fn apply(&mut self, deltas: &[ChatCompletionToolCallDelta]) {
        for delta in deltas {
            let entry = self.calls.entry(delta.index).or_default();
            if let Some(id) = &delta.id {
                entry.id = id.clone();
            }
            if let Some(kind) = &delta.kind {
                entry.kind = kind.clone();
            }
            if let Some(function) = &delta.function {
                if let Some(name) = &function.name {
                    entry.name.push_str(name);
                }
                if let Some(arguments) = &function.arguments {
                    entry.arguments.push_str(arguments);
                }
            }
        }
    }

    fn finish(self) -> Result<Vec<AssistantToolCall>, ApiError> {
        let mut ordered = self.calls.into_iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(index, _)| *index);

        ordered
            .into_iter()
            .map(|(_, partial)| {
                if partial.id.is_empty() || partial.name.is_empty() {
                    return Err(ApiError::Internal(anyhow!(
                        "upstream returned incomplete tool call"
                    )));
                }

                Ok(AssistantToolCall {
                    id: partial.id,
                    kind: if partial.kind.is_empty() {
                        "function".to_owned()
                    } else {
                        partial.kind
                    },
                    function: AssistantToolFunction {
                        name: partial.name,
                        arguments: partial.arguments,
                    },
                })
            })
            .collect()
    }
}


fn encode_data_event(data: &str) -> Bytes {
    Bytes::from(format!("data: {data}\n\n"))
}

fn encode_tool_result_event(id: &str, name: &str, content: &str) -> Bytes {
    encode_data_event(
        &json!({
            "object": "chat.completion.tool_result",
            "tool_call_id": id,
            "name": name,
            "content": content,
        })
        .to_string(),
    )
}

fn encode_done_event() -> Bytes {
    encode_data_event("[DONE]")
}

fn encode_error_event(message: &str) -> Bytes {
    encode_data_event(
        &json!({
            "error": {
                "message": message,
                "type": "server_error"
            }
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::EmbedderHandle,
        config::{AuthConfig, ChunkingConfig, MultimodalConfig, OpenAiChatConfig},
        db::{AuthStore, GraphConfig, ItemRecord, SqliteVectorStore, VectorStore},
        embedding::EmbeddingService,
    };
    use std::sync::Arc;

    struct MockEmbedder;

    impl EmbeddingService for MockEmbedder {
        fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3, 0.4])
        }

        fn count_tokens(&self, text: &str) -> anyhow::Result<usize> {
            Ok(text.split_whitespace().count())
        }
    }

    #[test]
    fn upstream_payload_uses_server_tools_only() {
        let request = ChatCompletionsRequest {
            model: None,
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: Some(Value::String("hello".to_owned())),
                reasoning_content: None,
                name: None,
                tool_call_id: None,
                tool_calls: None,
            }],
            stream: Some(true),
            tools: vec![ChatToolDefinition {
                kind: "function".to_owned(),
                function: ChatFunctionDefinition {
                    name: "client__open_modal".to_owned(),
                    description: None,
                    parameters: None,
                },
            }],
            tool_choice: Some(json!({
                "type": "function",
                "function": { "name": "client__open_modal" }
            })),
            temperature: None,
            max_completion_tokens: None,
            parallel_tool_calls: None,
            extra: HashMap::new(),
        };

        let payload = build_upstream_payload(&request, &request.messages, "gpt-test")
            .expect("payload should build");

        assert_eq!(payload["model"], Value::String("gpt-test".to_owned()));
        assert_eq!(payload["stream"], Value::Bool(true));
        assert_eq!(payload["tools"].as_array().map(Vec::len), Some(15));
        assert_eq!(payload.get("tool_choice").map(|v| v.clone()), Some(serde_json::json!({ "type": "function", "function": { "name": "client__open_modal" } })));
    }

    #[test]
    fn upstream_payload_omits_null_message_fields() {
        let request = ChatCompletionsRequest {
            model: None,
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: Some(Value::String("hi".to_owned())),
                reasoning_content: None,
                name: None,
                tool_call_id: None,
                tool_calls: None,
            }],
            stream: Some(true),
            tools: Vec::new(),
            tool_choice: None,
            temperature: None,
            max_completion_tokens: None,
            parallel_tool_calls: None,
            extra: HashMap::from([
                ("return_progress".to_owned(), Value::Bool(true)),
                (
                    "reasoning_format".to_owned(),
                    Value::String("auto".to_owned()),
                ),
            ]),
        };

        let payload = build_upstream_payload(&request, &request.messages, "current_model.gguf")
            .expect("payload should build");
        let message = payload["messages"][0]
            .as_object()
            .expect("message should be object");

        assert_eq!(message.get("role"), Some(&Value::String("user".to_owned())));
        assert_eq!(
            message.get("content"),
            Some(&Value::String("hi".to_owned()))
        );
        assert_eq!(message.get("name"), None);
        assert_eq!(message.get("tool_call_id"), None);
        assert_eq!(message.get("tool_calls"), None);
        assert_eq!(payload.get("temperature"), None);
        assert_eq!(payload.get("max_completion_tokens"), None);
        assert_eq!(payload.get("parallel_tool_calls"), None);
        assert_eq!(payload["return_progress"], Value::Bool(true));
        assert_eq!(
            payload["reasoning_format"],
            Value::String("auto".to_owned())
        );
    }

    #[test]
    fn sse_buffer_parses_multi_line_events() {
        let mut buffer = SseEventBuffer::default();
        let events = buffer.push("data: {\"a\":1}\n\ndata: [DONE]\n\n");

        assert_eq!(events, vec!["{\"a\":1}".to_owned(), "[DONE]".to_owned()]);
    }

    #[test]
    fn tool_accumulator_reassembles_incremental_arguments() {
        let mut accumulator = ToolCallAccumulator::default();
        accumulator.apply(&[
            ChatCompletionToolCallDelta {
                index: 0,
                id: Some("call_123".to_owned()),
                kind: Some("function".to_owned()),
                function: Some(ChatCompletionToolFunctionDelta {
                    name: Some(SEARCH_ENTRIES_TOOL.to_owned()),
                    arguments: Some("{\"query\":\"hel".to_owned()),
                }),
            },
            ChatCompletionToolCallDelta {
                index: 0,
                id: None,
                kind: None,
                function: Some(ChatCompletionToolFunctionDelta {
                    name: None,
                    arguments: Some("lo\"}".to_owned()),
                }),
            },
        ]);

        let calls = accumulator.finish().expect("tool calls should finalize");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.arguments, "{\"query\":\"hello\"}");
    }

    #[tokio::test]
    async fn search_tool_queries_store_and_serializes_results() {
        let store = Arc::new(
            SqliteVectorStore::connect_uri(
                "file:search-tool?mode=memory&cache=shared",
                4,
                GraphConfig::default(),
            )
            .expect("sqlite store should initialize"),
        );
        let item = ItemRecord {
            id: "doc-1".to_owned(),
            text: "hello world".to_owned(),
            metadata: json!({"kind": "note"}),
            source_id: "knowledge".to_owned(),
            created_at: 123,
            path: None,
            type_name: None,
            data: None,
        };
        store
            .upsert_item(item, &[0.1, 0.2, 0.3, 0.4])
            .expect("item should store");

        let state = AppState::new(
            Arc::new(EmbedderHandle::ready(Arc::new(MockEmbedder))),
            store.clone() as Arc<dyn VectorStore>,
            store as Arc<dyn AuthStore>,
            Arc::new(super::super::NoopUserMemory),
            Arc::new(super::super::NoopMessages),
            AuthConfig::default(),
            OpenAiChatConfig {
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
            MultimodalConfig::default(),
            "uploads".to_owned(),
            ChunkingConfig::default(),
        );

        let result = search_entries_tool(
            &state,
            r#"{"query":"hello","source_id":"knowledge","hybrid":false}"#,
        )
        .await
        .expect("tool execution should succeed");

        let parsed: Value = serde_json::from_str(&result).expect("tool output should be JSON");
        assert_eq!(parsed["results"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            parsed["results"][0]["id"],
            Value::String("doc-1".to_owned())
        );
    }
}
