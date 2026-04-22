use super::{ApiError, AppState, SearchResultPayload};
use anyhow::anyhow;
use async_stream::stream;
use axum::{
    Json,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, convert::Infallible};

const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const MAX_SUB_QUERIES: usize = 8;

fn default_top_k() -> usize {
    5
}

fn default_max_distance() -> f32 {
    0.8
}

#[derive(Debug, Deserialize)]
pub struct AssistedQueryRequest {
    pub query: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default = "default_max_distance")]
    pub max_distance: f32,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
struct QueriesEvent<'a> {
    object: &'a str,
    queries: &'a [String],
}

#[derive(Debug, Serialize)]
struct QueryResultEvent<'a> {
    object: &'a str,
    query: &'a str,
    index: usize,
    results: Vec<SearchResultPayload>,
}

#[derive(Debug, Serialize)]
struct MergedEvent<'a> {
    object: &'a str,
    results: &'a [SearchResultPayload],
}

pub(super) async fn assisted_query(
    State(state): State<AppState>,
    Json(request): Json<AssistedQueryRequest>,
) -> std::result::Result<Response, ApiError> {
    let query = request.query.trim().to_owned();
    if query.is_empty() {
        return Err(ApiError::BadRequest("query must not be empty".to_owned()));
    }

    if request.top_k == 0 {
        return Err(ApiError::BadRequest(
            "top_k must be greater than zero".to_owned(),
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

    let top_k = request.top_k;
    let max_distance = request.max_distance;
    let source_id = request.source_id.clone();

    let body_stream = Body::from_stream(stream! {
        let sub_queries = match generate_sub_queries(&state, &openai_config, &model, &query).await {
            Ok(queries) => queries,
            Err(error) => {
                yield Ok::<_, Infallible>(encode_error_event(&error.to_string()));
                yield Ok::<_, Infallible>(encode_done_event());
                return;
            }
        };

        let effective_queries: Vec<String> = if sub_queries.is_empty() {
            vec![query.clone()]
        } else {
            sub_queries
        };

        let queries_payload = serde_json::to_string(&QueriesEvent {
            object: "assisted_query.queries",
            queries: &effective_queries,
        })
        .unwrap_or_else(|_| "{}".to_owned());
        yield Ok::<_, Infallible>(encode_data_event(&queries_payload));

        let mut merged: Vec<SearchResultPayload> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for (index, sub_query) in effective_queries.iter().enumerate() {
            let results = match run_search(&state, sub_query, source_id.as_deref(), top_k, max_distance).await {
                Ok(results) => results,
                Err(error) => {
                    yield Ok::<_, Infallible>(encode_error_event(&format!(
                        "search for sub-query {index} failed: {error}"
                    )));
                    continue;
                }
            };

            let event_payload = serde_json::to_string(&QueryResultEvent {
                object: "assisted_query.result",
                query: sub_query,
                index,
                results: results.clone(),
            })
            .unwrap_or_else(|_| "{}".to_owned());
            yield Ok::<_, Infallible>(encode_data_event(&event_payload));

            for hit in results {
                if seen_ids.insert(hit.id.clone()) {
                    merged.push(hit);
                } else if let Some(existing) = merged.iter_mut().find(|existing| existing.id == hit.id) {
                    if hit.distance < existing.distance {
                        existing.distance = hit.distance;
                    }
                }
            }
        }

        merged.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
        merged.truncate(top_k);

        let merged_payload = serde_json::to_string(&MergedEvent {
            object: "assisted_query.merged",
            results: &merged,
        })
        .unwrap_or_else(|_| "{}".to_owned());
        yield Ok::<_, Infallible>(encode_data_event(&merged_payload));

        yield Ok::<_, Infallible>(encode_done_event());
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

async fn generate_sub_queries(
    state: &AppState,
    openai_config: &crate::config::OpenAiChatConfig,
    model: &str,
    user_query: &str,
) -> anyhow::Result<Vec<String>> {
    let base_url = openai_config
        .base_url
        .as_deref()
        .ok_or_else(|| anyhow!("upstream base_url not configured"))?
        .trim_end_matches('/');

    let system_content = format!(
        "{}\n\n{}",
        openai_config.retrieval_system_prompt, openai_config.query_expansion_prompt,
    );

    let payload = json!({
        "model": model,
        "stream": false,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": system_content },
            { "role": "user", "content": user_query }
        ]
    });

    let mut request_builder = state
        .http_client
        .post(format!("{}{}", base_url, CHAT_COMPLETIONS_PATH))
        .json(&payload);

    if let Some(api_key) = openai_config.api_key.as_deref() {
        request_builder = request_builder.bearer_auth(api_key);
    }

    let response = request_builder.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| status.to_string());
        return Err(anyhow!(
            "upstream chat provider returned {status}: {body}"
        ));
    }

    let body: Value = response.json().await?;
    let content = body
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .unwrap_or("")
        .trim()
        .to_owned();

    if content.is_empty() {
        return Ok(Vec::new());
    }

    let queries = parse_query_array(&content)?;
    let mut trimmed: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for raw in queries.into_iter() {
        let candidate = raw.trim().to_owned();
        if candidate.is_empty() {
            continue;
        }
        let key = candidate.to_lowercase();
        if seen.insert(key) {
            trimmed.push(candidate);
        }
        if trimmed.len() >= MAX_SUB_QUERIES {
            break;
        }
    }

    Ok(trimmed)
}

fn parse_query_array(content: &str) -> anyhow::Result<Vec<String>> {
    let stripped = strip_code_fences(content);
    let start = stripped.find('[');
    let end = stripped.rfind(']');
    let candidate = match (start, end) {
        (Some(s), Some(e)) if e > s => &stripped[s..=e],
        _ => stripped.as_str(),
    };

    let value: Value = serde_json::from_str(candidate)
        .map_err(|error| anyhow!("failed to parse query array: {error}"))?;
    match value {
        Value::Array(items) => Ok(items
            .into_iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s),
                other => other.as_str().map(str::to_owned),
            })
            .collect()),
        _ => Err(anyhow!("expected a JSON array of strings")),
    }
}

fn strip_code_fences(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let without_lang = rest.split_once('\n').map(|(_, tail)| tail).unwrap_or(rest);
        if let Some(end) = without_lang.rfind("```") {
            return without_lang[..end].trim().to_owned();
        }
        return without_lang.trim().to_owned();
    }
    trimmed.to_owned()
}

async fn run_search(
    state: &AppState,
    query: &str,
    source_id: Option<&str>,
    top_k: usize,
    max_distance: f32,
) -> anyhow::Result<Vec<SearchResultPayload>> {
    let embedder = state.embedder.get_ready().map_err(|error| anyhow!(error.to_string()))?;
    let store = state.store.clone();
    let query_owned = query.to_owned();
    let source_owned = source_id.map(|s| s.to_owned());

    let results = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<SearchResultPayload>> {
        let embedding = embedder.embed(&query_owned)?;
        let hits = store.search_hybrid(&query_owned, &embedding, top_k, source_owned.as_deref())?;
        Ok(hits
            .into_iter()
            .filter(|hit| hit.distance <= max_distance)
            .map(SearchResultPayload::from)
            .collect())
    })
    .await??;

    Ok(results)
}

fn encode_data_event(data: &str) -> Bytes {
    Bytes::from(format!("data: {data}\n\n"))
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
