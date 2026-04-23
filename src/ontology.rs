use crate::{
    api::EmbedderHandle,
    config::{OntologyConfig, OpenAiChatConfig},
    db::{ManualEdgeInput, SqliteVectorStore, VectorStore},
};
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashSet, sync::Arc};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};


// Exhaustive predicate list — output containing anything else is discarded post-parse.
// Two predicates added over the original Gemini schema:
//   contains      — hierarchy within a document (article contains a fact/claim)
//   implemented_by — links a spec/concept to its concrete realization
const VALID_PREDICATES: &[&str] = &[
    "is_a",
    "part_of",
    "caused_by",
    "works_for",
    "contradicts",
    "depends_on",
    "contains",
    "implemented_by",
];

/// Design notes on determinism:
///
/// 1. **Item-ID grounded** — LLM works with concrete item IDs, not free-text entity names.
///    Eliminates "Apple" vs "Apple Inc." drift entirely.
///
/// 2. **Confidence per edge** — LLM rates each edge 0.0–1.0. Only edges above
///    `confidence_threshold` are committed; lower ones are silently dropped.
///    This is the key guard against low-quality automated edges.
///
/// 3. **Few-shot example** — shows exact input/output including the confidence field
///    and how to skip an irrelevant candidate. Biggest single driver of output consistency.
///
/// 4. **Explicit skip rule** — "if no predicate fits, omit the candidate" removes
///    pressure to invent `related_to`-style edges.
///
/// 5. **Direction table** — each predicate's from→to semantics are shown inline,
///    preventing the most common direction errors (reversed is_a etc.).
///
/// 6. **Hard JSON-only constraint** — combined with temperature=0 makes the output
///    fully machine-readable in a single parse attempt. `extract_json` handles
///    models that wrap output in markdown fences despite instructions.
const ONTOLOGY_SYSTEM_PROMPT: &str = r#"### Role
You are an Ontology Edge Detector for a knowledge graph. Given a TARGET document and CANDIDATE documents, identify directed semantic relationships between them using a fixed schema.

### Relationship Schema (EXHAUSTIVE — use NO other predicates)
| Predicate      | Direction (from → to)                    | Example                                                  |
|----------------|------------------------------------------|----------------------------------------------------------|
| is_a           | from is a subtype/instance of to         | "HashMap" is_a "data structure"                          |
| part_of        | from is a component of to                | "Wheel" part_of "Car"                                    |
| caused_by      | from is an effect/consequence of to      | "Smoke" caused_by "Fire"                                 |
| works_for      | entity in from is affiliated with to     | "Musk" works_for "Tesla"                                 |
| contradicts    | from makes a claim incompatible with to  | "Vaccines cause autism" contradicts "Vaccines are safe"  |
| depends_on     | from requires to to function/exist       | "TCP" depends_on "IP routing"                            |
| contains       | from includes/embeds to as a sub-element | "News article" contains "quoted statistic"               |
| implemented_by | to is the concrete realization of from   | "Auth spec" implemented_by "OAuth2 library"              |

### Rules
1. Direction matters: `from_id is_a to_id` means FROM is a subtype of TO, not the reverse.
2. Only create an edge if the relationship is EXPLICIT or STRONGLY implied. When in doubt, omit.
3. If no predicate from the schema fits a candidate, skip that candidate entirely.
4. Every edge must involve the TARGET as either from_id or to_id.
5. Only use IDs that appear verbatim in the input — never invent IDs.
6. Assign a confidence score (0.0–1.0) to each edge:
   - 0.9–1.0: explicit, unambiguous statement in the text
   - 0.7–0.9: strongly implied
   - 0.5–0.7: reasonably inferred but not stated
   - below 0.5: do not include the edge
7. Output ONLY the JSON object below. No prose, no markdown, no code fences.

### Output Schema
{"edges":[{"from_id":"<id>","predicate":"<predicate>","to_id":"<id>","confidence":<0.0-1.0>}]}
When no relationships apply: {"edges":[]}

### Example
TARGET: {"id":"id-A","text":"A HashMap is a hash-table-based associative data structure providing O(1) average-case lookup."}
CANDIDATES:
- {"id":"id-B","text":"A data structure is an abstraction for organizing data in memory to enable efficient operations."}
- {"id":"id-C","text":"Hashing maps variable-length data to a fixed-size integer index via a hash function."}
- {"id":"id-D","text":"A linked list stores elements in nodes where each node points to the next, giving O(n) lookup."}

OUTPUT: {"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.97},{"from_id":"id-A","predicate":"depends_on","to_id":"id-C","confidence":0.91}]}

Explanation (for your reference only — do not output): HashMap IS A data structure (is_a, explicit). HashMap DEPENDS ON hashing to work (depends_on, strongly implied). id-D is also a data structure but has no direct relationship to id-A worth encoding — skipped."#;

#[derive(Debug, Deserialize)]
struct OntologyEdge {
    from_id: String,
    predicate: String,
    to_id: String,
    #[serde(default)]
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct OntologyResponse {
    edges: Vec<OntologyEdge>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

pub async fn run_ontology_worker(
    store: Arc<SqliteVectorStore>,
    embedder: Arc<EmbedderHandle>,
    http_client: Client,
    openai: OpenAiChatConfig,
    ontology_cfg: OntologyConfig,
) {
    if !openai.is_configured() {
        info!("ontology worker: disabled (no OpenAI base URL configured)");
        return;
    }
    let Some(model) = openai.default_model.clone() else {
        warn!("ontology worker: disabled (RAG_OPENAI_MODEL not set)");
        return;
    };

    // Rough token budget estimate: system_prompt≈700 + target≈(target_chars/4) +
    // neighbors*(candidate_chars/4) + output≤512. Log it so context-window issues
    // are immediately visible. Set RUST_LOG=rust_rag::ontology=debug for per-item detail.
    let estimated_input_tokens = 700
        + ontology_cfg.target_preview_chars / 4
        + ontology_cfg.neighbor_count * (ontology_cfg.candidate_preview_chars / 4);
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
        ticker.tick().await;
        if let Err(err) =
            process_batch(&store, &embedder, &http_client, &openai, &model, &ontology_cfg).await
        {
            error!("ontology worker: batch error: {err}");
        }
    }
}

async fn process_batch(
    store: &Arc<SqliteVectorStore>,
    embedder: &Arc<EmbedderHandle>,
    http_client: &Client,
    openai: &OpenAiChatConfig,
    model: &str,
    cfg: &OntologyConfig,
) -> Result<()> {
    // Skip if embedder isn't ready yet — will retry on next tick.
    let embedder_svc = match embedder.get_ready() {
        Ok(svc) => svc,
        Err(_) => return Ok(()),
    };

    let store_clone = store.clone();
    let batch_size = cfg.batch_size;
    let pending =
        tokio::task::spawn_blocking(move || store_clone.get_items_pending_ontology(batch_size))
            .await??;

    if pending.is_empty() {
        return Ok(());
    }

    info!("ontology worker: processing {} item(s)", pending.len());

    for item in pending {
        let text = item.text.clone();
        let item_id = item.id.clone();
        let store_clone = store.clone();
        let svc = embedder_svc.clone();

        let neighbor_count = cfg.neighbor_count;
        let neighbors =
            tokio::task::spawn_blocking(move || -> Result<Vec<crate::db::SearchHit>> {
                let embedding = svc.embed(&text)?;
                let hits = store_clone.search(&embedding, neighbor_count + 1, None)?;
                Ok(hits
                    .into_iter()
                    .filter(|h| h.id != item_id)
                    .take(neighbor_count)
                    .collect())
            })
            .await??;

        let status = match call_llm_for_edges(
            http_client,
            openai,
            model,
            &item.id,
            &item.text,
            &neighbors,
            cfg.confidence_threshold,
            cfg.target_preview_chars,
            cfg.candidate_preview_chars,
        )
        .await
        {
            Ok(edges) => {
                let count = edges.len();
                for edge in edges {
                    let store_clone = store.clone();
                    if let Err(err) =
                        tokio::task::spawn_blocking(move || store_clone.add_manual_edge(edge))
                            .await
                    {
                        error!("ontology worker: failed to insert edge: {err}");
                    }
                }
                if count > 0 {
                    info!("ontology worker: item {} → {count} edge(s) committed", item.id);
                } else {
                    tracing::debug!("ontology worker: item {} → no edges (all filtered or empty)", item.id);
                }
                "done"
            }
            Err(err) => {
                error!("ontology worker: item {} extraction failed: {err}", item.id);
                "failed"
            }
        };

        let store_clone = store.clone();
        let id = item.id.clone();
        let _ =
            tokio::task::spawn_blocking(move || store_clone.mark_ontology_status(&id, status))
                .await;
    }

    Ok(())
}

async fn call_llm_for_edges(
    http_client: &Client,
    openai: &OpenAiChatConfig,
    model: &str,
    item_id: &str,
    item_text: &str,
    neighbors: &[crate::db::SearchHit],
    confidence_threshold: f32,
    target_preview_chars: usize,
    candidate_preview_chars: usize,
) -> Result<Vec<ManualEdgeInput>> {
    if neighbors.is_empty() {
        return Ok(vec![]);
    }

    let target_preview: String = item_text.chars().take(target_preview_chars).collect();
    let mut candidates = String::new();
    for n in neighbors {
        let preview: String = n.text.chars().take(candidate_preview_chars).collect();
        candidates.push_str(&format!(
            "- {{\"id\":{},\"text\":{}}}\n",
            serde_json::to_string(&n.id)?,
            serde_json::to_string(&preview)?,
        ));
    }

    let user_message = format!(
        "TARGET: {{\"id\":{},\"text\":{}}}\n\nCANDIDATES:\n{}",
        serde_json::to_string(item_id)?,
        serde_json::to_string(&target_preview)?,
        candidates,
    );

    tracing::debug!(
        item_id,
        neighbors = neighbors.len(),
        target_chars = target_preview.len(),
        "ontology worker: calling LLM"
    );

    let payload = json!({
        "model": model,
        "temperature": 0,
        "max_tokens": (1024.0 * 1.5) as usize, // generous limit to avoid truncation issues (will be cut off by OpenAI if it exceeds the model's context window)
        "messages": [
            {"role": "system", "content": ONTOLOGY_SYSTEM_PROMPT},
            {"role": "user", "content": user_message}
        ]
    });

    let base_url = openai
        .base_url
        .as_deref()
        .expect("is_configured() already checked");
    let mut req = http_client
        .post(format!(
            "{}/chat/completions",
            base_url.trim_end_matches('/')
        ))
        .json(&payload);
    if let Some(key) = openai.api_key.as_deref() {
        req = req.bearer_auth(key);
    }

    let response = req.send().await?.error_for_status()?;
    let chat: ChatCompletionResponse = response.json().await?;

    let content = chat
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();

    tracing::debug!(item_id, raw_response = %content, "ontology worker: LLM response");

    parse_ontology_response(&content, item_id, neighbors, confidence_threshold)
}

fn parse_ontology_response(
    content: &str,
    item_id: &str,
    neighbors: &[crate::db::SearchHit],
    confidence_threshold: f32,
) -> Result<Vec<ManualEdgeInput>> {
    let json_str = extract_json(content);
    let ontology: OntologyResponse = serde_json::from_str(json_str)?;

    let valid_ids: HashSet<&str> = neighbors
        .iter()
        .map(|n| n.id.as_str())
        .chain(std::iter::once(item_id))
        .collect();

    let edges = ontology
        .edges
        .into_iter()
        .filter(|e| {
            VALID_PREDICATES.contains(&e.predicate.as_str())
                && valid_ids.contains(e.from_id.as_str())
                && valid_ids.contains(e.to_id.as_str())
                && (e.from_id == item_id || e.to_id == item_id)
                && e.from_id != e.to_id
                && e.confidence >= confidence_threshold
        })
        .map(|e| ManualEdgeInput {
            from_item_id: e.from_id,
            to_item_id: e.to_id,
            relation: Some(e.predicate),
            // Confidence becomes the edge weight — queryable and visible in the graph UI.
            weight: e.confidence,
            directed: true,
            metadata: json!({
                "source": "ontology_worker",
                "confidence": e.confidence
            }),
        })
        .collect();

    Ok(edges)
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

pub fn ontology_system_prompt() -> &'static str {
    ONTOLOGY_SYSTEM_PROMPT
}

pub fn valid_predicates() -> &'static [&'static str] {
    VALID_PREDICATES
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
            distance: 0.1,
        }
    }

    #[test]
    fn valid_edges_are_accepted() {
        let neighbors = vec![hit("id-B", "data structure"), hit("id-C", "hashing")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.95},{"from_id":"id-A","predicate":"depends_on","to_id":"id-C","confidence":0.88}]}"#;
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].relation, Some("is_a".to_owned()));
        assert!((edges[0].weight - 0.95).abs() < 0.001);
        assert!(edges[0].directed);
    }

    #[test]
    fn edge_below_confidence_threshold_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.5}]}"#;
        // threshold 0.7 — 0.5 should be dropped
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn edge_at_confidence_threshold_is_accepted() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.7}]}"#;
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn missing_confidence_field_uses_zero_and_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        // confidence defaults to 0.0 when missing — below any sensible threshold
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B"}]}"#;
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn invalid_predicate_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"related_to","to_id":"id-B","confidence":0.95}]}"#;
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn new_predicates_are_valid() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"contains","to_id":"id-B","confidence":0.9},{"from_id":"id-B","predicate":"implemented_by","to_id":"id-A","confidence":0.85}]}"#;
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn unknown_id_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-PHANTOM","confidence":0.9}]}"#;
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn edge_not_involving_target_is_dropped() {
        let neighbors = vec![hit("id-B", "b"), hit("id-C", "c")];
        let content =
            r#"{"edges":[{"from_id":"id-B","predicate":"is_a","to_id":"id-C","confidence":0.9}]}"#;
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn markdown_fence_is_stripped() {
        let neighbors = vec![hit("id-B", "data structure")];
        let content = "```json\n{\"edges\":[{\"from_id\":\"id-A\",\"predicate\":\"is_a\",\"to_id\":\"id-B\",\"confidence\":0.9}]}\n```";
        let edges = parse_ontology_response(content, "id-A", &neighbors, 0.7).unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn empty_edges_array_is_ok() {
        let neighbors = vec![hit("id-B", "unrelated")];
        let edges = parse_ontology_response(r#"{"edges":[]}"#, "id-A", &neighbors, 0.7).unwrap();
        assert!(edges.is_empty());
    }
}
