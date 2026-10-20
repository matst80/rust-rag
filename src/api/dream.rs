use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::error;
use uuid::Uuid;

use super::dreaming;
use super::error::{ApiError, validate_source_id};
use super::state::AppState;
use crate::cluster::{ClusterParams, Doc};
use crate::db::{DreamActionRecord, DreamClusterRecord, ItemRecord, ListItemsRequest, SortOrder};

/// Optional per-request overrides for a manually triggered dreaming round.
/// Everything is optional: an empty body (or no body at all) reproduces the
/// previous behaviour of running the server-configured round.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct DreamRequest {
    /// Namespace to dream over. Defaults to `RAG_DREAMING_SOURCE_ID`.
    pub source_id: Option<String>,
    /// Pass `true` to persist `dream_clusters` / members this round.
    /// Still honours the server-side `RAG_DREAMING_DRY_RUN` guard: when the
    /// server is configured dry-run, the round stays dry no matter what.
    /// Defaults to `false` (dry run: proposals only, nothing written).
    pub apply: Option<bool>,
    /// Ceiling on entries fed to the deterministic pre-pass. Defaults to
    /// `RAG_DREAMING_MAX_DOCS_PER_ROUND`.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DreamTriggerResponse {
    pub accepted: bool,
    pub source_id: String,
    /// Effective write posture after applying the server-side dry-run guard.
    pub dry_run: bool,
    pub limit: usize,
    pub note: String,
}

pub(crate) async fn dreaming_endpoint(
    State(state): State<AppState>,
    body: Option<Json<DreamRequest>>,
) -> Result<(StatusCode, Json<DreamTriggerResponse>), ApiError> {
    let req = body.map(|b| b.0).unwrap_or_default();
    if let Some(source_id) = req.source_id.as_deref() {
        validate_source_id(source_id)?;
    }
    let cfg = &state.dreaming;
    let source_id = req.source_id.unwrap_or_else(|| cfg.source_id.clone());
    // Default dry unless the caller explicitly passes `"apply": true`, and
    // the server-side `dry_run` guard always wins so a mis-set client cannot
    // start write automation the operator did not configure.
    let dry_run = cfg.dry_run || !req.apply.unwrap_or(false);
    let limit = req
        .limit
        .unwrap_or(cfg.max_docs_per_round)
        .clamp(1, cfg.max_docs_per_round.max(1));

    // Deterministic corpus round: no LLM needed. Spawn in background so the
    // HTTP request doesn't timeout.
    let state_clone = state.clone();
    let source_for_task = source_id.clone();
    tokio::spawn(async move {
        if let Err(e) = dreaming::run_corpus_dreaming_round(
            &state_clone,
            &source_for_task,
            dry_run,
            Some(limit),
        )
        .await
        {
            error!("manual dreaming error: {e}");
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(DreamTriggerResponse {
            accepted: true,
            source_id,
            dry_run,
            limit,
            note: "corpus round running in background; read clusters/actions when it lands"
                .to_owned(),
        }),
    ))
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ListDreamClustersQuery {
    pub source_id: Option<String>,
    pub round_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DreamClusterPayload {
    pub id: String,
    pub round_id: String,
    pub source_id: String,
    pub label: String,
    pub member_count: usize,
    pub vocabulary: Vec<String>,
    pub cohesion: f32,
    pub created_at: i64,
}

impl From<DreamClusterRecord> for DreamClusterPayload {
    fn from(value: DreamClusterRecord) -> Self {
        Self {
            id: value.id,
            round_id: value.round_id,
            source_id: value.source_id,
            label: value.label,
            member_count: value.member_count,
            vocabulary: value.vocabulary,
            cohesion: value.cohesion,
            created_at: value.created_at,
        }
    }
}

/// Clusters persisted by corpus dreaming rounds, newest first. Empty until a
/// non-dry round has run (`POST /api/dream` with `"apply": true` on a server
/// whose `RAG_DREAMING_DRY_RUN` allows writes).
pub(crate) async fn list_dream_clusters(
    State(state): State<AppState>,
    Query(query): Query<ListDreamClustersQuery>,
) -> Result<Json<Vec<DreamClusterPayload>>, ApiError> {
    if let Some(source_id) = query.source_id.as_deref() {
        validate_source_id(source_id)?;
    }
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let store = state.store.clone();
    let clusters = tokio::task::spawn_blocking(move || {
        store.list_dream_clusters(limit, query.source_id.as_deref(), query.round_id.as_deref())
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(clusters.into_iter().map(Into::into).collect()))
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DreamClusterMembersResponse {
    pub cluster_id: String,
    pub member_count: usize,
    pub members: Vec<String>,
}

/// Member entry ids of one persisted dream cluster, sorted.
pub(crate) async fn dream_cluster_members(
    State(state): State<AppState>,
    Path(cluster_id): Path<String>,
) -> Result<Json<DreamClusterMembersResponse>, ApiError> {
    let store = state.store.clone();
    let id_for_lookup = cluster_id.clone();
    let members = tokio::task::spawn_blocking(move || store.dream_cluster_members(&id_for_lookup))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(DreamClusterMembersResponse {
        cluster_id,
        member_count: members.len(),
        members,
    }))
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ListDreamActionsQuery {
    pub limit: Option<usize>,
    pub item_id: Option<String>,
    pub dry_run: Option<bool>,
}

/// Deterministic merge rule for one near-duplicate pair: the longer text
/// keeps (at high containment it subsumes the shorter note); ties break
/// toward the smaller id so reruns agree. Returns `(keeper_id, dup_id)`.
pub(crate) fn pick_merge_keeper(
    a_id: &str,
    a_text: &str,
    b_id: &str,
    b_text: &str,
) -> (String, String) {
    if a_text.len() > b_text.len() || (a_text.len() == b_text.len() && a_id <= b_id) {
        (a_id.to_owned(), b_id.to_owned())
    } else {
        (b_id.to_owned(), a_id.to_owned())
    }
}

fn importance_of(metadata: &Value) -> Option<i64> {
    metadata.get("importance")?.as_i64()
}

/// Fold a deleted duplicate's signal into its keeper: union `tags`
/// (keeper order first) and keep the higher numeric `importance`.
/// Missing/non-numeric importance counts as 0 and is only written back
/// when the duplicate actually raises the keeper.
pub(crate) fn fold_dup_metadata(keeper: &mut Value, dup: &Value) {
    let mut tags: Vec<String> = keeper
        .get("tags")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if let Some(arr) = dup.get("tags").and_then(|t| t.as_array()) {
        for v in arr.iter().filter_map(|v| v.as_str()) {
            let t = v.trim();
            if !t.is_empty() && !tags.iter().any(|e| e == t) {
                tags.push(t.to_owned());
            }
        }
    }
    let keeper_imp = importance_of(keeper).unwrap_or(0);
    let dup_imp = dup
        .as_object()
        .and_then(|o| o.get("importance"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let Some(obj) = keeper.as_object_mut() else {
        return;
    };
    if !tags.is_empty() {
        obj.insert(
            "tags".to_owned(),
            Value::Array(tags.into_iter().map(Value::String).collect()),
        );
    }
    if dup_imp > keeper_imp {
        obj.insert("importance".to_owned(), Value::Number(dup_imp.into()));
    }
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct DreamApplyRequest {
    /// Namespace to declutter. Defaults to `RAG_DREAMING_SOURCE_ID`.
    pub source_id: Option<String>,
    /// Only pairs at or above this containment merge. Never looser than the
    /// server's `RAG_DREAMING_DUP_CONTAINMENT`. Defaults to that value.
    pub min_containment: Option<f32>,
    /// Max pairs to apply this call. Defaults to 50, capped at 200.
    pub limit: Option<usize>,
    /// `false` (default) returns the merge plan without writing anything.
    /// `true` merges keepers, deletes dups and logs `applied` audit rows.
    pub apply: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DreamMergePlan {
    pub keeper_id: String,
    pub dup_id: String,
    pub containment: f32,
    pub keeper_chars: usize,
    pub dup_chars: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DreamApplyResponse {
    pub dry_run: bool,
    pub source_id: String,
    pub min_containment: f32,
    pub pairs: Vec<DreamMergePlan>,
    pub applied: usize,
    pub skipped: usize,
    pub deleted: Vec<String>,
}

/// Declutter one namespace by merging deterministic near-duplicate pairs.
///
/// Plan mode (default) only reports what *would* merge. Apply mode folds
/// each dup's tags/importance into the longer-text keeper (`pick_merge_keeper`)
/// and deletes the dup — the same procedure as a reviewed manual dreaming
/// pass, restricted to pairs the deterministic pass already proposes.
/// Every applied merge lands an `applied` row in `dream_actions`.
pub(crate) async fn apply_dreaming_round(
    State(state): State<AppState>,
    body: Option<Json<DreamApplyRequest>>,
) -> Result<Json<DreamApplyResponse>, ApiError> {
    let req = body.map(|b| b.0).unwrap_or_default();
    if let Some(source_id) = req.source_id.as_deref() {
        validate_source_id(source_id)?;
    }
    let cfg = &state.dreaming;
    let source_id = req.source_id.unwrap_or_else(|| cfg.source_id.clone());
    let min_containment = req
        .min_containment
        .unwrap_or(cfg.dup_containment)
        .max(cfg.dup_containment)
        .clamp(0.0, 1.0);
    let max_pairs = req.limit.unwrap_or(50).clamp(1, 200);
    let dry_run = !req.apply.unwrap_or(false);
    let max_docs = cfg.max_docs_per_round;
    let shingle_len = crate::cluster::ClusterParams::default().shingle_len;

    let store = state.store.clone();
    let source_for_task = source_id.clone();
    let response = tokio::task::spawn_blocking(move || -> anyhow::Result<DreamApplyResponse> {
        let params = ClusterParams {
            dup_containment: min_containment,
            max_docs: max_docs.max(1),
            shingle_len,
            ..ClusterParams::default()
        };
        let items: Vec<ItemRecord> = store
            .list_items(ListItemsRequest {
                source_id: Some(source_for_task.clone()),
                limit: Some(max_docs.max(1)),
                sort_order: SortOrder::Asc,
                ..ListItemsRequest::default()
            })
            .map(|page| page.0)?;
        let docs: Vec<Doc> = items
            .iter()
            .map(|item| Doc {
                id: item.id.clone(),
                text: item.text.clone(),
            })
            .collect();
        let pairs = crate::cluster::near_duplicates(&docs, &params);
        let by_id: std::collections::HashMap<&str, &ItemRecord> =
            items.iter().map(|item| (item.id.as_str(), item)).collect();

        let round_id = Uuid::now_v7().to_string();
        let now = Utc::now().timestamp_millis();
        let mut plans = Vec::new();
        let mut applied = 0usize;
        let mut skipped = 0usize;
        let mut deleted = Vec::new();
        let mut gone: std::collections::HashSet<String> = std::collections::HashSet::new();
        for pair in pairs.into_iter().take(max_pairs) {
            if gone.contains(&pair.a) || gone.contains(&pair.b) {
                skipped += 1;
                continue;
            }
            let (Some(a), Some(b)) = (by_id.get(pair.a.as_str()), by_id.get(pair.b.as_str()))
            else {
                skipped += 1;
                continue;
            };
            let (keeper_id, dup_id) = pick_merge_keeper(&a.id, &a.text, &b.id, &b.text);
            let (keeper, dup) = if keeper_id == a.id {
                (*a, *b)
            } else {
                (*b, *a)
            };
            plans.push(DreamMergePlan {
                keeper_id: keeper_id.clone(),
                dup_id: dup_id.clone(),
                containment: pair.containment,
                keeper_chars: keeper.text.len(),
                dup_chars: dup.text.len(),
            });
            if dry_run {
                continue;
            }
            let mut keeper_metadata = keeper.metadata.clone();
            fold_dup_metadata(&mut keeper_metadata, &dup.metadata);
            store.update_item_metadata(&keeper_id, keeper_metadata)?;
            if store.delete_item(&dup_id)? {
                deleted.push(dup_id.clone());
                applied += 1;
            }
            gone.insert(keeper_id.clone());
            gone.insert(dup_id.clone());
            store.insert_dream_action(DreamActionRecord {
                id: format!("{round_id}:applied:{}", plans.len()),
                round_id: round_id.clone(),
                item_id: dup_id.clone(),
                action: "merge".to_owned(),
                target_id: Some(keeper_id),
                result_item_id: None,
                reason: format!(
                    "dream apply: {:.0}% word-shingle containment, keeper kept, dup deleted",
                    pair.containment * 100.0
                ),
                outcome: "applied".to_owned(),
                dry_run: false,
                created_at: now,
            })?;
        }
        Ok(DreamApplyResponse {
            dry_run,
            source_id: source_for_task,
            min_containment,
            pairs: plans,
            applied,
            skipped,
            deleted,
        })
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(response))
}

/// Dream audit rows, newest first. `dry_run=true` returns only proposals
/// (near-duplicate merge candidates); `dry_run=false` only applied records.
pub(crate) async fn list_dream_actions(
    State(state): State<AppState>,
    Query(query): Query<ListDreamActionsQuery>,
) -> Result<Json<Vec<crate::db::DreamActionRecord>>, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let store = state.store.clone();
    let actions = tokio::task::spawn_blocking(move || {
        store.list_dream_actions(limit, query.item_id.as_deref(), query.dry_run)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(actions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeper_is_longer_text_ties_break_by_id() {
        assert_eq!(
            pick_merge_keeper("a", "short", "b", "much longer text here"),
            ("b".to_owned(), "a".to_owned())
        );
        assert_eq!(
            pick_merge_keeper("b", "same", "a", "same!"),
            ("a".to_owned(), "b".to_owned())
        );
    }

    #[test]
    fn fold_unions_tags_and_keeps_max_importance() {
        let mut keeper = json!({"importance": 3, "tags": ["a", "b"]});
        let dup = json!({"importance": 5, "tags": ["b", "c", " "]});
        fold_dup_metadata(&mut keeper, &dup);
        assert_eq!(keeper["importance"], json!(5));
        assert_eq!(keeper["tags"], json!(["a", "b", "c"]));
    }

    #[test]
    fn fold_never_lowers_importance_or_invents_tags() {
        let mut keeper = json!({"importance": 4});
        let dup = json!({"kind": "fact"});
        fold_dup_metadata(&mut keeper, &dup);
        assert_eq!(keeper["importance"], json!(4));
        assert!(keeper.get("tags").is_none());
    }
}
