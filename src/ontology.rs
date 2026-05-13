use crate::{
    api::{ChatCompletionRequest, EmbedderHandle, chat_completion_text},
    config::{OntologyConfig, OpenAiChatConfig},
    db::{GraphEdgeRecord, ItemRecord, ManualEdgeInput, VectorStore},
};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{borrow::Cow, collections::HashSet, sync::Arc};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

/// One-shot run report returned by [`run_once`] / [`run_for_item`].
/// Surfaced via the admin endpoints so a caller can verify the worker
/// without tailing logs.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OntologyRunReport {
    /// Items the worker pulled and ran the LLM on.
    pub items_processed: usize,
    /// Items that had no neighbors (LLM skipped — nothing to compare against).
    pub items_skipped_no_neighbors: usize,
    /// Edges actually written to the graph table.
    pub edges_committed: Vec<CommittedEdge>,
    /// LLM input-token estimate per item, for context-window debugging.
    pub estimated_input_tokens_per_item: usize,
    /// Per-item LLM traces: raw model output + filter-reason breakdown.
    /// Populated regardless of whether any edges were committed, so a
    /// "0 edges committed" run still surfaces why.
    pub debug: Vec<ItemDebug>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemDebug {
    pub item_id: String,
    /// Number of neighbors fed to the LLM (max = `RAG_ONTOLOGY_NEIGHBOR_COUNT`).
    pub neighbors: usize,
    /// Neighbor ids — handy when the model hallucinates ids that don't match these.
    pub neighbor_ids: Vec<String>,
    /// Names of predicates seeded for this item's source_id. Empty list ⇒
    /// every edge will be filtered (no valid predicates).
    pub valid_predicates: Vec<String>,
    /// Raw JSON the LLM emitted, truncated to ~2 KB. `None` if the call
    /// didn't reach the parse stage (HTTP error, empty body, etc.).
    pub raw_llm_output: Option<String>,
    /// Edges the model proposed before filtering. `None` when the LLM call
    /// itself failed (see `error`).
    pub proposed_edges: Option<usize>,
    pub filter_drops: FilterDrops,
    /// Set when the LLM call or parse failed entirely.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FilterDrops {
    /// Predicate not in the schema table for this source_id.
    pub bad_predicate: usize,
    /// `from_id` or `to_id` not in the candidates + target set.
    pub unknown_id: usize,
    /// Edge doesn't include the target as one endpoint.
    pub target_not_involved: usize,
    /// `from_id == to_id`.
    pub self_loop: usize,
    /// `confidence < threshold`.
    pub below_threshold: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommittedEdge {
    pub item_id: String,
    pub from_id: String,
    pub to_id: String,
    pub predicate: String,
    pub confidence: f32,
    pub status: String,
    pub reasoning: Option<String>,
}

fn committed_edge(item_id: &str, record: &GraphEdgeRecord) -> CommittedEdge {
    let status = record
        .metadata
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("suggested")
        .to_owned();
    let reasoning = record
        .metadata
        .get("reasoning")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    CommittedEdge {
        item_id: item_id.to_owned(),
        from_id: record.from_item_id.clone(),
        to_id: record.to_item_id.clone(),
        predicate: record.relation.clone().unwrap_or_default(),
        confidence: record.weight,
        status,
        reasoning,
    }
}


fn get_ontology_system_prompt(predicates: &[crate::db::OntologyPredicateRecord]) -> String {
    // Flat bullet list keyed by name + description from the DB. Small models
    // lose alignment in markdown tables, so we deliberately avoid them.
    let mut relations = String::new();
    for p in predicates {
        relations.push_str(&format!("- {}: {}\n", p.name, p.description));
    }

    format!(
        r#"You connect a TARGET document to nearby CANDIDATE documents using relations.
A HUMAN will review every edge you emit and reject the bad ones, so PROPOSE
PLAUSIBLE RELATIONS even when you're not 100% certain. Recall matters more
than precision — a missing edge is worse than a rejected one.

Allowed relations (use these exact names, nothing else) — read as `from RELATION to`:
{}
"Same topic" / "related work" / "both mention X" is still NOT a relation — those
add no information. But if a relation is plausibly there, emit it.

Output one line of JSON, nothing else:
{{"edges":[{{"from":"<id>","rel":"<relation>","to":"<id>","conf":<0.5-1.0>}}]}}

If no relations apply at all: {{"edges":[]}}

Confidence guide:
- 0.9+ : text explicitly states the relation
- 0.7-0.9 : strongly implied
- 0.5-0.7 : plausible but inferred (still emit — the human will judge)

Rules:
- One endpoint must be the TARGET id.
- Use ids verbatim from the input. Never invent."#,
        relations
    )
}

/// Edge the LLM emits. Field names use the short form documented in the
/// system prompt (`from`/`rel`/`to`/`conf`). Serde aliases keep us tolerant
/// of the legacy `from_id`/`predicate`/`to_id`/`confidence` names in case
/// a model parrots an old prompt or a cached response sneaks through.
/// `reasoning` is no longer requested but accepted if a model volunteers one.
#[derive(Debug, Deserialize)]
struct OntologyEdge {
    #[serde(alias = "from_id")]
    from: String,
    #[serde(alias = "predicate")]
    rel: String,
    #[serde(alias = "to_id")]
    to: String,
    #[serde(default, alias = "confidence")]
    conf: f32,
    #[serde(default)]
    reasoning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OntologyResponse {
    edges: Vec<OntologyEdge>,
}

// (chat envelope parsing now lives in api::analysis::chat_completion_text)

/// Token-budget estimate per LLM call. Surfaced in the worker startup log
/// and in run reports so context-window issues are visible at a glance.
fn estimate_input_tokens(cfg: &OntologyConfig) -> usize {
    700 + cfg.target_preview_chars / 4 + cfg.neighbor_count * (cfg.candidate_preview_chars / 4)
}

pub async fn run_ontology_worker(
    store: Arc<dyn VectorStore>,
    embedder: Arc<EmbedderHandle>,
    http_client: Client,
    openai: OpenAiChatConfig,
    ontology_cfg: OntologyConfig,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    if !openai.is_configured() {
        info!("ontology worker: disabled (no LLM base URL configured)");
        return;
    }
    let Some(model) = openai.default_model.clone() else {
        warn!("ontology worker: disabled (no LLM model configured)");
        return;
    };

    let estimated_input_tokens = estimate_input_tokens(&ontology_cfg);
    info!(
        "ontology worker: starting — interval={}s batch={} neighbors={} threshold={} \
         ~{estimated_input_tokens} input tokens/call (set RUST_LOG=rust_rag::ontology=debug for per-item logs)",
        ontology_cfg.interval_secs,
        ontology_cfg.batch_size,
        ontology_cfg.neighbor_count,
        ontology_cfg.confidence_threshold,
    );

    let mut ticker = interval(Duration::from_secs(ontology_cfg.interval_secs));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match run_once(&store, &embedder, &http_client, &openai, &model, &ontology_cfg).await {
                    Ok(report) if report.items_processed == 0 => {} // idle
                    Ok(report) => info!(
                        "ontology worker: tick — processed={} skipped_no_neighbors={} edges_committed={}",
                        report.items_processed,
                        report.items_skipped_no_neighbors,
                        report.edges_committed.len()
                    ),
                    Err(err) => error!("ontology worker: batch error: {err}"),
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("ontology worker: shutdown signal received, exiting loop");
                    break;
                }
            }
        }
    }
}

/// Run one pass over items the store marks as pending. Same logic as the
/// worker tick — exposed so admin endpoints can force a run for verification.
pub async fn run_once(
    store: &Arc<dyn VectorStore>,
    embedder: &Arc<EmbedderHandle>,
    http_client: &Client,
    openai: &OpenAiChatConfig,
    model: &str,
    cfg: &OntologyConfig,
) -> Result<OntologyRunReport> {
    let embedder_svc = embedder
        .get_ready()
        .map_err(|e| anyhow!("embedder not ready: {e}"))?;

    let store_clone = store.clone();
    let batch_size = cfg.batch_size;
    let pending =
        tokio::task::spawn_blocking(move || store_clone.get_items_pending_ontology(batch_size))
            .await??;

    let mut report = OntologyRunReport {
        estimated_input_tokens_per_item: estimate_input_tokens(cfg),
        ..Default::default()
    };

    if pending.is_empty() {
        return Ok(report);
    }

    info!("ontology worker: processing {} item(s)", pending.len());

    for item in pending {
        let mark_status = match process_item(
            store,
            embedder_svc.clone(),
            http_client,
            openai,
            model,
            cfg,
            &item,
            &mut report,
        )
        .await
        {
            Ok(()) => "done",
            Err(err) => {
                error!("ontology worker: item {} extraction failed: {err}", item.id);
                "failed"
            }
        };

        let store_clone = store.clone();
        let id = item.id.clone();
        let status_to_set = mark_status.to_string();
        let _ =
            tokio::task::spawn_blocking(move || store_clone.mark_ontology_status(&id, &status_to_set))
                .await;
        info!("ontology worker: marked item {} as {}", item.id, mark_status);
    }

    Ok(report)
}

/// Force-process a single item by id, ignoring its `ontology_status`. Useful
/// for re-running ontology extraction from the UI after editing an entry or
/// after enabling the worker for the first time.
pub async fn run_for_item(
    store: &Arc<dyn VectorStore>,
    embedder: &Arc<EmbedderHandle>,
    http_client: &Client,
    openai: &OpenAiChatConfig,
    model: &str,
    cfg: &OntologyConfig,
    item_id: &str,
) -> Result<OntologyRunReport> {
    let embedder_svc = embedder
        .get_ready()
        .map_err(|e| anyhow!("embedder not ready: {e}"))?;

    let store_clone = store.clone();
    let id_owned = item_id.to_owned();
    let item = tokio::task::spawn_blocking(move || store_clone.get_item(&id_owned))
        .await??
        .ok_or_else(|| anyhow!("item not found: {item_id}"))?;

    let mut report = OntologyRunReport {
        estimated_input_tokens_per_item: estimate_input_tokens(cfg),
        ..Default::default()
    };

    let process_result = process_item(
        store,
        embedder_svc,
        http_client,
        openai,
        model,
        cfg,
        &item,
        &mut report,
    )
    .await;

    let mark_status = match &process_result {
        Ok(()) => "done",
        Err(err) => {
            error!("ontology worker: item {} extraction failed: {err:#}", item.id);
            "failed"
        }
    };

    let store_clone = store.clone();
    let id = item.id.clone();
    let status_to_set = mark_status.to_string();
    let _ = tokio::task::spawn_blocking(move || store_clone.mark_ontology_status(&id, &status_to_set))
        .await;

    // Bubble the underlying error up so admin callers see the actual cause
    // (e.g. "LLM returned empty body") instead of a generic wrapper.
    process_result?;
    Ok(report)
}

async fn process_item(
    store: &Arc<dyn VectorStore>,
    embedder_svc: Arc<dyn crate::embedding::EmbeddingService>,
    http_client: &Client,
    openai: &OpenAiChatConfig,
    model: &str,
    cfg: &OntologyConfig,
    item: &ItemRecord,
    report: &mut OntologyRunReport,
) -> Result<()> {
    let source_id = item.source_id.clone();
    let store_clone = store.clone();
    let predicates =
        tokio::task::spawn_blocking(move || store_clone.list_ontology_predicates(Some(&source_id)))
            .await??;

    let text = item.text.clone();
    let item_id = item.id.clone();
    let store_clone = store.clone();
    let svc = embedder_svc;
    let neighbor_count = cfg.neighbor_count;
    let neighbors = tokio::task::spawn_blocking(move || -> Result<Vec<crate::db::SearchHit>> {
        let embedding = svc.embed(&text)?;
        let hits = store_clone.search(&embedding, neighbor_count + 1, None, None)?;
        Ok(hits
            .into_iter()
            .filter(|h| h.id != item_id)
            .take(neighbor_count)
            .collect())
    })
    .await??;

    let neighbor_ids: Vec<String> = neighbors.iter().map(|n| n.id.clone()).collect();
    let valid_predicates: Vec<String> = predicates.iter().map(|p| p.name.clone()).collect();

    if neighbors.is_empty() {
        report.items_skipped_no_neighbors += 1;
        report.debug.push(ItemDebug {
            item_id: item.id.clone(),
            neighbors: 0,
            neighbor_ids,
            valid_predicates,
            raw_llm_output: None,
            proposed_edges: None,
            filter_drops: FilterDrops::default(),
            error: Some("no neighbors".to_owned()),
        });
        info!("ontology worker: item {} → no neighbors, skipped", item.id);
        return Ok(());
    }

    let call = call_llm_for_edges(
        http_client,
        openai,
        model,
        item,
        &neighbors,
        &predicates,
        cfg.confidence_threshold,
        cfg.target_preview_chars,
        cfg.candidate_preview_chars,
    )
    .await;

    let LlmCallResult { parsed, raw_content } = match call {
        Ok(r) => r,
        Err(err) => {
            // Capture the failure in the debug trace so the UI can surface it
            // even when the LLM call itself blew up.
            report.debug.push(ItemDebug {
                item_id: item.id.clone(),
                neighbors: neighbors.len(),
                neighbor_ids,
                valid_predicates,
                raw_llm_output: None,
                proposed_edges: None,
                filter_drops: FilterDrops::default(),
                error: Some(err.to_string()),
            });
            return Err(err);
        }
    };

    report.items_processed += 1;
    let ParsedEdges { edges, proposed, drops } = parsed;
    let count = edges.len();
    for edge in edges {
        let store_clone = store.clone();
        match tokio::task::spawn_blocking(move || store_clone.add_manual_edge(edge)).await {
            Err(err) => error!("ontology worker: failed to insert edge: {err}"),
            Ok(Err(err)) => error!("ontology worker: failed to insert edge: {err}"),
            Ok(Ok(record)) => {
                info!(
                    "ontology worker: added edge: {} --[{}]--> {} (conf: {:.2})",
                    record.from_item_id,
                    record.relation.as_deref().unwrap_or("none"),
                    record.to_item_id,
                    record.weight
                );
                report.edges_committed.push(committed_edge(&item.id, &record));
            }
        }
    }
    if count > 0 {
        info!("ontology worker: item {} → {count} edge(s) committed", item.id);
    } else {
        info!(
            item_id = %item.id,
            proposed = proposed,
            bad_predicate = drops.bad_predicate,
            unknown_id = drops.unknown_id,
            target_not_involved = drops.target_not_involved,
            self_loop = drops.self_loop,
            below_threshold = drops.below_threshold,
            "ontology worker: item → no edges committed (all filtered or empty)"
        );
    }

    report.debug.push(ItemDebug {
        item_id: item.id.clone(),
        neighbors: neighbors.len(),
        neighbor_ids,
        valid_predicates,
        raw_llm_output: Some(truncate(&raw_content, 2000)),
        proposed_edges: Some(proposed),
        filter_drops: drops,
        error: None,
    });

    Ok(())
}

pub struct LlmCallResult {
    pub parsed: ParsedEdges,
    /// Raw assistant content the model emitted (already extracted from the
    /// chat envelope, before code-fence stripping). Useful for debugging.
    pub raw_content: String,
}

async fn call_llm_for_edges(
    http_client: &Client,
    openai: &OpenAiChatConfig,
    model: &str,
    target: &crate::db::ItemRecord,
    neighbors: &[crate::db::SearchHit],
    predicates: &[crate::db::OntologyPredicateRecord],
    confidence_threshold: f32,
    target_preview_chars: usize,
    candidate_preview_chars: usize,
) -> Result<LlmCallResult> {
    if neighbors.is_empty() {
        return Ok(LlmCallResult {
            parsed: ParsedEdges { edges: vec![], proposed: 0, drops: FilterDrops::default() },
            raw_content: String::new(),
        });
    }

    let valid_predicates: HashSet<String> = predicates.iter().map(|p| p.name.clone()).collect();
    let system_prompt = get_ontology_system_prompt(predicates);

    // Plain-text delimited blocks. Empirically gives a 4B model much higher
    // schema compliance than JSON-on-bullets, because each candidate has
    // clear visual boundaries. Drops type/tags — they add noise without
    // affecting relation choice in practice.
    let target_preview: String = target.text.chars().take(target_preview_chars).collect();
    let mut user_message = format!(
        "TARGET id={}\n{}\n\nCANDIDATES:\n\n",
        target.id, target_preview
    );
    for (i, n) in neighbors.iter().enumerate() {
        let preview: String = n.text.chars().take(candidate_preview_chars).collect();
        user_message.push_str(&format!("[{}] id={}\n{}\n\n", i + 1, n.id, preview));
    }

    tracing::debug!(
        item_id = target.id,
        neighbors = neighbors.len(),
        target_chars = target_preview.len(),
        "ontology worker: calling LLM"
    );
    // Dump the full prompts at info so the operator can read what's being
    // sent. Cheap: one ontology call per item, not in a hot loop.
    info!(
        item_id = %target.id,
        system_prompt = %system_prompt,
        user_prompt = %user_message,
        "ontology worker: LLM prompts"
    );

    // Delegate the HTTP/envelope/thinking-mode plumbing to the same helper
    // the analysis endpoint uses — keeps both paths on the well-tested code.
    let base_url = openai
        .base_url
        .as_deref()
        .expect("is_configured() already checked");
    let content = chat_completion_text(
        http_client,
        ChatCompletionRequest {
            base_url,
            api_key: openai.api_key.as_deref(),
            model,
            timeout_secs: openai.timeout_secs,
            system_prompt: &system_prompt,
            user_prompt: &user_message,
            max_tokens: 2048,
            temperature: 0.0,
            // No response_format: many llama.cpp builds reject json_object
            // with non-OpenAI models. We strip code fences in extract_json.
            response_format_json: false,
        },
    )
    .await?;

    tracing::debug!(
        item_id = target.id,
        raw_response = %content,
        "ontology worker: LLM response"
    );

    let parsed = parse_ontology_response(
        &content,
        &target.id,
        neighbors,
        &valid_predicates,
        confidence_threshold,
    )
    .map_err(|e| {
        // Small models (4B) frequently emit malformed JSON. Log the raw
        // content so the operator can see exactly what came back.
        warn!(
            item_id = %target.id,
            error = %e,
            raw_content = %truncate(&content, 1000),
            "ontology worker: LLM content failed schema parse"
        );
        anyhow!("LLM output failed schema parse ({e}); content preview: {}", truncate(&content, 200))
    })?;

    Ok(LlmCallResult { parsed, raw_content: content })
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.replace('\n', "\\n")
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out.replace('\n', "\\n")
    }
}

pub struct ParsedEdges {
    pub edges: Vec<ManualEdgeInput>,
    pub proposed: usize,
    pub drops: FilterDrops,
}

fn parse_ontology_response(
    content: &str,
    item_id: &str,
    neighbors: &[crate::db::SearchHit],
    valid_predicates: &HashSet<String>,
    confidence_threshold: f32,
) -> Result<ParsedEdges> {
    let json_str = extract_json(content);
    let ontology: OntologyResponse = serde_json::from_str(json_str)?;

    let valid_ids: HashSet<&str> = neighbors
        .iter()
        .map(|n| n.id.as_str())
        .chain(std::iter::once(item_id))
        .collect();

    let mut drops = FilterDrops::default();
    let proposed = ontology.edges.len();
    let mut edges = Vec::with_capacity(proposed);

    for e in ontology.edges {
        // Evaluate in priority order — first failed check wins the count
        // (otherwise the totals over-report when an edge has multiple issues).
        if !valid_predicates.contains(&e.rel) {
            drops.bad_predicate += 1;
            continue;
        }
        if !valid_ids.contains(e.from.as_str())
            || !valid_ids.contains(e.to.as_str())
        {
            drops.unknown_id += 1;
            continue;
        }
        if e.from != item_id && e.to != item_id {
            drops.target_not_involved += 1;
            continue;
        }
        if e.from == e.to {
            drops.self_loop += 1;
            continue;
        }
        if e.conf < confidence_threshold {
            drops.below_threshold += 1;
            continue;
        }

        // Auto-confirm bar is high (0.95) so almost everything routes through
        // the HITL review UI. A 4B model rarely emits >=0.95 on its own, so in
        // practice the only auto-confirmed edges are the ones the model is
        // genuinely certain about. Adjust upward if false-positives slip through.
        let status = if e.conf >= 0.95 { "confirmed" } else { "suggested" };
        edges.push(ManualEdgeInput {
            from_item_id: e.from,
            to_item_id: e.to,
            relation: Some(Cow::Owned(e.rel)),
            // Confidence becomes the edge weight — queryable and visible in the graph UI.
            weight: e.conf,
            directed: true,
            metadata: json!({
                "source": "ontology_worker",
                "confidence": e.conf,
                "reasoning": e.reasoning,
                "status": status
            }),
        });
    }

    Ok(ParsedEdges { edges, proposed, drops })
}

/// Strip optional markdown code fences so the JSON can be parsed even when a
/// model ignores the "no markdown" instruction.
fn extract_json(content: &str) -> &str {
    let trimmed = content.trim();
    if let Some(inner) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        inner.trim_end_matches("```").trim()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SearchHit;
    use serde_json::json;

    fn hit(id: &str, text: &str) -> SearchHit {
        SearchHit {
            id: id.to_owned(),
            text: text.to_owned(),
            metadata: json!({}),
            source_id: "test".to_owned(),
            created_at: 0,
            updated_at: 0,
            distance: 0.1,
            section_path: Vec::new(),
            retrievers: Vec::new(),
            chunk_text: None,
            path: None,
            type_name: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn valid_edges_are_accepted() {
        let neighbors = vec![hit("id-B", "data structure"), hit("id-C", "hashing")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.95,"reasoning":"HashMap is a data structure."},{"from_id":"id-A","predicate":"depends_on","to_id":"id-C","confidence":0.88,"reasoning":"HashMap depends on hashing."}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        valid.insert("depends_on".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].relation.as_ref().map(|r| r.as_ref()), Some("is_a"));
        assert!((edges[0].weight - 0.95).abs() < 0.001);
        assert!(edges[0].directed);
        assert_eq!(edges[0].metadata["status"], "confirmed");
        assert_eq!(edges[0].metadata["reasoning"], "HashMap is a data structure.");
        assert_eq!(edges[1].metadata["status"], "suggested");
        assert_eq!(edges[1].metadata["reasoning"], "HashMap depends on hashing.");
    }

    #[test]
    fn edge_below_confidence_threshold_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.5}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        // threshold 0.7 — 0.5 should be dropped
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert!(edges.is_empty());
    }

    #[test]
    fn edge_at_confidence_threshold_is_accepted() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.7}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn missing_confidence_field_uses_zero_and_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        // confidence defaults to 0.0 when missing — below any sensible threshold
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B"}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert!(edges.is_empty());
    }

    #[test]
    fn invalid_predicate_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"related_to","to_id":"id-B","confidence":0.95}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert!(edges.is_empty());
    }

    #[test]
    fn new_predicates_are_valid() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"contains","to_id":"id-B","confidence":0.9},{"from_id":"id-B","predicate":"implemented_by","to_id":"id-A","confidence":0.85}]}"#;
        let mut valid = HashSet::new();
        valid.insert("contains".to_owned());
        valid.insert("implemented_by".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn unknown_id_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-PHANTOM","confidence":0.9}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert!(edges.is_empty());
    }

    #[test]
    fn edge_not_involving_target_is_dropped() {
        let neighbors = vec![hit("id-B", "b"), hit("id-C", "c")];
        let content =
            r#"{"edges":[{"from_id":"id-B","predicate":"is_a","to_id":"id-C","confidence":0.9}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert!(edges.is_empty());
    }

    #[test]
    fn markdown_fence_is_stripped() {
        let neighbors = vec![hit("id-B", "data structure")];
        let content = "```json\n{\"edges\":[{\"from_id\":\"id-A\",\"predicate\":\"is_a\",\"to_id\":\"id-B\",\"confidence\":0.9}]}\n```";
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn empty_edges_array_is_ok() {
        let neighbors = vec![hit("id-B", "unrelated")];
        let valid = HashSet::new();
        let edges = parse_ontology_response(r#"{"edges":[]}"#, "id-A", &neighbors, &valid, 0.7).unwrap().edges;
        assert!(edges.is_empty());
    }
}
