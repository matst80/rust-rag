use super::{
    ApiError, AppState, SearchResultPayload, current_timestamp_millis, validate_non_empty,
    validate_source_id,
};
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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, convert::Infallible};

const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const SERVER_TOOL_PREFIX: &str = "server__";
const CLIENT_TOOL_PREFIX: &str = "client__";
const SEARCH_ENTRIES_TOOL: &str = "server__search_entries";

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

    let body_stream = Body::from_stream(stream! {
        let mut messages = request.messages.clone();

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

                    let mut contains_tool_delta = false;
                    for choice in &parsed.choices {
                        if let Some(content) = &choice.delta.content {
                            assistant_content.push_str(content);
                        }

                        if let Some(tool_call_deltas) = &choice.delta.tool_calls {
                            contains_tool_delta = true;
                            tool_calls.apply(tool_call_deltas);
                        }

                        if choice.finish_reason.as_deref() == Some("tool_calls") {
                            tool_loop_requested = true;
                        }
                    }

                    if !contains_tool_delta {
                        yield Ok::<_, Infallible>(encode_data_event(&event));
                    }
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
                name: None,
                tool_call_id: None,
                tool_calls: Some(finalized_tool_calls.clone()),
            });

            for tool_call in finalized_tool_calls {
                let tool_output = execute_server_tool(&state, &tool_call).await;
                messages.push(ChatMessage {
                    role: "tool".to_owned(),
                    content: Some(Value::String(tool_output)),
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
        serde_json::to_value(messages)
            .map_err(|error| ApiError::Internal(anyhow!(error).context("failed to encode messages")))?,
    );
    object.insert("stream".to_owned(), Value::Bool(true));
    object.insert(
        "tools".to_owned(),
        serde_json::to_value(server_tool_definitions())
            .map_err(|error| ApiError::Internal(anyhow!(error).context("failed to encode tools")))?,
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
    let value = tool_choice?;
    let Some(function_name) = value
        .get("function")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
    else {
        return Some(value);
    };

    if function_name.starts_with(SERVER_TOOL_PREFIX) {
        Some(value)
    } else {
        None
    }
}

fn server_tool_definitions() -> Vec<ChatToolDefinition> {
    vec![ChatToolDefinition {
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
    }]
}

async fn execute_server_tool(state: &AppState, tool_call: &AssistantToolCall) -> String {
    if tool_call.function.name.starts_with(CLIENT_TOOL_PREFIX) {
        return json!({
            "error": "client tools must not be executed by the backend"
        })
        .to_string();
    }

    match tool_call.function.name.as_str() {
        SEARCH_ENTRIES_TOOL => match search_entries_tool(state, &tool_call.function.arguments).await {
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

    let results = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<SearchResultPayload>> {
        let embedding = embedder.embed(&query)?;
        let hits = if hybrid {
            store.search_hybrid(&query, &embedding, top_k, source_id.as_deref())?
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
        generated_at: current_timestamp_millis().map_err(|error| ApiError::Internal(anyhow!(error.to_string())))?,
        results,
    })
    .map_err(|error| ApiError::Internal(anyhow!(error).context("failed to encode tool result")))?)
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

fn encode_done_event() -> Bytes {
    encode_data_event("[DONE]")
}

fn encode_error_event(message: &str) -> Bytes {
    encode_data_event(&json!({
        "error": {
            "message": message,
            "type": "server_error"
        }
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::EmbedderHandle,
        config::{AuthConfig, OpenAiChatConfig},
        db::{GraphConfig, ItemRecord, SqliteVectorStore, VectorStore},
        embedding::EmbeddingService,
    };
    use std::sync::Arc;

    struct MockEmbedder;

    impl EmbeddingService for MockEmbedder {
        fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3, 0.4])
        }
    }

    #[test]
    fn upstream_payload_uses_server_tools_only() {
        let request = ChatCompletionsRequest {
            model: None,
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: Some(Value::String("hello".to_owned())),
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
        assert_eq!(payload["tools"].as_array().map(Vec::len), Some(1));
        assert_eq!(payload.get("tool_choice"), None);
    }

    #[test]
    fn upstream_payload_omits_null_message_fields() {
        let request = ChatCompletionsRequest {
            model: None,
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: Some(Value::String("hi".to_owned())),
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
                ("reasoning_format".to_owned(), Value::String("auto".to_owned())),
            ]),
        };

        let payload = build_upstream_payload(&request, &request.messages, "current_model.gguf")
            .expect("payload should build");
        let message = payload["messages"][0]
            .as_object()
            .expect("message should be object");

        assert_eq!(message.get("role"), Some(&Value::String("user".to_owned())));
        assert_eq!(message.get("content"), Some(&Value::String("hi".to_owned())));
        assert_eq!(message.get("name"), None);
        assert_eq!(message.get("tool_call_id"), None);
        assert_eq!(message.get("tool_calls"), None);
        assert_eq!(payload.get("temperature"), None);
        assert_eq!(payload.get("max_completion_tokens"), None);
        assert_eq!(payload.get("parallel_tool_calls"), None);
        assert_eq!(payload["return_progress"], Value::Bool(true));
        assert_eq!(payload["reasoning_format"], Value::String("auto".to_owned()));
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
        };
        store
            .upsert_item(item, &[0.1, 0.2, 0.3, 0.4])
            .expect("item should store");

        let state = AppState::new(
            Arc::new(EmbedderHandle::ready(Arc::new(MockEmbedder))),
            store as Arc<dyn VectorStore>,
            AuthConfig::default(),
            OpenAiChatConfig {
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
        );

        let result = search_entries_tool(
            &state,
            r#"{"query":"hello","source_id":"knowledge","hybrid":false}"#,
        )
        .await
        .expect("tool execution should succeed");

        let parsed: Value = serde_json::from_str(&result).expect("tool output should be JSON");
        assert_eq!(parsed["results"].as_array().map(Vec::len), Some(1));
        assert_eq!(parsed["results"][0]["id"], Value::String("doc-1".to_owned()));
    }
}