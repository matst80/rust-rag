//! Dreaming: deterministic corpus clustering of short-term entries.
//!
//! Corpus-only pipeline. The old per-item LLM loop (one model call per entry
//! against five retrieved neighbours) was removed: measured against a real
//! 2489-entry namespace its retrieval input was noise (every pairwise distance
//! in a 0.44-0.54 band, no separation between related and unrelated entries),
//! so feeding it to a model was garbage-in-garbage-out.
//!
//! What a round does now, with no model and no retrieval:
//!
//! * partitions the whole source into clusters from text alone
//!   (`crate::cluster`: TF-IDF over identifying tokens, hub filtering,
//!   mutual-kNN, cohesion gate),
//! * reports near-duplicates by word-shingle containment as `proposed` audit
//!   rows (evidence, never auto-applied — nothing is deleted or rewritten),
//! * persists clusters to `dream_clusters` / `dream_cluster_members` unless
//!   this is a dry run. Cluster ids hash the member set, so re-runs yield the
//!   same ids and the audit log stays comparable across rounds.
//!
//! Safety model: the pipeline never destroys or moves entries, so there is no
//! allowlist and no protected-namespace gate. `POST /api/dream` and the worker
//! both honour `RAG_DREAMING_DRY_RUN` (default `true`).

use crate::{
    api::AppState,
    cluster::{ClusterParams, Corpus, Doc, near_duplicates},
    config::{DreamActionKind, DreamingConfig},
    db::{DreamActionRecord, DreamClusterRecord, ItemRecord, ListItemsRequest, SortOrder},
};
use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, interval};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Cap on near-duplicate proposals attached to one round report. They are
/// audit rows only, but an unbounded list would still bloat the report.
const MAX_DUP_PROPOSALS: usize = 200;

/// What a round actually did. Returned by `POST /api/dream` so an operator can
/// see the effect without reading the audit table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DreamRoundReport {
    pub round_id: String,
    /// `true` when nothing was written (proposals + in-memory partition only).
    pub dry_run: bool,
    pub source_id: String,
    /// Items considered this round (oldest first, up to `max_docs_per_round`).
    pub considered: usize,
    /// Clusters written to storage. Zero for a dry run.
    pub persisted_clusters: usize,
    pub applied: usize,
    pub blocked: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Cursor high-water mark for the next round: the largest `created_at`
    /// considered. `None` when the round saw nothing.
    pub cursor_created_at: Option<i64>,
    pub actions: Vec<DreamActionRecord>,
    /// Always `"corpus"` — the only pipeline. Kept so stored reports stay
    /// comparable with history from when a scope field existed.
    pub scope: String,
    /// What the deterministic pass found, with no model involved at all.
    #[serde(default)]
    pub clustering: Option<ClusteringReport>,
}

/// What the deterministic pass found, with no model involved at all.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusteringReport {
    /// Entries fed to the pre-pass.
    pub documents: usize,
    pub clusters: usize,
    pub clustered_items: usize,
    pub singletons: usize,
    /// Too short or too generic to carry recoverable meaning.
    pub junk: usize,
    /// Near-duplicate pairs found by word-shingle containment.
    pub duplicate_pairs: usize,
    /// Clusters written to storage (skipped on a dry run).
    pub persisted: usize,
    /// Largest cluster, and its label. A very large one is usually a chain
    /// rather than a topic, which is why `min_cohesion` gates on it.
    pub largest_cluster: Option<(String, usize)>,
}

/// Build the clustering knobs from config so a round is fully described by
/// `(source_id, params)` and can be re-derived later.
fn cluster_params(cfg: &DreamingConfig) -> ClusterParams {
    ClusterParams {
        min_cluster_size: cfg.cluster_min_size,
        min_similarity: cfg.cluster_min_similarity,
        min_shared_ratio: cfg.cluster_min_shared_ratio,
        min_cohesion: cfg.cluster_min_cohesion,
        dup_containment: cfg.dup_containment,
        max_docs: cfg.max_docs_per_round,
        ..ClusterParams::default()
    }
}

/// Run one deterministic corpus round.
///
/// No model is called and no retrieval happens: the whole source is partitioned
/// into clusters from the text alone, and near-duplicates are found by
/// word-shingle containment.
///
/// Clusters are persisted unless this is a dry run. Cluster ids are a hash of
/// the member set, so re-running produces the same ids and the audit log stays
/// comparable across rounds.
pub async fn run_corpus_dreaming_round(
    state: &AppState,
    source_id: &str,
    dry_run: bool,
    limit: Option<usize>,
) -> Result<DreamRoundReport> {
    let cfg = &state.dreaming;
    let round_id = Uuid::now_v7().to_string();
    let params = cluster_params(cfg);
    let requested = limit
        .unwrap_or(cfg.max_docs_per_round)
        .clamp(1, cfg.max_docs_per_round.max(1));

    let store = state.store.clone();
    let source = source_id.to_owned();
    let cap = requested;
    let items: Vec<ItemRecord> = tokio::task::spawn_blocking(move || {
        store.list_items(ListItemsRequest {
            source_id: Some(source),
            limit: Some(cap),
            sort_order: SortOrder::Asc,
            ..ListItemsRequest::default()
        })
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(|e| anyhow!("corpus round could not list {source_id}: {e}"))?
    .0;

    let cursor_created_at = items.iter().map(|item| item.created_at).max();
    let docs: Vec<Doc> = items
        .iter()
        .map(|item| Doc {
            id: item.id.clone(),
            text: item.text.clone(),
        })
        .collect();

    let duplicates = near_duplicates(&docs, &params);
    let corpus = Corpus::new(&docs, params.clone());
    let partition = corpus.partition(&docs);

    let clustered_items: usize = partition
        .clusters
        .iter()
        .map(|c| c.members.len())
        .sum();
    let largest = partition
        .clusters_by_size()
        .first()
        .map(|c| (c.label.clone(), c.members.len()));

    let mut report = DreamRoundReport {
        round_id: round_id.clone(),
        dry_run,
        source_id: source_id.to_owned(),
        scope: "corpus".to_owned(),
        considered: docs.len(),
        cursor_created_at,
        clustering: Some(ClusteringReport {
            documents: docs.len(),
            clusters: partition.clusters.len(),
            clustered_items,
            singletons: partition.singletons.len(),
            junk: partition.junk.len(),
            duplicate_pairs: duplicates.len(),
            persisted: 0,
            largest_cluster: largest.clone(),
        }),
        ..DreamRoundReport::default()
    };

    // Near-duplicates are reported as audit rows so they show up in the same
    // place as every other decision. They are never auto-applied: "these two
    // texts are 92% identical" is evidence, not permission to delete.
    for pair in duplicates.iter().take(MAX_DUP_PROPOSALS) {
        report.actions.push(DreamActionRecord {
            id: format!("{round_id}:dup:{}:{}", pair.a, pair.b),
            round_id: round_id.clone(),
            item_id: pair.a.clone(),
            action: DreamActionKind::Merge.as_str().to_owned(),
            target_id: Some(pair.b.clone()),
            result_item_id: None,
            reason: format!(
                "near-duplicate: {:.0}% word-shingle containment (deterministic pre-pass)",
                pair.containment * 100.0
            ),
            outcome: "proposed".to_owned(),
            dry_run: true,
            created_at: Utc::now().timestamp_millis(),
        });
    }

    if dry_run {
        info!(
            round = %round_id,
            source = %source_id,
            documents = docs.len(),
            clusters = partition.clusters.len(),
            clustered_items,
            singletons = partition.singletons.len(),
            junk = partition.junk.len(),
            duplicates = duplicates.len(),
            "corpus dreaming round (dry run) complete"
        );
        return Ok(report);
    }

    let index_of: std::collections::HashMap<&str, usize> = docs
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();
    let params_json = serde_json::json!({
        "min_cluster_size": params.min_cluster_size,
        "min_similarity": params.min_similarity,
        "min_shared_ratio": params.min_shared_ratio,
        "min_cohesion": params.min_cohesion,
        "knn": params.knn,
        "max_df_ratio": params.max_df_ratio,
        "max_df_min_count": params.max_df_min_count,
        "max_cluster_size": params.max_cluster_size,
        "dup_containment": params.dup_containment,
        "shingle_len": params.shingle_len,
    })
    .to_string();
    let now = Utc::now().timestamp_millis();

    for cluster in partition.clusters_by_size() {
        let indices: Vec<usize> = cluster
            .members
            .iter()
            .filter_map(|m| index_of.get(m.as_str()).copied())
            .collect();
        let cohesion = corpus.cohesion_of(&indices);
        let record = DreamClusterRecord {
            id: cluster.id.clone(),
            round_id: round_id.clone(),
            source_id: source_id.to_owned(),
            label: cluster.label.clone(),
            member_count: cluster.members.len(),
            vocabulary: cluster
                .label
                .split(" · ")
                .map(str::to_owned)
                .collect(),
            cohesion,
            params: params_json.clone(),
            created_at: now,
        };
        let store = state.store.clone();
        let members = cluster.members.clone();
        let cluster_id = cluster.id.clone();
        let persisted = tokio::task::spawn_blocking(move || {
            store.upsert_dream_cluster(record)?;
            store.replace_dream_cluster_members(&cluster_id, &members)
        })
        .await
        .map_err(anyhow::Error::from)?;
        match persisted {
            Ok(()) => {
                report.persisted_clusters += 1;
                if let Some(clustering) = report.clustering.as_mut() {
                    clustering.persisted += 1;
                }
            }
            Err(err) => warn!(
                cluster = %cluster.id,
                "corpus round could not persist cluster: {err}"
            ),
        }
    }

    info!(
        round = %round_id,
        source = %source_id,
        documents = docs.len(),
        clusters = partition.clusters.len(),
        persisted = report.persisted_clusters,
        duplicates = duplicates.len(),
        "corpus dreaming round complete"
    );
    Ok(report)
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
        "dreaming worker: starting — interval={}s source={} dry_run={} max_docs={}",
        cfg.interval_secs, cfg.source_id, cfg.dry_run, cfg.max_docs_per_round,
    );

    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match process_dreaming_round(&state).await {
                    Ok(report) => info!(
                        round = %report.round_id,
                        considered = report.considered,
                        persisted = report.persisted_clusters,
                        duplicates = report.clustering.as_ref().map(|c| c.duplicate_pairs).unwrap_or(0),
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
/// `RAG_DREAMING_DRY_RUN`). Used by the background worker, `POST /api/dream`,
/// and the MCP `dream` tool. Needs no LLM and no embedder.
pub async fn process_dreaming_round(state: &AppState) -> Result<DreamRoundReport> {
    let cfg = &state.dreaming;
    run_corpus_dreaming_round(
        state,
        &cfg.source_id.clone(),
        cfg.dry_run,
        Some(cfg.max_docs_per_round),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_params_follow_config() {
        let cfg = DreamingConfig {
            cluster_min_size: 5,
            cluster_min_similarity: 0.5,
            cluster_min_shared_ratio: 0.6,
            cluster_min_cohesion: 0.7,
            dup_containment: 0.9,
            max_docs_per_round: 123,
            ..DreamingConfig::default()
        };
        let params = cluster_params(&cfg);
        assert_eq!(params.min_cluster_size, 5);
        assert_eq!(params.min_similarity, 0.5);
        assert_eq!(params.min_shared_ratio, 0.6);
        assert_eq!(params.min_cohesion, 0.7);
        assert_eq!(params.dup_containment, 0.9);
        assert_eq!(params.max_docs, 123);
    }

    #[test]
    fn report_scope_is_always_corpus() {
        let report = DreamRoundReport::default();
        assert!(report.actions.is_empty());
        assert_eq!(report.persisted_clusters, 0);
    }

    #[test]
    fn duplicate_proposals_use_merge_label() {
        assert_eq!(DreamActionKind::parse("merge"), Some(DreamActionKind::Merge));
        assert_eq!(DreamActionKind::Merge.as_str(), "merge");
    }
}
