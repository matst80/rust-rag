use crate::{
    api::{AppState, analysis::call_llm},
    db::{ItemRecord, SearchHit},
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn, debug};

const DREAMING_SYSTEM_PROMPT: &str = r#"You are the Memory Consolidation Engine for a RAG system.
Your task is to review entries in the 'memory' (short-term) source and decide which should be promoted to 'knowledge' (long-term), merged with others, or pruned.

ACTIONS:
- promote: This is a durable fact, a finalized decision, or an important architectural note that belongs in the permanent knowledge base.
- merge: This entry is related to another entry (short-term or long-term) and should be combined into a single, better entry.
- prune: This is a transient note, a duplicate that adds no value, or a resolved todo that is no longer needed.
- keep: This is still relevant short-term memory that isn't ready for promotion or merging yet.

HEURISTICS:
- Promotion happens when a task is completed AND the outcome is worth saving, or when a temporary note contains a fact that should be permanent.
- Merging happens when multiple notes cover the same topic; provide a 'consolidated_text' that combines the value of both.
- Pruning happens for 'checking if X works' or 'noted for later' items that have been addressed or are stale.

Output JSON ONLY:
{
  "actions": [
    {
      "item_id": "<id>",
      "action": "promote|merge|prune|keep",
      "target_id": "<optional_id_for_merge>",
      "reason": "...",
      "consolidated_text": "<optional_new_text_if_merged_or_refined>"
    }
  ]
}"#;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DreamingAction {
    pub item_id: String,
    pub action: String,
    pub target_id: Option<String>,
    pub reason: String,
    pub consolidated_text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DreamingResponse {
    pub actions: Vec<DreamingAction>,
}

pub async fn run_dreaming_worker(
    state: AppState,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    let cfg = state.dreaming.clone();
    if !cfg.enabled {
        info!("dreaming worker: disabled");
        return;
    }

    info!(
        "dreaming worker: starting — interval={}s batch={} source={} target={}",
        cfg.interval_secs, cfg.batch_size, cfg.source_id, cfg.target_source_id
    );

    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = process_dreaming_round(&state).await {
                    error!("dreaming worker: round error: {err}");
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("dreaming worker: shutdown signal received, exiting loop");
                    break;
                }
            }
        }
    }
}

pub async fn process_dreaming_round(state: &AppState) -> Result<()> {
    if !state.analysis.is_configured() {
        return Err(anyhow!("dreaming requires analysis (LLM) to be configured"));
    }

    let cfg = &state.dreaming;
    let (items, _) = state.store.list_items(crate::db::ListItemsRequest {
        source_id: Some(cfg.source_id.clone()),
        limit: Some(cfg.batch_size),
        sort_order: crate::db::SortOrder::Asc, // Process oldest first
        ..Default::default()
    })?;

    if items.is_empty() {
        debug!("dreaming: no items in '{}' to process", cfg.source_id);
        return Ok(());
    }

    info!("dreaming: processing {} item(s) from '{}'", items.len(), cfg.source_id);

    for item in items {
        if let Err(e) = process_item_dreaming(state, item).await {
            error!("dreaming: failed to process item: {e}");
        }
    }

    Ok(())
}

async fn process_item_dreaming(state: &AppState, item: ItemRecord) -> Result<()> {
    let embedder = state.embedder.get_ready().map_err(|e| anyhow!(e.to_string()))?;
    let embedding = embedder.embed(&item.text)?;
    
    // Find neighbors in both memory and knowledge to see if we should merge or promote
    let neighbors = state.store.search(&embedding, 5, None)?;
    let filtered_neighbors: Vec<_> = neighbors.into_iter().filter(|h| h.id != item.id).collect();

    let user_prompt = build_dreaming_prompt(&item, &filtered_neighbors);
    let llm_output = call_llm(state, DREAMING_SYSTEM_PROMPT, &user_prompt).await?;
    
    let response: DreamingResponse = serde_json::from_str(extract_json(&llm_output))
        .map_err(|e| anyhow!("failed to parse dreaming response: {e}. Output was: {}", llm_output))?;

    for action in response.actions {
        apply_dreaming_action(state, action).await?;
    }

    Ok(())
}

fn build_dreaming_prompt(item: &ItemRecord, neighbors: &[SearchHit]) -> String {
    let mut out = String::new();
    out.push_str("ITEM TO REVIEW:\n");
    out.push_str(&format!("id: {}\ntext: {}\n\n", item.id, item.text));
    
    if !neighbors.is_empty() {
        out.push_str("POTENTIAL NEIGHBORS (for merging/context):\n");
        for n in neighbors {
            out.push_str(&format!("- id: {} (src: {}) dist: {:.3}\n  text: {}\n", 
                n.id, n.source_id, n.distance, n.text.chars().take(200).collect::<String>()));
        }
    }

    out.push_str("\nReview the ITEM TO REVIEW and suggest one of the ACTIONS. If merging, specify target_id and provide consolidated_text.");
    out
}

async fn apply_dreaming_action(state: &AppState, action: DreamingAction) -> Result<()> {
    let item = state.store.get_item(&action.item_id)?
        .ok_or_else(|| anyhow!("item {} not found during action application", action.item_id))?;

    match action.action.as_str() {
        "promote" => {
            info!("dreaming: promoting {} to knowledge: {}", action.item_id, action.reason);
            let mut metadata = item.metadata.clone();
            metadata["original_source"] = json!(item.source_id);
            metadata["dreamt_at"] = json!(chrono::Utc::now().to_rfc3339());
            metadata["dream_reason"] = json!(action.reason);
            
            let embedder = state.embedder.get_ready().map_err(|e| anyhow!(e.to_string()))?;
            let embedding = embedder.embed(&item.text)?;

            state.store.upsert_item(ItemRecord {
                source_id: state.dreaming.target_source_id.clone(),
                metadata,
                ..item
            }, &embedding)?;
        }
        "merge" => {
            if let Some(target_id) = action.target_id {
                info!("dreaming: merging {} into {}: {}", action.item_id, target_id, action.reason);
                if let Some(consolidated) = action.consolidated_text {
                    let mut target = state.store.get_item(&target_id)?
                        .ok_or_else(|| anyhow!("merge target {} not found", target_id))?;
                    
                    target.text = consolidated;
                    target.metadata["merged_from"] = json!(vec![action.item_id.clone()]);
                    
                    // We need to re-embed the consolidated text
                    let embedder = state.embedder.get_ready().map_err(|e| anyhow!(e.to_string()))?;
                    let embedding = embedder.embed(&target.text)?;
                    
                    state.store.upsert_item(target, &embedding)?;
                    state.store.delete_item(&action.item_id)?;
                }
            }
        }
        "prune" => {
            info!("dreaming: pruning {}: {}", action.item_id, action.reason);
            state.store.delete_item(&action.item_id)?;
        }
        "keep" => {
            debug!("dreaming: keeping {} in memory: {}", action.item_id, action.reason);
            // Optionally update metadata to mark it as reviewed
        }
        _ => warn!("dreaming: unknown action '{}' for item {}", action.action, action.item_id),
    }

    Ok(())
}

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

    #[test]
    fn test_extract_json() {
        let content = "```json\n{\"actions\": []}\n```";
        assert_eq!(extract_json(content), "{\"actions\": []}");
        
        let content = "{\"actions\": []}";
        assert_eq!(extract_json(content), "{\"actions\": []}");
    }

    #[test]
    fn test_parse_response() {
        let raw = r#"{"actions": [{"item_id": "1", "action": "promote", "reason": "test"}]}"#;
        let resp: DreamingResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.actions.len(), 1);
        assert_eq!(resp.actions[0].action, "promote");
    }
}
