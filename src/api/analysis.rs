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

const SYSTEM_PROMPT: &str = r#"You analyze a NEW knowledge-base entry against existing NEIGHBOR entries and output a single JSON object.

RELATIONS (always: NEW relates to NEIGHBOR):
- agrees: NEW states same fact as neighbor, no new info
- refines: NEW adds detail to a fact in neighbor; both still true
- supersedes: neighbor is outdated/obsolete; NEW is the new truth
- contradicts: facts conflict; cannot both be true (and neighbor is not just stale)
- duplicates: essentially the same entry
- unrelated: different topics

Heuristics:
- If NEW just ADDS DETAIL but neighbor is still true → refines (NOT supersedes).
- If neighbor has freshness=stale or historical and NEW conflicts → supersedes.
- If neighbor is current and NEW conflicts → contradicts.

Examples:
NEW: 'service uses Postgres 15'
- NEIGHBOR 'service uses Postgres' → refines
- NEIGHBOR 'service uses MySQL' (current) → contradicts
- NEIGHBOR 'service uses Postgres 14' (stale) → supersedes

Output JSON ONLY, no prose, matching this shape:
{
  "verdicts": [{"target_id": "<id>", "relation": "agrees|contradicts|supersedes|refines|duplicates|unrelated", "confidence": 0.0, "reason": "..."}],
  "suggested_edges": [{"target_id": "<id>", "rel": "related|refines|supersedes|contradicts", "weight": 0.0}],
  "cluster_hint": "kebab-case-slug",
  "tags": ["..."],
  "title": "one line",
  "summary": "1-2 sentences",
  "doc_type": "decision|architecture|todo|note|incident|reference",
  "freshness": "current|stale|historical",
  "quality": {"score": 0.0, "issues": ["..."]}
}"#;

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
    let v = if raw > 1.5 && raw <= 100.0 { raw / 100.0 } else { raw };
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

/// Public entry point: embed, fetch neighbors, prompt the LLM, parse.
#[tracing::instrument(
    name = "analysis.run",
    skip(state, text),
    fields(
        text_len = text.len(),
        source_id = source_id.unwrap_or("*"),
        neighbors_found = tracing::field::Empty,
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

    let user_prompt = build_user_prompt(text, &neighbors);
    let started = std::time::Instant::now();
    let raw = call_llm(state, SYSTEM_PROMPT, &user_prompt).await?;
    span.record("llm_ms", started.elapsed().as_millis() as i64);

    let parsed = parse_analysis(&raw);
    span.record("verdicts", parsed.verdicts.len());
    span.record("tags", parsed.tags.len());
    Ok(parsed)
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
            .filter(|h| {
                excluded
                    .as_deref()
                    .map(|x| h.id != x)
                    .unwrap_or(true)
            })
            .take(max_neighbors)
            .collect())
    })
    .await
    .map_err(|e| anyhow!("neighbor lookup task join: {e}"))??;

    Ok(hits)
}

fn build_user_prompt(new_text: &str, neighbors: &[SearchHit]) -> String {
    let mut out = String::new();
    out.push_str("NEW ENTRY:\n");
    out.push_str(new_text.trim());
    out.push_str("\n\nNEIGHBORS:\n");
    if neighbors.is_empty() {
        out.push_str("(no semantically similar neighbors found)\n");
    } else {
        for (i, hit) in neighbors.iter().enumerate() {
            let preview: String = hit
                .text
                .chars()
                .take(400)
                .collect::<String>()
                .replace('\n', " ");
            let freshness = hit
                .metadata
                .get("freshness")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            out.push_str(&format!(
                "[{}] id={} src={} freshness={} dist={:.3}\n{}\n\n",
                i, hit.id, hit.source_id, freshness, hit.distance, preview
            ));
        }
    }
    out.push_str("Return JSON only matching the schema. Use the exact id strings from above (without brackets).");
    out
}

pub(crate) async fn call_llm(state: &AppState, system_prompt: &str, user_prompt: &str) -> Result<String> {
    let cfg = &state.analysis;
    let base_url = cfg
        .base_url
        .as_deref()
        .ok_or_else(|| anyhow!("analysis base_url missing"))?
        .trim_end_matches('/');
    let model = cfg
        .model
        .as_deref()
        .ok_or_else(|| anyhow!("analysis model missing"))?;
    tracing::Span::current().record("model", model);

    let payload = json!({
        "model": model,
        "stream": false,
        "temperature": 0.05,
        "max_tokens": 2000,
        "chat_template_kwargs": {"enable_thinking": false},
        "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
    });

    let mut req = state
        .http_client
        .post(format!("{base_url}/chat/completions"))
        .timeout(std::time::Duration::from_secs(cfg.timeout_secs.max(1)))
        .json(&payload);
    if let Some(key) = cfg.api_key.as_deref() {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await?;
    let status = resp.status();
    tracing::Span::current().record("http_status", status.as_u16());
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_else(|_| status.to_string());
        tracing::warn!(status = %status, body = %body, "analysis LLM error");
        return Err(anyhow!("analysis LLM returned {status}: {body}"));
    }
    let body: Value = resp.json().await?;
    let content = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    tracing::Span::current().record("output_len", content.len());
    Ok(content)
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
        let neighbor_source = if state.analysis.cross_source { None } else { Some(source_id.as_str()) };
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
                if let Err(e) = state.store.update_item_analysis(&item_id, &json, &model) {
                    tracing::warn!(error=%e, "analysis persist failed");
                    span.record("outcome", "persist_err");
                } else {
                    // Promote LLM-derived tags onto the item's metadata.tags
                    // so they participate in list/search filtering. Best-effort.
                    if !analysis.tags.is_empty() {
                        if let Err(e) = state.store.merge_item_tags(&item_id, &analysis.tags) {
                            tracing::warn!(error=%e, "tag merge failed");
                        }
                    }

                    // Create "anti-edges" for unrelated verdicts to penalize them in search.
                    for verdict in &analysis.verdicts {
                        if verdict.relation == "unrelated" {
                            let input = ManualEdgeInput {
                                from_item_id: item_id.clone(),
                                to_item_id: verdict.target_id.clone(),
                                relation: Some(std::borrow::Cow::Borrowed("unrelated")),
                                weight: -1.0,
                                directed: false,
                                metadata: serde_json::json!({
                                    "reason": verdict.reason,
                                    "confidence": verdict.confidence,
                                    "source": "analysis"
                                }),
                            };
                            if let Err(e) = state.store.add_manual_edge(input) {
                                tracing::warn!(error=%e, target_id=%verdict.target_id, "failed to create anti-edge");
                            }
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
