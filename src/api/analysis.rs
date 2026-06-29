//! LLM-on-store analysis pass.
//!
//! Given a candidate entry, embed it, retrieve top-K neighbors above a
//! similarity threshold, and ask an OpenAI-compatible chat backend to
//! classify the candidate against each neighbor (agrees / refines /
//! supersedes / contradicts / duplicates / unrelated) plus extract
//! cluster_hint, tags, title, summary, doc_type, freshness, quality.
//!
//! Tolerant deserializer: missing fields default, target_id brackets are
//! stripped, confidences are clamped to [0, 1]. Use a permissive
//! `response_format: json_object` instead of strict `json_schema` —
//! testing showed strict schema collapses 4B model quality.
//!
//! The dry-run endpoint at `POST /api/store/analyze` returns the
//! `StoreAnalysis` without writing anything; the entry-view re-run
//! button calls it on every edit. Background persistence is wired
//! from `store_entry_core` via `spawn_analysis`.

use super::{ApiError, AppState};
use crate::db::{ManualEdgeInput, SearchHit};
use anyhow::{Result, anyhow};
use axum::{Json, extract::State};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

/// Pass 1: summary metadata only. Input is the new entry text — no
/// neighbors, no graph context. Cheap, parallel.
const SUMMARY_PROMPT: &str = r#"You classify and summarize a knowledge-base entry. Output JSON only:
{
  "title": "one line",
  "summary": "1-2 sentences",
  "tags": ["lowercase-kebab", ...],
  "cluster_hint": "kebab-case-slug",
  "doc_type": "decision|architecture|todo|note|incident|reference",
  "freshness": "current|stale|historical",
  "quality": {"score": 0.0, "issues": ["..."]}
}
No prose, no commentary."#;

/// Pass 2: near-duplicate / replacement check. Neighbors are tight matches
/// (low distance). Each carries `created` and `updated` timestamps so the
/// model can reason about which entry is newer when facts conflict. Output
/// is focused on agrees/refines/supersedes/contradicts/duplicates.
const CLOSE_PROMPT: &str = r#"You compare a NEW knowledge-base entry against tightly-similar NEIGHBOR entries. The entries are already known to be on the same topic; decide the precise relation for each.

RELATIONS (NEW vs NEIGHBOR):
- agrees: same fact, no new info
- refines: NEW adds detail; neighbor still true
- supersedes: neighbor outdated/obsolete; NEW is the new truth
- contradicts: facts conflict; cannot both be true (and neighbor is not just stale)
- duplicates: essentially the same entry

DATE HEURISTICS (each neighbor has created/updated; NEW is brand new = now):
- If neighbor is OLDER and facts conflict → likely supersedes.
- If neighbor was UPDATED RECENTLY and facts conflict → likely contradicts.
- If neighbor has freshness=stale/historical and facts conflict → supersedes.
- A small detail added on top of an older fact → refines, not supersedes.

Output JSON only:
{
  "verdicts": [{"target_id": "<id>", "relation": "agrees|refines|supersedes|contradicts|duplicates", "confidence": 0.0, "reason": "..."}],
  "suggested_edges": [{"target_id": "<id>", "rel": "refines|supersedes|contradicts|related", "weight": 0.0}]
}
Use the exact id strings from the neighbor list. Skip neighbors that turn out unrelated — do not emit a verdict for them."#;

/// Pass 3: loose-similarity discovery. Neighbors are in the outer band —
/// embedding said "kinda similar" but not duplicate-close. Goal: surface
/// non-obvious connections (shared concept, cross-reference, prerequisite)
/// that pure similarity would miss.
const LOOSE_PROMPT: &str = r#"You scan a NEW knowledge-base entry against loosely-similar NEIGHBOR entries. Most will be unrelated. Find the FEW that have a real conceptual link the embedding alone would miss (shared system, prerequisite, cross-reference, same incident).

Output JSON only:
{
  "suggested_edges": [{"target_id": "<id>", "rel": "related|refines", "weight": 0.0}]
}
Rules:
- Only emit an edge when the link is real. Empty list is the correct answer when nothing connects.
- Do not emit supersedes/contradicts here — distance is too high for those.
- weight in [0,1]: how confident the link is.
Use exact id strings from the neighbor list."#;

/// Output of a single analysis pass. All fields tolerant: missing → default.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct StoreAnalysis {
    #[serde(default)]
    pub verdicts: Vec<Verdict>,
    #[serde(default)]
    pub suggested_edges: Vec<SuggestedEdge>,
    #[serde(default)]
    pub cluster_hint: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub doc_type: Option<String>,
    #[serde(default)]
    pub freshness: Option<String>,
    #[serde(default)]
    pub quality: Option<Quality>,
    /// Raw model output for debugging.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Verdict {
    #[serde(default, deserialize_with = "strip_brackets")]
    pub target_id: String,
    #[serde(default)]
    pub relation: String,
    #[serde(default, deserialize_with = "clamp_unit")]
    pub confidence: f32,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SuggestedEdge {
    #[serde(default, deserialize_with = "strip_brackets")]
    pub target_id: String,
    #[serde(default)]
    pub rel: String,
    #[serde(default, deserialize_with = "clamp_unit")]
    pub weight: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Quality {
    #[serde(default, deserialize_with = "clamp_unit")]
    pub score: f32,
    #[serde(default)]
    pub issues: Vec<String>,
}

fn strip_brackets<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let raw = String::deserialize(d)?;
    Ok(raw.trim_matches(|c: char| c == '[' || c == ']').to_owned())
}

fn clamp_unit<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    let raw = f32::deserialize(d).unwrap_or(0.0);
    // Some models return 0-100 percentages; rescale when clearly in that range.
    let v = if raw > 1.5 && raw <= 100.0 {
        raw / 100.0
    } else {
        raw
    };
    Ok(v.clamp(0.0, 1.0))
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    pub text: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub exclude_id: Option<String>,
}

/// MCP tool parameters for `analyze_entry` — same shape as the HTTP
/// request but JsonSchema-derived for tool surface advertising.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AnalyzeEntryParams {
    /// Candidate text to analyze. Required.
    pub text: String,
    /// Namespace to restrict neighbor search. Omit for global.
    #[serde(default)]
    pub source_id: Option<String>,
    /// If `text` belongs to an existing item, pass its id to exclude it
    /// from neighbor results (avoids self-comparison).
    #[serde(default)]
    pub exclude_id: Option<String>,
}

pub async fn analyze_endpoint(
    State(state): State<AppState>,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<StoreAnalysis>, ApiError> {
    if request.text.trim().is_empty() {
        return Err(ApiError::BadRequest("text must not be empty".to_owned()));
    }
    if !state.analysis.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "analysis not configured (set RAG_ANALYSIS_ENABLED + base_url + model)".to_owned(),
        ));
    }
    let analysis = run_analysis(
        &state,
        &request.text,
        request.source_id.as_deref(),
        request.exclude_id.as_deref(),
    )
    .await
    .map_err(|e| ApiError::Internal(anyhow!(e.to_string())))?;
    Ok(Json(analysis))
}

/// Public entry point: embed, fetch neighbors, run three LLM passes in
/// parallel (summary / close-similar / loose-similar) and merge into one
/// [`StoreAnalysis`]. Each pass hits a (possibly different) backend URL
/// from `analysis.base_urls` via round-robin, so multiple slower LLM
/// instances can share the load.
#[tracing::instrument(
    name = "analysis.run",
    skip(state, text),
    fields(
        text_len = text.len(),
        source_id = source_id.unwrap_or("*"),
        neighbors_found = tracing::field::Empty,
        close_count = tracing::field::Empty,
        loose_count = tracing::field::Empty,
        llm_ms = tracing::field::Empty,
        verdicts = tracing::field::Empty,
        tags = tracing::field::Empty,
    )
)]
pub async fn run_analysis(
    state: &AppState,
    text: &str,
    source_id: Option<&str>,
    exclude_id: Option<&str>,
) -> Result<StoreAnalysis> {
    let span = tracing::Span::current();
    let neighbors = fetch_neighbors(state, text, source_id, exclude_id).await?;
    span.record("neighbors_found", neighbors.len());

    let close_threshold = state.analysis.close_threshold;
    let (close, loose): (Vec<_>, Vec<_>) = neighbors
        .into_iter()
        .partition(|h| h.distance <= close_threshold);
    span.record("close_count", close.len());
    span.record("loose_count", loose.len());

    let started = std::time::Instant::now();

    let summary_prompt = build_summary_prompt(text);
    let close_prompt_opt = if close.is_empty() {
        None
    } else {
        Some(build_close_prompt(text, &close))
    };
    let loose_prompt_opt = if loose.is_empty() {
        None
    } else {
        Some(build_loose_prompt(text, &loose))
    };

    let summary_fut = call_llm(state, SUMMARY_PROMPT, &summary_prompt);
    let close_fut = async {
        match close_prompt_opt.as_deref() {
            Some(p) => call_llm(state, CLOSE_PROMPT, p).await.map(Some),
            None => Ok(None),
        }
    };
    let loose_fut = async {
        match loose_prompt_opt.as_deref() {
            Some(p) => call_llm(state, LOOSE_PROMPT, p).await.map(Some),
            None => Ok(None),
        }
    };

    let (summary_raw, close_raw, loose_raw) = tokio::try_join!(summary_fut, close_fut, loose_fut)?;
    span.record("llm_ms", started.elapsed().as_millis() as i64);

    let mut merged = parse_analysis(&summary_raw);
    // The summary call doesn't emit verdicts/edges; clear any stray ones a
    // chatty model returned so we don't double-count downstream.
    merged.verdicts.clear();
    merged.suggested_edges.clear();

    let mut raw_parts = vec![format!("SUMMARY:\n{summary_raw}")];

    if let Some(raw) = close_raw {
        let close_parsed = parse_analysis(&raw);
        merged.verdicts.extend(close_parsed.verdicts);
        merged.suggested_edges.extend(close_parsed.suggested_edges);
        raw_parts.push(format!("CLOSE:\n{raw}"));
    }
    if let Some(raw) = loose_raw {
        let loose_parsed = parse_analysis(&raw);
        // Loose pass: edges only. Drop any verdicts the model snuck in;
        // distance is too high to trust supersedes/contradicts here.
        merged.suggested_edges.extend(loose_parsed.suggested_edges);
        raw_parts.push(format!("LOOSE:\n{raw}"));
    }

    merged.raw = Some(raw_parts.join("\n\n---\n\n"));

    span.record("verdicts", merged.verdicts.len());
    span.record("tags", merged.tags.len());
    Ok(merged)
}

#[tracing::instrument(name = "analysis.fetch_neighbors", skip(state, text))]
async fn fetch_neighbors(
    state: &AppState,
    text: &str,
    source_id: Option<&str>,
    exclude_id: Option<&str>,
) -> Result<Vec<SearchHit>> {
    let embedder = state
        .embedder
        .get_ready()
        .map_err(|e| anyhow!(e.to_string()))?;
    let store = state.store.clone();
    let owned_text = text.to_owned();
    let owned_source = source_id.map(str::to_owned);
    let max_neighbors = state.analysis.max_neighbors.max(1);
    let threshold = state.analysis.neighbor_threshold;
    let excluded = exclude_id.map(str::to_owned);

    let hits = tokio::task::spawn_blocking(move || -> Result<Vec<SearchHit>> {
        let (dense, sparse) = embedder.embed_both(&owned_text)?;
        let hits = store.search_hybrid(
            &owned_text,
            &dense,
            &sparse,
            max_neighbors + 1,
            owned_source.as_deref(),
            None,
        )?;
        Ok(hits
            .into_iter()
            .filter(|h| h.distance <= threshold)
            .filter(|h| excluded.as_deref().map(|x| h.id != x).unwrap_or(true))
            .take(max_neighbors)
            .collect())
    })
    .await
    .map_err(|e| anyhow!("neighbor lookup task join: {e}"))??;

    Ok(hits)
}

fn build_summary_prompt(new_text: &str) -> String {
    let mut out = String::with_capacity(new_text.len() + 64);
    out.push_str("ENTRY:\n");
    out.push_str(new_text.trim());
    out.push_str("\n\nReturn JSON only matching the schema.");
    out
}

fn build_close_prompt(new_text: &str, neighbors: &[SearchHit]) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let mut out = String::new();
    out.push_str("NEW ENTRY (just authored, treat as 'now'):\n");
    out.push_str(new_text.trim());
    out.push_str("\n\nTIGHTLY-SIMILAR NEIGHBORS:\n");
    for (i, hit) in neighbors.iter().enumerate() {
        let preview: String = hit
            .text
            .chars()
            .take(500)
            .collect::<String>()
            .replace('\n', " ");
        let freshness = hit
            .metadata
            .get("freshness")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        out.push_str(&format!(
            "[{}] id={} src={} freshness={} dist={:.3} created={} updated={}\n{}\n\n",
            i,
            hit.id,
            hit.source_id,
            freshness,
            hit.distance,
            format_age(hit.created_at, now_ms),
            format_age(hit.updated_at, now_ms),
            preview
        ));
    }
    out.push_str("Return JSON only. Use exact ids (no brackets). Skip unrelated neighbors entirely.");
    out
}

fn build_loose_prompt(new_text: &str, neighbors: &[SearchHit]) -> String {
    let mut out = String::new();
    out.push_str("NEW ENTRY:\n");
    out.push_str(new_text.trim());
    out.push_str("\n\nLOOSELY-SIMILAR NEIGHBORS (embedding distance is moderate; many will be unrelated):\n");
    for (i, hit) in neighbors.iter().enumerate() {
        let preview: String = hit
            .text
            .chars()
            .take(300)
            .collect::<String>()
            .replace('\n', " ");
        out.push_str(&format!(
            "[{}] id={} src={} dist={:.3}\n{}\n\n",
            i, hit.id, hit.source_id, hit.distance, preview
        ));
    }
    out.push_str("Return JSON only. Empty suggested_edges is the right answer when nothing connects.");
    out
}

/// Render a stored epoch-millis timestamp as a human-friendly relative age
/// (e.g. `3d ago`, `2h ago`). The LLM reads this; absolute ms would just
/// be noise. Returns `"unknown"` for zero/missing timestamps.
fn format_age(ts_ms: i64, now_ms: i64) -> String {
    if ts_ms <= 0 {
        return "unknown".to_owned();
    }
    let delta_ms = (now_ms - ts_ms).max(0);
    let secs = delta_ms / 1000;
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Settings for one `chat/completions` call, independent of `AppState`.
/// Use this when the caller is outside the API layer (e.g. the ontology
/// worker) and needs to share the same battle-tested HTTP/parse code as
/// the analysis endpoint.
pub struct ChatCompletionRequest<'a> {
    pub base_url: &'a str,
    pub api_key: Option<&'a str>,
    pub model: &'a str,
    pub timeout_secs: u64,
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    pub max_tokens: usize,
    pub temperature: f32,
    /// Set the OpenAI `response_format: {type: "json_object"}` so the
    /// server constrains the model to valid JSON. Some llama.cpp builds
    /// ignore this — we still strip code fences and fall back to
    /// reasoning_content in `chat_completion_text` either way.
    pub response_format_json: bool,
}

/// POST one chat completion and return the assistant content as a string.
///
/// Hardened against:
/// - non-2xx HTTP (returns an error with status + body preview),
/// - empty body / non-JSON envelope (errors with preview),
/// - thinking-mode models that put their answer in `reasoning_content`
///   instead of `content` — falls back automatically and emits a `warn!`
///   so the operator knows the upstream `enable_thinking=false` is being
///   ignored.
///
/// Newlines in previews are escaped to `\n` so each log line stays on
/// one line. Used by both `analyze_endpoint` and the ontology worker.
pub async fn chat_completion_text(
    http_client: &reqwest::Client,
    req: ChatCompletionRequest<'_>,
) -> Result<String> {
    let base_url = req.base_url.trim_end_matches('/');
    tracing::Span::current().record("model", req.model);

    let mut payload = json!({
        "model": req.model,
        "stream": false,
        "temperature": req.temperature,
        "max_tokens": req.max_tokens,
        // Hard-off thinking via chat_template_kwargs — llama.cpp ignores
        // reasoning_effort / thinking_budget flags, this is the only
        // switch that actually disables it.
        "chat_template_kwargs": {"enable_thinking": false},
        "messages": [
            {"role": "system", "content": req.system_prompt},
            {"role": "user", "content": req.user_prompt},
        ],
    });
    if req.response_format_json {
        payload["response_format"] = json!({"type": "json_object"});
    }

    let mut http_req = http_client
        .post(format!("{base_url}/chat/completions"))
        .timeout(std::time::Duration::from_secs(req.timeout_secs.max(1)))
        .json(&payload);
    if let Some(key) = req.api_key {
        http_req = http_req.bearer_auth(key);
    }

    let resp = http_req.send().await?;
    let status = resp.status();
    tracing::Span::current().record("http_status", status.as_u16());
    let body = resp
        .text()
        .await
        .map_err(|e| anyhow!("LLM read body failed (status {status}): {e}"))?;

    if !status.is_success() {
        tracing::warn!(
            status = %status,
            body_preview = %truncate(&body, 500),
            "LLM HTTP error"
        );
        return Err(anyhow!("LLM HTTP {status}: {}", truncate(&body, 200)));
    }

    if body.trim().is_empty() {
        tracing::warn!("LLM returned empty body");
        return Err(anyhow!("LLM returned empty response body"));
    }

    let envelope: Value = serde_json::from_str(&body).map_err(|e| {
        tracing::warn!(
            error = %e,
            body_preview = %truncate(&body, 500),
            "LLM envelope is not valid JSON"
        );
        anyhow!(
            "LLM envelope not valid JSON ({e}); preview: {}",
            truncate(&body, 200)
        )
    })?;

    let message = envelope
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"));
    let primary = message
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let (content, source) = if !primary.is_empty() {
        (primary.to_owned(), "content")
    } else {
        let reasoning = message
            .and_then(|m| m.get("reasoning_content"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        (reasoning.to_owned(), "reasoning_content")
    };

    if content.is_empty() {
        tracing::warn!(
            envelope_preview = %truncate(&body, 500),
            "LLM returned envelope with empty assistant content (and empty reasoning_content)"
        );
        return Err(anyhow!("LLM choice has empty content"));
    }

    if source == "reasoning_content" {
        tracing::warn!(
            "model put output in reasoning_content despite enable_thinking=false — falling back"
        );
    }

    tracing::Span::current().record("output_len", content.len());
    Ok(content)
}

/// Single-line preview helper — replaces newlines so log lines stay tidy.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.replace('\n', "\\n")
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out.replace('\n', "\\n")
    }
}

/// Thin wrapper around [`chat_completion_text`] that pulls config from
/// `AppState.analysis` — what the `/api/store/analyze` endpoint uses.
pub(crate) async fn call_llm(
    state: &AppState,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let cfg = &state.analysis;
    let base_url = cfg
        .pick_base_url()
        .ok_or_else(|| anyhow!("analysis base_url missing"))?;
    let model = cfg
        .model
        .as_deref()
        .ok_or_else(|| anyhow!("analysis model missing"))?;
    chat_completion_text(
        &state.http_client,
        ChatCompletionRequest {
            base_url,
            api_key: cfg.api_key.as_deref(),
            model,
            timeout_secs: cfg.timeout_secs,
            system_prompt,
            user_prompt,
            max_tokens: 2000,
            temperature: 0.05,
            response_format_json: true,
        },
    )
    .await
}

pub(crate) fn parse_analysis(raw: &str) -> StoreAnalysis {
    let stripped = strip_code_fences(raw);
    // Find the outermost JSON object.
    let start = stripped.find('{');
    let end = stripped.rfind('}');
    let candidate = match (start, end) {
        (Some(s), Some(e)) if e > s => &stripped[s..=e],
        _ => stripped.as_str(),
    };
    let mut parsed: StoreAnalysis = serde_json::from_str(candidate).unwrap_or_default();
    parsed.raw = Some(raw.to_owned());
    parsed
}

fn strip_code_fences(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let body = rest.split_once('\n').map(|(_, x)| x).unwrap_or(rest);
        if let Some(end) = body.rfind("```") {
            return body[..end].trim().to_owned();
        }
        return body.trim().to_owned();
    }
    t.to_owned()
}

/// Run the analysis in the background after a successful write, persisting
/// the result onto the item via `update_item_analysis`. Errors are logged
/// and swallowed — analysis is best-effort and must never affect the write
/// path.
pub fn spawn_analysis(state: AppState, item_id: String, text: String, source_id: String) {
    if !state.analysis.is_configured() {
        return;
    }
    tokio::spawn(async move {
        let span = tracing::info_span!(
            "analysis.on_store",
            item_id = %item_id,
            source_id = %source_id,
            outcome = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _g = span.enter();
        let started = std::time::Instant::now();
        let neighbor_source = if state.analysis.cross_source {
            None
        } else {
            Some(source_id.as_str())
        };
        match run_analysis(&state, &text, neighbor_source, Some(&item_id)).await {
            Ok(analysis) => {
                let model = state
                    .analysis
                    .model
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned());
                let json = match serde_json::to_string(&analysis) {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::warn!(error=%e, "analysis serialize failed");
                        span.record("outcome", "serialize_err");
                        span.record("elapsed_ms", started.elapsed().as_millis() as i64);
                        return;
                    }
                };
                // All VectorStore calls below are sync; on Postgres they call
                // `runtime.block_on()` internally, which PANICS when invoked
                // from a tokio runtime worker thread (i.e. from inside this
                // `tokio::spawn` future). Wrap in `spawn_blocking` so the
                // sync calls execute on the blocking pool. Without this, the
                // first `update_item_analysis` panics silently and nothing
                // gets persisted — even though `run_analysis` succeeded.
                let store = state.store.clone();
                let id_owned = item_id.clone();
                let json_owned = json.clone();
                let model_owned = model.clone();
                let persist = tokio::task::spawn_blocking(move || {
                    store.update_item_analysis(&id_owned, &json_owned, &model_owned)
                })
                .await;
                if let Err(e) = persist.unwrap_or_else(|e| Err(anyhow!("persist join: {e}"))) {
                    tracing::warn!(error=%e, "analysis persist failed");
                    span.record("outcome", "persist_err");
                } else {
                    // Promote LLM-derived tags onto the item's metadata.tags
                    // so they participate in list/search filtering. Best-effort.
                    if !analysis.tags.is_empty() {
                        let store = state.store.clone();
                        let id_owned = item_id.clone();
                        let tags = analysis.tags.clone();
                        let res = tokio::task::spawn_blocking(move || {
                            store.merge_item_tags(&id_owned, &tags)
                        })
                        .await;
                        if let Err(e) = res.unwrap_or_else(|e| Err(anyhow!("tag merge join: {e}")))
                        {
                            tracing::warn!(error=%e, "tag merge failed");
                        }
                    }

                    // Create graph edges for high-quality analysis verdicts.
                    for verdict in &analysis.verdicts {
                        let (input, label) = if verdict.relation == "unrelated" {
                            (
                                ManualEdgeInput {
                                    from_item_id: item_id.clone(),
                                    to_item_id: verdict.target_id.clone(),
                                    relation: Some(std::borrow::Cow::Borrowed("unrelated")),
                                    sort_order: None,
                                    weight: -1.0,
                                    directed: false,
                                    metadata: serde_json::json!({
                                        "reason": verdict.reason,
                                        "confidence": verdict.confidence,
                                        "source": "analysis"
                                    }),
                                },
                                "anti-edge",
                            )
                        } else {
                            // "Proven" relationships from the analysis pass
                            // (agrees / refines / supersedes / contradicts / duplicates).
                            // Directed + confirmed so the graph UI surfaces them.
                            (
                                ManualEdgeInput {
                                    from_item_id: item_id.clone(),
                                    to_item_id: verdict.target_id.clone(),
                                    relation: Some(std::borrow::Cow::Owned(
                                        verdict.relation.clone(),
                                    )),
                                    sort_order: None,
                                    weight: verdict.confidence,
                                    directed: true,
                                    metadata: serde_json::json!({
                                        "reason": verdict.reason,
                                        "confidence": verdict.confidence,
                                        "source": "analysis",
                                        "status": "confirmed"
                                    }),
                                },
                                "analysis edge",
                            )
                        };
                        let store = state.store.clone();
                        let target = verdict.target_id.clone();
                        let relation = verdict.relation.clone();
                        let res =
                            tokio::task::spawn_blocking(move || store.add_manual_edge(input)).await;
                        if let Err(e) = res
                            .map_err(|e| anyhow!("edge join: {e}"))
                            .and_then(|r| r.map(|_| ()))
                        {
                            tracing::warn!(
                                error = %e,
                                target_id = %target,
                                relation = %relation,
                                kind = label,
                                "failed to create graph edge"
                            );
                        }
                    }
                    tracing::info!(
                        verdicts = analysis.verdicts.len(),
                        tags = analysis.tags.len(),
                        suggested_edges = analysis.suggested_edges.len(),
                        doc_type = analysis.doc_type.as_deref().unwrap_or(""),
                        freshness = analysis.freshness.as_deref().unwrap_or(""),
                        "analysis stored"
                    );
                    span.record("outcome", "ok");
                }
            }
            Err(e) => {
                tracing::warn!(error=%e, "analysis run failed");
                span.record("outcome", "run_err");
            }
        }
        span.record("elapsed_ms", started.elapsed().as_millis() as i64);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strips_brackets_and_clamps() {
        let raw = r#"{"verdicts":[{"target_id":"[a]","relation":"contradicts","confidence":95,"reason":"x"}],"quality":{"score":1.5,"issues":[]}}"#;
        let p = parse_analysis(raw);
        assert_eq!(p.verdicts.len(), 1);
        assert_eq!(p.verdicts[0].target_id, "a");
        assert!((p.verdicts[0].confidence - 0.95).abs() < 1e-3);
        assert!((p.quality.unwrap().score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_handles_code_fence() {
        let raw = "```json\n{\"tags\":[\"k8s\"]}\n```";
        let p = parse_analysis(raw);
        assert_eq!(p.tags, vec!["k8s"]);
    }

    #[test]
    fn parse_garbage_returns_default() {
        let p = parse_analysis("not json");
        assert!(p.verdicts.is_empty());
        assert!(p.tags.is_empty());
    }
}
