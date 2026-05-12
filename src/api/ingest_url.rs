//! URL ingestion with optional CDP and LLM cleaning.

use axum::{Extension, Json, extract::State, http::StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use crate::api::{ApiError, AppState, SessionSubject, StoreRequest, StoreResponse, store_entry_core};
use chromiumoxide::browser::Browser;
use futures_util::StreamExt;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IngestUrlRequest {
    /// Remote URL to fetch.
    pub url: String,
    /// Namespace/category for the entry.
    pub source_id: String,
    /// When true, use the remote CDP instance (RAG_CDP_URL) to fetch the page.
    /// Useful for JavaScript-heavy sites.
    #[serde(default)]
    pub use_cdp: bool,
    /// When true, use the LLM to extract main content and remove boilerplate.
    #[serde(default)]
    pub llm_clean: bool,
    /// Optional wiki path.
    #[serde(default)]
    pub path: Option<String>,
    /// Optional structured-data type name.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// Optional chunking configuration. If omitted, uses the backend's default (e.g. md_chunker).
    #[serde(default)]
    pub chunk: Option<crate::api::ChunkConfig>,
}

pub async fn ingest_url(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    Json(req): Json<IngestUrlRequest>,
) -> Result<(StatusCode, Json<StoreResponse>), ApiError> {
    tracing::info!(url = %req.url, use_cdp = req.use_cdp, llm_clean = req.llm_clean, "starting URL ingestion");

    // 1. Fetch HTML or Markdown
    let html_or_md = if req.use_cdp {
        fetch_with_cdp(&state, &req.url).await?
    } else {
        match fetch_with_reqwest(&state, &req.url).await {
            Ok(res) => res,
            Err(e) => {
                if state.openai_chat.cdp_url.is_some() {
                    tracing::warn!(url = %req.url, error = %e, "reqwest fetch failed, falling back to CDP");
                    fetch_with_cdp(&state, &req.url).await?
                } else {
                    return Err(e);
                }
            }
        }
    };
    tracing::debug!(content_len = html_or_md.content.len(), is_markdown = html_or_md.is_markdown, "fetched content");
    
    // 2. HTML to Markdown (skip if already markdown)
    let is_markdown = html_or_md.is_markdown;
    let content = html_or_md.content;
    
    let md = if is_markdown {
        tracing::debug!("content is already markdown, skipping html2md");
        content
    } else {
        tokio::task::spawn_blocking(move || html2md::parse_html(&content))
            .await
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("html2md join error: {e}")))?
    };
    tracing::debug!(md_len = md.len(), "final markdown ready");

    // 3. Optional LLM Cleaning
    let final_text = if req.llm_clean && state.openai_chat.is_configured() {
        tracing::info!("cleaning markdown with LLM");
        clean_with_llm(&state, &md).await?
    } else {
        md
    };
    tracing::debug!(final_len = final_text.len(), "final text ready");

    // 4. Store
    let store_req = StoreRequest {
        id: None,
        text: final_text,
        metadata: json!({
            "source_type": "url",
            "url": req.url,
            "fetched_at": crate::api::current_timestamp_millis()?,
            "use_cdp": req.use_cdp,
            "llm_clean": req.llm_clean,
        }),
        source_id: req.source_id,
        chunk: req.chunk, // Use requested config, or None to fall back to md_chunker
        path: req.path,
        type_name: req.type_name,
        data: None,
    };

    let resp = store_entry_core(&state, store_req, session.0).await?;
    tracing::info!(id = %resp.id, "URL ingestion completed");
    Ok((StatusCode::CREATED, Json(resp)))
}

pub(crate) struct FetchResult {
    pub content: String,
    pub is_markdown: bool,
}

pub(crate) async fn fetch_with_reqwest(state: &AppState, url: &str) -> Result<FetchResult, ApiError> {
    let resp = state.http_client.get(url)
        .header("Accept", "text/markdown, text/html;q=0.9, application/xhtml+xml;q=0.9, */*;q=0.8")
        .send().await
        .map_err(|e| ApiError::BadRequest(format!("fetch failed: {e}")))?;
    
    if !resp.status().is_success() {
        return Err(ApiError::BadRequest(format!("remote returned {}", resp.status())));
    }

    let is_markdown = resp.headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.contains("text/markdown"))
        .unwrap_or(false);
    
    let content = resp.text().await.map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to read response text: {e}")))?;
    Ok(FetchResult { content, is_markdown })
}

pub(crate) async fn fetch_with_cdp(state: &AppState, url: &str) -> Result<FetchResult, ApiError> {
    let cdp_url = state.openai_chat.cdp_url.as_ref()
        .ok_or_else(|| ApiError::BadRequest("RAG_CDP_URL not configured".to_owned()))?;

    tracing::debug!(cdp_url = %cdp_url, "connecting to remote CDP");
    
    // Connect to remote CDP
    let (mut browser, mut handler) = Browser::connect(cdp_url).await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("CDP connect failed: {e}")))?;

    tokio::spawn(async move {
        while let Some(_) = handler.next().await {}
    });

    let page = browser.new_page(url).await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("CDP new_page failed: {e}")))?;

    tracing::debug!("waiting for navigation");
    page.wait_for_navigation().await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("CDP navigation failed: {e}")))?;

    let content = page.content().await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("CDP get_content failed: {e}")))?;

    // Try to close gracefully
    let _ = browser.close().await;

    // CDP always returns serialized HTML from the DOM
    Ok(FetchResult { content, is_markdown: false })
}

const CLEAN_SYSTEM_PROMPT: &str = r#"You are a content extraction assistant.
Your goal is to extract the main meaningful content from the provided Markdown of a web page.

RULES:
1. Remove navigation bars, footers, sidebars, advertisements, and social media widgets.
2. Preserve the main headings (H1, H2, H3), lists, tables, and code blocks.
3. Keep the original Markdown formatting for the content you extract.
4. If the page contains an article, extract the article title and body.
5. Remove redundant links and boilerplate text like 'Click here to read more' or 'Privacy Policy'.
6. If the page is empty or contains no meaningful content, return an empty string.
7. Output the extracted Markdown ONLY. No preamble, no comments."#;

async fn clean_with_llm(state: &AppState, md: &str) -> Result<String, ApiError> {
    let cfg = &state.openai_chat;
    let base_url = cfg.base_url.as_deref().unwrap();
    let model = cfg.default_model.as_deref().unwrap_or("gpt-4o");
    
    let payload = json!({
        "model": model,
        "temperature": 0.0,
        "messages": [
            {"role": "system", "content": CLEAN_SYSTEM_PROMPT},
            {"role": "user", "content": format!("Extract content from this markdown:\n\n{}", md)}
        ],
    });

    let mut req = state.http_client.post(format!("{base_url}/chat/completions")).json(&payload);
    if let Some(key) = cfg.api_key.as_deref() {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await.map_err(|e| ApiError::Internal(anyhow::anyhow!("LLM request failed: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Internal(anyhow::anyhow!("LLM returned {}: {}", status, body)));
    }

    let body: Value = resp.json().await.map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to parse LLM response: {e}")))?;
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_default();

    Ok(content)
}
