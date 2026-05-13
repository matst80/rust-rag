use crate::{
    api::EmbedderHandle,
    config::{OntologyConfig, OpenAiChatConfig},
    db::{ManualEdgeInput, VectorStore},
};
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::{borrow::Cow, collections::HashSet, sync::Arc};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};


fn get_ontology_system_prompt(predicates: &[crate::db::OntologyPredicateRecord]) -> String {
    let mut schema_table = String::from("| Predicate | Direction (from → to) | Example |\n|-----------|-----------------------|---------|\n");
    for p in predicates {
        let example = match (&p.example_from, &p.example_to) {
            (Some(f), Some(t)) => format!("\"{}\" {} \"{}\"", f, p.name, t),
            _ => "...".to_owned(),
        };
        schema_table.push_str(&format!(
            "| {} | {} | {} |\n",
            p.name,
            p.direction,
            example
        ));
    }
    
    format!(r#"### Role
You are an Ontology Edge Detector for a knowledge graph. Given a TARGET document and CANDIDATE documents, identify directed semantic relationships between them using a fixed schema.

### Relationship Schema (EXHAUSTIVE — use NO other predicates)
{}

### Rules
1. Direction matters: `from_id PREDICATE to_id` means the relationship holds in that direction.
2. Only create an edge if the relationship is EXPLICIT or STRONGLY implied. When in doubt, omit.
3. If no predicate from the schema fits a candidate, skip that candidate entirely.
4. Every edge must involve the TARGET as either from_id or to_id.
5. Only use IDs that appear verbatim in the input — never invent IDs.
6. Assign a confidence score (0.0–1.0) to each edge:
   - 0.9–1.0: explicit, unambiguous statement in the text
   - 0.7–0.9: strongly implied
   - 0.5–0.7: reasonably inferred but not stated
   - below 0.5: do not include the edge
7. Provide a one-sentence `reasoning` for each edge explaining why the relationship exists.
8. Output ONLY the JSON object below. No prose, no markdown, no code fences.

### Output Schema
{{"edges":[{{"from_id":"<id>","predicate":"<predicate>","to_id":"<id>","confidence":<0.0-1.0>,"reasoning":"<one sentence description>"}}]}}
When no relationships apply: {{"edges":[]}}

### Example
TARGET: {{"id":"id-A","text":"A HashMap is a hash-table-based associative data structure providing O(1) average-case lookup.","type":"fact","tags":["rust","data-structures"],"source_id":"knowledge"}}
CANDIDATES:
- {{"id":"id-B","text":"A data structure is an abstraction for organizing data in memory to enable efficient operations.","type":"concept","tags":["cs-basics"],"source_id":"knowledge"}}
- {{"id":"id-C","text":"Hashing maps variable-length data to a fixed-size integer index via a hash function.","type":"fact","tags":["algorithms"],"source_id":"knowledge"}}

OUTPUT: {{"edges":[{{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.97,"reasoning":"HashMap is explicitly described as a data structure."}},{{"from_id":"id-A","predicate":"depends_on","to_id":"id-C","confidence":0.91,"reasoning":"HashMap uses hashing as its underlying mechanism for lookups."}}]}}
"#, schema_table)
}

#[derive(Debug, Deserialize)]
struct OntologyEdge {
    from_id: String,
    predicate: String,
    to_id: String,
    #[serde(default)]
    confidence: f32,
    #[serde(default)]
    reasoning: Option<String>,
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
    store: Arc<dyn VectorStore>,
    embedder: Arc<EmbedderHandle>,
    http_client: Client,
    openai: OpenAiChatConfig,
    ontology_cfg: OntologyConfig,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
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
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) =
                    process_batch(&store, &embedder, &http_client, &openai, &model, &ontology_cfg).await
                {
                    error!("ontology worker: batch error: {err}");
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

async fn process_batch(
    store: &Arc<dyn VectorStore>,
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
        let source_id = item.source_id.clone();
        let store_clone = store.clone();

        let predicates = tokio::task::spawn_blocking(move || store_clone.list_ontology_predicates(Some(&source_id)))
            .await??;
            
        let store_clone = store.clone();
        let svc = embedder_svc.clone();

        let neighbor_count = cfg.neighbor_count;
        let neighbors =
            tokio::task::spawn_blocking(move || -> Result<Vec<crate::db::SearchHit>> {
                let embedding = svc.embed(&text)?;
                let hits = store_clone.search(&embedding, neighbor_count + 1, None, None)?;
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
            &item,
            &neighbors,
            &predicates,
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
                    match tokio::task::spawn_blocking(move || store_clone.add_manual_edge(edge))
                        .await
                    {
                        Err(err) => {
                            error!("ontology worker: failed to insert edge: {err}");
                        }
                        Ok(Err(err)) => {
                            error!("ontology worker: failed to insert edge: {err}");
                        }
                        Ok(Ok(record)) => {
                            info!(
                                "ontology worker: added edge: {} --[{}]--> {} (conf: {:.2})",
                                record.from_item_id,
                                record.relation.as_deref().unwrap_or("none"),
                                record.to_item_id,
                                record.weight
                            );
                        }
                    }
                }
                if count > 0 {
                    info!("ontology worker: item {} → {count} edge(s) committed", item.id);
                } else {
                    info!("ontology worker: item {} → no edges (all filtered or empty)", item.id);
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
        let status_to_set = status.to_string();
        let _ =
            tokio::task::spawn_blocking(move || store_clone.mark_ontology_status(&id, &status_to_set))
                .await;
        info!("ontology worker: marked item {} as {}", item.id, status);
    }

    Ok(())
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
) -> Result<Vec<ManualEdgeInput>> {
    if neighbors.is_empty() {
        return Ok(vec![]);
    }

    let valid_predicates: HashSet<String> = predicates.iter().map(|p| p.name.clone()).collect();
    let system_prompt = get_ontology_system_prompt(predicates);

    let target_preview: String = target.text.chars().take(target_preview_chars).collect();
    let target_tags: Vec<String> = target.metadata.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let mut candidates = String::new();
    for n in neighbors {
        let preview: String = n.text.chars().take(candidate_preview_chars).collect();
        candidates.push_str(&format!(
            "- {}\n",
            json!({
                "id": n.id,
                "text": preview,
                "type": n.type_name,
                "tags": n.tags,
                "source_id": n.source_id,
            })
        ));
    }

    let user_message = format!(
        "TARGET: {}\n\nCANDIDATES:\n{}",
        json!({
            "id": target.id,
            "text": target_preview,
            "type": target.type_name,
            "tags": target_tags,
            "source_id": target.source_id,
        }),
        candidates,
    );

    tracing::debug!(
        item_id = target.id,
        neighbors = neighbors.len(),
        target_chars = target_preview.len(),
        "ontology worker: calling LLM"
    );

    let payload = json!({
        "model": model,
        "temperature": 0,
        "max_tokens": (1024.0 * 1.5) as usize, // generous limit to avoid truncation issues (will be cut off by OpenAI if it exceeds the model's context window)
        "messages": [
            {"role": "system", "content": system_prompt},
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

    tracing::debug!(item_id = target.id, raw_response = %content, "ontology worker: LLM response");

    parse_ontology_response(&content, &target.id, neighbors, &valid_predicates, confidence_threshold)
}

fn parse_ontology_response(
    content: &str,
    item_id: &str,
    neighbors: &[crate::db::SearchHit],
    valid_predicates: &HashSet<String>,
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
            valid_predicates.contains(&e.predicate)
                && valid_ids.contains(e.from_id.as_str())
                && valid_ids.contains(e.to_id.as_str())
                && (e.from_id == item_id || e.to_id == item_id)
                && e.from_id != e.to_id
                && e.confidence >= confidence_threshold
        })
        .map(|e| {
            let relation = Cow::Owned(e.predicate);

            let status = if e.confidence >= 0.9 {
                "confirmed"
            } else {
                "suggested"
            };

            ManualEdgeInput {
                from_item_id: e.from_id,
                to_item_id: e.to_id,
                relation: Some(relation),
                // Confidence becomes the edge weight — queryable and visible in the graph UI.
                weight: e.confidence,
                directed: true,
                metadata: json!({
                    "source": "ontology_worker",
                    "confidence": e.confidence,
                    "reasoning": e.reasoning,
                    "status": status
                }),
            }
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
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
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
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn edge_at_confidence_threshold_is_accepted() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B","confidence":0.7}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn missing_confidence_field_uses_zero_and_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        // confidence defaults to 0.0 when missing — below any sensible threshold
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-B"}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn invalid_predicate_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"related_to","to_id":"id-B","confidence":0.95}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn new_predicates_are_valid() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"contains","to_id":"id-B","confidence":0.9},{"from_id":"id-B","predicate":"implemented_by","to_id":"id-A","confidence":0.85}]}"#;
        let mut valid = HashSet::new();
        valid.insert("contains".to_owned());
        valid.insert("implemented_by".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn unknown_id_is_dropped() {
        let neighbors = vec![hit("id-B", "something")];
        let content = r#"{"edges":[{"from_id":"id-A","predicate":"is_a","to_id":"id-PHANTOM","confidence":0.9}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn edge_not_involving_target_is_dropped() {
        let neighbors = vec![hit("id-B", "b"), hit("id-C", "c")];
        let content =
            r#"{"edges":[{"from_id":"id-B","predicate":"is_a","to_id":"id-C","confidence":0.9}]}"#;
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn markdown_fence_is_stripped() {
        let neighbors = vec![hit("id-B", "data structure")];
        let content = "```json\n{\"edges\":[{\"from_id\":\"id-A\",\"predicate\":\"is_a\",\"to_id\":\"id-B\",\"confidence\":0.9}]}\n```";
        let mut valid = HashSet::new();
        valid.insert("is_a".to_owned());
        let edges = parse_ontology_response(content, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn empty_edges_array_is_ok() {
        let neighbors = vec![hit("id-B", "unrelated")];
        let valid = HashSet::new();
        let edges = parse_ontology_response(r#"{"edges":[]}"#, "id-A", &neighbors, &valid, 0.7).unwrap();
        assert!(edges.is_empty());
    }
}
