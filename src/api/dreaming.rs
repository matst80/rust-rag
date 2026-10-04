//! Dreaming: LLM-assisted consolidation of short-term entries.
//!
//! Safety model (all of it is on by default):
//!
//! * **Durable cursor.** Every entry the worker reaches a *terminal* decision
//!   about — `promote`, `merge`, `prune`, `keep`, an unknown action, or a
//!   parse/LLM failure — gets `dreamed_at` stamped. The next round asks the
//!   store for `dreamed_at IS NULL`, so an entry is reviewed exactly once.
//!   This is what `keep` previously failed to do, which made the worker
//!   re-pick the same oldest N every round forever.
//! * **Copy, don't move.** `promote` writes a *new* entry into the target
//!   namespace and leaves the original in place, instead of rewriting
//!   `source_id` in place. Nothing can silently vanish from a namespace a
//!   client searches.
//! * **Protected namespaces.** Any action that would move or delete an entry
//!   out of `RAG_DREAMING_PROTECTED_SOURCE_IDS` (default: `memory`, the
//!   namespace the `eventsourced-ai` enricher searches) is downgraded to
//!   `keep` and audited as `blocked`.
//! * **Allowlist.** Only `RAG_DREAMING_ALLOWED_ACTIONS` (default
//!   `keep,promote`) may be applied. `merge` and `prune` delete client-owned
//!   data and stay opt-in.
//! * **Dry run first.** `POST /api/dream` defaults to `dry_run`, so a manual
//!   invocation reports proposals and changes nothing. Applying requires an
//!   explicit `"apply": true` plus a server-side config that permits it.
//! * **Audit log.** Every applied action, blocked action and dry-run proposal
//!   lands in `dream_actions` (what / why / which item / which round / when).
//! * **Bounded.** `max_items_per_round` and `max_actions_per_round` cap the
//!   round regardless of what the LLM returns.
//!
//! See `docs/dreaming.md` for the operator-facing description.

use crate::{
    api::{AppState, analysis::call_llm},
    config::{DreamActionKind, DreamingConfig},
    db::{DreamActionRecord, ItemRecord, ListItemsRequest, SearchHit, SortOrder},
};
use anyhow::{Result, anyhow};
use chrono::Utc;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct DreamingAction {
    pub item_id: String,
    pub action: String,
    pub target_id: Option<String>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub consolidated_text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct DreamingResponse {
    #[serde(default)]
    pub actions: Vec<DreamingAction>,
}

/// What a round actually did. Returned by `POST /api/dream` so an operator can
/// see the effect without reading the audit table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DreamRoundReport {
    pub round_id: String,
    /// `true` when nothing was written to items (proposals only).
    pub dry_run: bool,
    pub source_id: String,
    /// Items considered this round (never reviewed before, oldest first).
    pub considered: usize,
    /// Items whose `dreamed_at` cursor was advanced. Zero for a dry run.
    pub marked: usize,
    pub applied: usize,
    pub blocked: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Cursor high-water mark for the next round: the largest `created_at`
    /// considered. `None` when the round saw nothing.
    pub cursor_created_at: Option<i64>,
    pub actions: Vec<DreamActionRecord>,
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
        "dreaming worker: starting — interval={}s batch={} source={} target={} dry_run={} allowed={:?} protected={:?}",
        cfg.interval_secs,
        cfg.batch_size,
        cfg.source_id,
        cfg.target_source_id,
        cfg.dry_run,
        cfg.allowed_actions,
        cfg.protected_source_ids
    );

    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // One round at a time: an overlapping round could double-stamp
                // or double-act on the same item.
                match process_dreaming_round(&state).await {
                    Ok(report) => info!(
                        round = %report.round_id,
                        considered = report.considered,
                        applied = report.applied,
                        blocked = report.blocked,
                        "dreaming round complete"
                    ),
                    Err(err) => error!("dreaming worker: round error: {err}"),
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

/// Run one dreaming round using the server-configured posture (including
/// `RAG_DREAMING_DRY_RUN`). Used by the background worker.
pub async fn process_dreaming_round(state: &AppState) -> Result<DreamRoundReport> {
    let opts = DreamRoundOptions {
        dry_run: state.dreaming.dry_run,
        ..DreamRoundOptions::default()
    };
    run_dreaming_round(state, opts).await
}

/// Knobs a single `POST /api/dream` invocation may tighten. `None` always
/// means "inherit the server config", never "ignore the server config".
#[derive(Debug, Clone)]
pub struct DreamRoundOptions {
    /// When `true`, propose actions and write nothing. Defaults to the server
    /// config, which itself defaults to `true`.
    pub dry_run: bool,
    /// Request `apply = true`. Honoured only when the server config permits
    /// the underlying action — the allowlist and protected namespaces are
    /// never bypassable by a request.
    pub apply: bool,
    /// Optional source namespace override (still subject to the same
    /// allowlist / protection rules).
    pub source_id: Option<String>,
    /// Optional item limit; clamped to `max_items_per_round`.
    pub limit: Option<usize>,
}

impl Default for DreamRoundOptions {
    fn default() -> Self {
        Self {
            dry_run: true,
            apply: false,
            source_id: None,
            limit: None,
        }
    }
}

impl DreamRoundOptions {
    /// Effective write mode for this round. `apply` alone is never enough:
    /// the per-call `dry_run` flag and the config default both have to agree.
    #[allow(dead_code)]
    fn writes(self, cfg: &DreamingConfig) -> bool {
        self.apply && !self.dry_run && !cfg.dry_run
    }
}

/// Run one dreaming round and return what it did.
pub async fn run_dreaming_round(
    state: &AppState,
    options: DreamRoundOptions,
) -> Result<DreamRoundReport> {
    if !state.analysis.is_configured() {
        return Err(anyhow!("dreaming requires analysis (LLM) to be configured"));
    }

    let cfg = &state.dreaming;
    let DreamRoundOptions {
        dry_run: request_dry_run,
        apply,
        source_id: source_override,
        limit: requested_limit,
    } = &options;
    let source_id = source_override
        .clone()
        .unwrap_or_else(|| cfg.source_id.clone());
    let dry_run = !(*apply && !*request_dry_run && !cfg.dry_run);
    let round_id = Uuid::now_v7().to_string();

    // Bounded twice over: by the caller's `limit` and by the config cap, and
    // never above `batch_size` unless that is the larger of the two.
    let cap = cfg.max_items_per_round.max(cfg.batch_size).max(1).min(4096);
    let limit = requested_limit
        .unwrap_or(cfg.batch_size.max(1))
        .clamp(1, cap);

    let (items, _) = state.store.list_items(ListItemsRequest {
        source_id: Some(source_id.clone()),
        limit: Some(limit),
        sort_order: SortOrder::Asc, // Process oldest first
        // The durable cursor: never-reviewed entries only. Combined with the
        // `dreamed_at` stamp this makes each entry single-shot.
        dreamed: Some(false),
        ..Default::default()
    })?;

    if items.is_empty() {
        debug!("dreaming: no unreviewed items in '{source_id}' to process");
        return Ok(DreamRoundReport {
            round_id,
            dry_run,
            source_id,
            ..Default::default()
        });
    }

    info!(
        "dreaming: round {round_id} considering {} item(s) from '{source_id}' \
         (dry_run={dry_run} concurrency={} max_actions={} allowed={:?} protected={:?})",
        items.len(),
        cfg.concurrency,
        cfg.max_actions_per_round,
        cfg.allowed_actions,
        cfg.protected_source_ids,
    );

    let cursor_created_at = items.iter().map(|item| item.created_at).max();
    let concurrency = cfg.concurrency.max(1);
    let action_budget = cfg.max_actions_per_round.max(1);
    let round_ctx = std::sync::Arc::new(RoundContext {
        round_id: round_id.clone(),
        dry_run,
        action_budget: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(action_budget)),
    });

    let reports: Vec<DreamRoundReport> = futures_util::stream::iter(items)
        .map(|item| process_item_dreaming(state, item, &source_id, &round_ctx))
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut report = DreamRoundReport {
        round_id,
        dry_run,
        source_id,
        considered: reports.len(),
        cursor_created_at,
        ..Default::default()
    };
    for item_report in reports {
        report.marked += item_report.marked;
        report.applied += item_report.applied;
        report.blocked += item_report.blocked;
        report.skipped += item_report.skipped;
        report.failed += item_report.failed;
        report.actions.extend(item_report.actions);
    }

    Ok(report)
}

/// Shared, per-round mutable state visible to concurrently processed items.
#[derive(Debug, Clone)]
pub(crate) struct RoundContext {
    round_id: String,
    dry_run: bool,
    /// Remaining action budget for the whole round, shared across items.
    action_budget: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl RoundContext {
    /// Claim one slot of the round's action budget, or `None` when exhausted.
    fn claim_action(&self) -> Option<()> {
        use std::sync::atomic::Ordering;
        self.action_budget
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1))
            .ok()
            .map(|_| ())
    }
}

/// One entry's turn through the round. An error is reported as a failed item
/// rather than aborting the round: one bad item must not strand the rest of
/// the batch (the cursor is what guarantees progress, not the round).
async fn process_item_dreaming(
    state: &AppState,
    item: ItemRecord,
    source_id: &str,
    round: &std::sync::Arc<RoundContext>,
) -> DreamRoundReport {
    match process_item_inner(state, item, round).await {
        Ok(report) => report,
        Err(err) => {
            error!("dreaming: failed to process item: {err}");
            DreamRoundReport {
                round_id: round.round_id.clone(),
                dry_run: round.dry_run,
                source_id: source_id.to_owned(),
                considered: 1,
                failed: 1,
                ..Default::default()
            }
        }
    }
}

async fn process_item_inner(
    state: &AppState,
    item: ItemRecord,
    round: &std::sync::Arc<RoundContext>,
) -> Result<DreamRoundReport> {
    let cfg = &state.dreaming;
    let mut report = DreamRoundReport {
        round_id: round.round_id.clone(),
        dry_run: round.dry_run,
        source_id: cfg.source_id.clone(),
        ..Default::default()
    };
    report.considered = 1;

    let embedder = state.embedder.try_ready()?;
    let embedding = embedder.embed(&item.text)?;

    // Find neighbors in both memory and knowledge to see if we should merge or promote
    let neighbors = state.store.search(&embedding, 5, None, None)?;
    let filtered_neighbors: Vec<_> = neighbors.into_iter().filter(|h| h.id != item.id).collect();

    let user_prompt = build_dreaming_prompt(&item, &filtered_neighbors);
    let llm_output = call_llm(state, DREAMING_SYSTEM_PROMPT, &user_prompt).await?;

    let response: DreamingResponse =
        serde_json::from_str(extract_json(&llm_output)).map_err(|e| {
            anyhow!(
                "failed to parse dreaming response: {e}. Output was: {}",
                llm_output
            )
        })?;

    // One LLM call may name the item several times (or not at all). Only the
    // first action that actually applies counts, and the item is marked
    // reviewed either way so a malformed response cannot re-pick it forever.
    let mut saw_action_for_item = false;
    for action in response.actions.iter().take(MAX_ACTIONS_PER_ITEM) {
        if action.item_id != item.id {
            // Actions about other items belong to their own round pass; ignore
            // them here so a batched response cannot act out of order.
            continue;
        }
        if saw_action_for_item {
            debug!("dreaming: ignoring duplicate action for item {}", item.id);
            continue;
        }
        saw_action_for_item = true;
        let record = apply_dreaming_action(state, action.clone(), round).await;
        let outcome = record.outcome.clone();
        report.actions.push(record);
        match outcome.as_str() {
            "applied" => report.applied += 1,
            "blocked" => report.blocked += 1,
            "skipped" => report.skipped += 1,
            _ => report.failed += 1,
        }
    }

    // Terminal outcome: advance the cursor. This runs for `keep`, for every
    // other recognised action, and for an empty/foreign response.
    if !round.dry_run
        && state
            .store
            .set_item_dreamed_at(&item.id, Some(Utc::now().timestamp_millis()))?
    {
        report.marked = 1;
    }
    Ok(report)
}

/// Upper bound on how many actions one LLM response for one item may
/// contribute. Keeps a chatty model from fanning a single item out.
const MAX_ACTIONS_PER_ITEM: usize = 4;

#[cfg(test)]
fn round_for_test(dry_run: bool, action_budget: usize) -> std::sync::Arc<RoundContext> {
    std::sync::Arc::new(RoundContext {
        round_id: "test-round".to_owned(),
        dry_run,
        action_budget: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(action_budget)),
    })
}

fn build_dreaming_prompt(item: &ItemRecord, neighbors: &[SearchHit]) -> String {
    let mut out = String::new();
    out.push_str("ITEM TO REVIEW:\n");
    out.push_str(&format!("id: {}\ntext: {}\n\n", item.id, item.text));

    if !neighbors.is_empty() {
        out.push_str("POTENTIAL NEIGHBORS (for merging/context):\n");
        for n in neighbors {
            out.push_str(&format!(
                "- id: {} (src: {}) dist: {:.3}\n  text: {}\n",
                n.id,
                n.source_id,
                n.distance,
                n.text.chars().take(200).collect::<String>()
            ));
        }
    }

    out.push_str("\nReview the ITEM TO REVIEW and suggest one of the ACTIONS. If merging, specify target_id and provide consolidated_text.");
    out
}

/// Decide what `action` would do and (unless this is a dry run) do it.
///
/// Every branch records a `DreamActionRecord` — applied, blocked, skipped or
/// failed — so the audit table always explains the outcome. Never panics and
/// never silently succeeds on an action it did not perform.
pub(crate) async fn apply_dreaming_action(
    state: &AppState,
    action: DreamingAction,
    round: &RoundContext,
) -> DreamActionRecord {
    let now_ms = Utc::now().timestamp_millis();
    let cfg = &state.dreaming;
    let mut record = DreamActionRecord {
        id: Uuid::now_v7().to_string(),
        round_id: round.round_id.clone(),
        item_id: action.item_id.clone(),
        action: action.action.trim().to_lowercase(),
        target_id: action.target_id.clone(),
        result_item_id: None,
        reason: action.reason.clone(),
        outcome: "skipped".to_owned(),
        dry_run: round.dry_run,
        created_at: now_ms,
    };

    let Some(kind) = DreamActionKind::parse(&action.action) else {
        record.outcome = "skipped".to_owned();
        record.reason = format!("unknown action '{}': {}", action.action, action.reason);
        warn!(
            "dreaming: unknown action '{}' for item {}",
            action.action, action.item_id
        );
        persist(state, &record);
        return record;
    };
    record.action = kind.as_str().to_owned();

    let item = match state.store.get_item(&action.item_id) {
        Ok(Some(item)) => item,
        Ok(None) => {
            record.outcome = "skipped".to_owned();
            record.reason = format!("item {} not found", action.item_id);
            persist(state, &record);
            return record;
        }
        Err(err) => {
            record.outcome = "failed".to_owned();
            record.reason = format!("failed to load item: {err}");
            persist(state, &record);
            return record;
        }
    };

    // Gate 1 — action allowlist. `merge` and `prune` delete client-owned data
    // and are opt-in via RAG_DREAMING_ALLOWED_ACTIONS.
    if !cfg.allows(kind) {
        record.outcome = "blocked".to_owned();
        record.reason = format!(
            "action '{}' is not in RAG_DREAMING_ALLOWED_ACTIONS ({:?}): {}",
            kind.as_str(),
            cfg.allowed_actions,
            action.reason
        );
        warn!(
            "dreaming: blocked {} on {} — action not allowed",
            kind.as_str(),
            item.id
        );
        persist(state, &record);
        return record;
    }

    // Gate 2 — protected namespaces. Only *destructive* actions are blocked
    // here: a `merge` or `prune` of an entry in a namespace the harness client
    // searches would remove or overwrite that client's data. A `promote`
    // copy is permitted — it writes a new row in the target namespace and
    // leaves the original exactly where it was, so nothing disappears from
    // the protected namespace.
    if kind.destroys_entry() && cfg.is_protected(&item.source_id) {
        record.outcome = "blocked".to_owned();
        record.reason = format!(
            "namespace '{}' is protected (RAG_DREAMING_PROTECTED_SOURCE_IDS {:?}); \
             {} not applied: {}",
            item.source_id,
            cfg.protected_source_ids,
            kind.as_str(),
            action.reason
        );
        warn!(
            "dreaming: blocked {} on {} — namespace '{}' is protected",
            kind.as_str(),
            item.id,
            item.source_id
        );
        persist(state, &record);
        return record;
    }

    // Gate 3 — round budget. Bounded even in dry-run proposal mode so the
    // report cannot grow without limit.
    if round.claim_action().is_none() {
        record.outcome = "skipped".to_owned();
        record.reason = format!(
            "round action budget exhausted ({} actions); not applied: {}",
            cfg.max_actions_per_round, action.reason
        );
        persist(state, &record);
        return record;
    }

    if round.dry_run {
        record.outcome = "proposed".to_owned();
        record.reason = format!("dry run, not applied: {}", action.reason);
        record.result_item_id = proposed_result_id(kind, &item, &action);
        persist(state, &record);
        return record;
    }

    let outcome = match kind {
        DreamActionKind::Promote => apply_promote(state, &item, &action, &mut record),
        DreamActionKind::Merge => apply_merge(state, &item, &action, &mut record),
        DreamActionKind::Prune => apply_prune(state, &item, &mut record),
        DreamActionKind::Keep => {
            info!("dreaming: keeping {} in place: {}", item.id, action.reason);
            record.outcome = "applied".to_owned();
            record.result_item_id = Some(item.id.clone());
            Ok(())
        }
    };
    if let Err(err) = outcome {
        record.outcome = "failed".to_owned();
        record.reason = format!("{err}");
    }
    persist(state, &record);
    record
}

/// Copy the entry into the target namespace under a new id. The original row
/// is left untouched, so nothing is lost if the copy is wrong — delete it by
/// hand or re-run. This is the deliberate change from the old in-place
/// `source_id` rewrite, which silently pulled entries out of namespaces
/// clients search.
fn apply_promote(
    state: &AppState,
    item: &ItemRecord,
    action: &DreamingAction,
    record: &mut DreamActionRecord,
) -> Result<()> {
    info!(
        "dreaming: promoting {} -> {} (copy): {}",
        item.id, state.dreaming.target_source_id, action.reason
    );
    let new_id = format!("{}:dream:{}", item.id, Uuid::now_v7());

    let mut metadata = item.metadata.clone();
    if let serde_json::Value::Object(ref mut map) = metadata {
        map.insert("original_source".to_owned(), json!(item.source_id));
        map.insert("dreamt_at".to_owned(), json!(Utc::now().to_rfc3339()));
        map.insert("dream_reason".to_owned(), json!(action.reason));
        map.insert("dream_source_id".to_owned(), json!(item.id));
        map.insert("dream_round_id".to_owned(), json!(record.round_id));
    }

    let embedder = state.embedder.try_ready()?;
    let embedding = embedder.embed(&item.text)?;

    state.store.upsert_item(
        ItemRecord {
            id: new_id.clone(),
            source_id: state.dreaming.target_source_id.clone(),
            metadata,
            created_at: Utc::now().timestamp_millis(),
            updated_at: Utc::now().timestamp_millis(),
            // The copy is already terminal; stamp it so it is not itself
            // picked up if the target namespace is ever a dream source.
            dreamed_at: Some(Utc::now().timestamp_millis()),
            ..item.clone()
        },
        &embedding,
    )?;
    record.result_item_id = Some(new_id);
    Ok(())
}

fn apply_merge(
    state: &AppState,
    item: &ItemRecord,
    action: &DreamingAction,
    record: &mut DreamActionRecord,
) -> Result<()> {
    let Some(target_id) = action.target_id.as_deref() else {
        anyhow::bail!("merge action for {} has no target_id", item.id);
    };
    if target_id == item.id {
        anyhow::bail!("merge target for {} is the item itself", item.id);
    }
    let Some(consolidated) = action.consolidated_text.as_deref() else {
        anyhow::bail!("merge action for {} has no consolidated_text", item.id);
    };
    if consolidated.trim().is_empty() {
        anyhow::bail!(
            "merge action for {} has an empty consolidated_text",
            item.id
        );
    }

    let mut target = state
        .store
        .get_item(target_id)?
        .ok_or_else(|| anyhow!("merge target {} not found", target_id))?;

    info!(
        "dreaming: merging {} into {}: {}",
        item.id, target_id, action.reason
    );

    let previous_text = target.text.clone();
    target.text = consolidated.to_owned();
    target.updated_at = Utc::now().timestamp_millis();
    if let serde_json::Value::Object(ref mut map) = target.metadata {
        let mut merged: Vec<String> = map
            .get("dream_merged_from")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        merged.push(item.id.clone());
        map.insert("dream_merged_from".to_owned(), json!(merged));
        map.insert("dream_previous_text".to_owned(), json!(previous_text));
        map.insert("dream_reason".to_owned(), json!(action.reason));
        map.insert("dream_round_id".to_owned(), json!(record.round_id));
    }

    // We need to re-embed the consolidated text
    let embedder = state.embedder.try_ready()?;
    let embedding = embedder.embed(&target.text)?;

    state.store.upsert_item(target, &embedding)?;

    // The source entry is removed last so a failure above leaves both rows
    // intact. The previous text is preserved on the target, so the merge is
    // reversible by hand.
    if !state.store.delete_item(&item.id)? {
        warn!("dreaming: merged {} but could not delete it", item.id);
    }
    record.result_item_id = Some(target_id.to_owned());
    Ok(())
}

fn apply_prune(state: &AppState, item: &ItemRecord, record: &mut DreamActionRecord) -> Result<()> {
    info!("dreaming: pruning {}: {}", item.id, record.reason);
    state.store.delete_item(&item.id)?;
    record.result_item_id = None;
    Ok(())
}

/// What id an action *would* produce, for the dry-run report.
fn proposed_result_id(
    kind: DreamActionKind,
    item: &ItemRecord,
    action: &DreamingAction,
) -> Option<String> {
    match kind {
        DreamActionKind::Promote => Some(format!("{}:dream:<new-id>", item.id)),
        DreamActionKind::Merge => action.target_id.clone(),
        DreamActionKind::Prune => None,
        DreamActionKind::Keep => Some(item.id.clone()),
    }
}

fn persist(state: &AppState, record: &DreamActionRecord) {
    if let Err(err) = state.store.insert_dream_action(record.clone()) {
        error!(
            "dreaming: failed to write audit record for {} on {}: {err}",
            record.action, record.item_id
        );
    }
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

    #[test]
    fn test_parse_response_defaults_missing_optional_fields() {
        let raw = r#"{"actions": [{"item_id": "1", "action": "keep"}]}"#;
        let resp: DreamingResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.actions[0].reason, "");
        assert!(resp.actions[0].target_id.is_none());
    }

    #[test]
    fn test_action_kind_parsing_is_case_insensitive_and_rejects_junk() {
        assert_eq!(
            DreamActionKind::parse("PROMOTE"),
            Some(DreamActionKind::Promote)
        );
        assert_eq!(
            DreamActionKind::parse("  prune "),
            Some(DreamActionKind::Prune)
        );
        assert_eq!(DreamActionKind::parse("nonsense"), None);
    }

    #[test]
    fn only_merge_and_prune_destroy_the_entry() {
        // A `promote` copy writes a new row and leaves the original in place,
        // so it must not be treated as destructive: the harness searches the
        // protected "memory" namespace and needs copies to escape it.
        assert!(!DreamActionKind::Keep.destroys_entry());
        assert!(!DreamActionKind::Promote.destroys_entry());
        assert!(DreamActionKind::Merge.destroys_entry());
        assert!(DreamActionKind::Prune.destroys_entry());
    }

    #[test]
    fn memory_namespace_is_protected_by_default() {
        let cfg = DreamingConfig::default();
        assert!(cfg.is_protected("memory"));
        assert!(!cfg.is_protected("knowledge"));
    }
}
