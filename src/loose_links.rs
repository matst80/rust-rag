//! Background worker for metadata-derived "loose links".
//!
//! Two entries carrying the same value under a configured metadata key (e.g.
//! both `repo: rust-rag`) get linked, so the wiki shows backlinks between
//! entries that never resemble each other in text.
//!
//! Distinct from `ontology`, which is LLM-driven and semantic: this pass is
//! deterministic, needs no embedder and no model, and a metadata-only table
//! walk that is cheap enough to redo from scratch on every tick.

use crate::{
    config::LooseLinksConfig,
    db::{LOOSE_LINK_SOURCE, LooseLinkReport, LooseLinkRule, LooseLinkRuleCount, VectorStore},
};
use anyhow::Result;
use std::{sync::Arc, time::Duration};
use tokio::time::interval;
use tracing::{error, info, warn};

/// Flattened view of one rebuild, for the worker log line and for tests.
#[derive(Debug, Clone, Default)]
pub struct LooseLinksRunReport {
    pub clusters: usize,
    pub edges: usize,
    /// The recomputed plan matched what was already stored; nothing was written.
    pub unchanged: bool,
    pub rule_matches: Vec<LooseLinkRuleCount>,
}

impl From<LooseLinkReport> for LooseLinksRunReport {
    fn from(report: LooseLinkReport) -> Self {
        Self {
            clusters: report.clusters,
            edges: report.edges_committed,
            unchanged: report.unchanged,
            rule_matches: report.rule_matches,
        }
    }
}

/// Run one rebuild with the server-configured rules. Shared by the worker loop
/// and the admin endpoint so both take the same code path.
pub async fn run_once(
    store: &Arc<dyn VectorStore>,
    cfg: &LooseLinksConfig,
) -> Result<LooseLinksRunReport> {
    if cfg.rules.is_empty() {
        warn!("loose links: no rules configured, nothing to do");
        return Ok(LooseLinksRunReport::default());
    }

    let store = store.clone();
    let rules: Vec<LooseLinkRule> = cfg.rules.clone();
    let clique_max_members = cfg.clique_max_members;
    let report =
        tokio::task::spawn_blocking(move || store.rebuild_loose_links(&rules, clique_max_members))
            .await?
            .map_err(|err| anyhow::anyhow!("loose links: rebuild failed: {err}"))?;

    Ok(report.into())
}

pub async fn run_loose_links_worker(
    store: Arc<dyn VectorStore>,
    cfg: LooseLinksConfig,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    if !cfg.enabled {
        info!("loose links worker: disabled");
        return;
    }
    if cfg.rules.is_empty() {
        warn!("loose links worker: enabled but no rules configured — disabling");
        return;
    }
    if cfg.interval_secs == 0 {
        warn!("loose links worker: interval is 0s — disabling");
        return;
    }

    let rules: Vec<String> = cfg
        .rules
        .iter()
        .map(|rule| format!("{}:{}", rule.metadata_key, rule.relation))
        .collect();
    info!(
        "loose links worker: starting — interval={}s clique_max={} rules={:?}",
        cfg.interval_secs, cfg.clique_max_members, rules
    );

    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match run_once(&store, &cfg).await {
                    // An unchanged plan means nothing was written; staying quiet
                    // keeps the log readable on a long-running idle instance.
                    Ok(report) if report.unchanged => {}
                    Ok(report) => info!(
                        clusters = report.clusters,
                        edges = report.edges,
                        "rebuilt loose-link edges"
                    ),
                    Err(err) => error!("loose links worker: rebuild error: {err}"),
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("loose links worker: shutdown signal received, exiting loop");
                    break;
                }
            }
        }
    }
}

/// Source tag used on every edge this worker writes. Re-exported so operators
/// can grep the same constant the writer uses.
pub const EDGE_SOURCE: &str = LOOSE_LINK_SOURCE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{GraphConfig, GraphEdgeType, SqliteVectorStore};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn run_report_projects_the_store_report() {
        let store = LooseLinkReport {
            clusters: 3,
            clusters_skipped_singleton: 1,
            hub_edges: 9,
            clique_edges: 1,
            edges_committed: 10,
            unchanged: true,
            rule_matches: vec![LooseLinkRuleCount {
                metadata_key: "repo".to_owned(),
                relation: "shares_repo".to_owned(),
                distinct_values: 3,
                linked_entries: 10,
            }],
            hubs: Vec::new(),
        };
        let run = LooseLinksRunReport::from(store);
        assert_eq!(run.clusters, 3);
        assert_eq!(run.edges, 10);
        assert!(run.unchanged);
        assert_eq!(run.rule_matches.len(), 1);
    }

    #[test]
    fn edge_source_matches_the_writer_constant() {
        assert_eq!(EDGE_SOURCE, "loose_links");
    }

    fn store() -> Arc<dyn VectorStore> {
        static NEXT_DB_ID: AtomicUsize = AtomicUsize::new(0);
        let db_id = NEXT_DB_ID.fetch_add(1, Ordering::Relaxed);
        let uri = format!("file:loose-links-worker-{db_id}?mode=memory&cache=shared");
        Arc::new(
            SqliteVectorStore::connect_uri(
                &uri,
                3,
                GraphConfig {
                    enabled: true,
                    ..GraphConfig::default()
                },
            )
            .expect("in-memory sqlite store should initialize"),
        )
    }

    fn put(store: &Arc<dyn VectorStore>, id: &str, repo: &str) {
        store
            .upsert_item(
                crate::db::ItemRecord {
                    id: id.to_owned(),
                    text: format!("body of {id}"),
                    metadata: serde_json::json!({"repo": repo}),
                    source_id: "knowledge".to_owned(),
                    created_at: 1000,
                    updated_at: 1000,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                    dreamed_at: None,
                },
                &[1.0, 0.0, 0.0],
            )
            .expect("seeding item");
    }

    fn count_loose_edges(store: &Arc<dyn VectorStore>) -> usize {
        store
            .list_graph_edges(None, Some(GraphEdgeType::Manual), None)
            .expect("listing edges")
            .iter()
            .filter(|edge| {
                edge.metadata.get("source").and_then(|v| v.as_str()) == Some(LOOSE_LINK_SOURCE)
            })
            .count()
    }

    #[tokio::test]
    async fn disabled_worker_returns_immediately() {
        let cfg = LooseLinksConfig {
            enabled: false,
            ..LooseLinksConfig::default()
        };
        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(run_loose_links_worker(store(), cfg, rx));
        // Would hang if the guard didn't fire before the loop.
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("disabled worker must return")
            .expect("worker panicked");
        assert!(!*tx.borrow());
    }

    #[tokio::test]
    async fn enabled_worker_without_rules_returns_immediately() {
        let cfg = LooseLinksConfig {
            enabled: true,
            rules: Vec::new(),
            ..LooseLinksConfig::default()
        };
        let (_tx, rx) = tokio::sync::watch::channel(false);
        tokio::time::timeout(
            Duration::from_secs(5),
            run_loose_links_worker(store(), cfg, rx),
        )
        .await
        .expect("ruleless worker must return");
    }

    #[tokio::test]
    async fn enabled_worker_with_zero_interval_returns_immediately() {
        // `tokio::time::interval(0)` panics, so the guard must fire first.
        let cfg = LooseLinksConfig {
            enabled: true,
            interval_secs: 0,
            ..LooseLinksConfig::default()
        };
        let (_tx, rx) = tokio::sync::watch::channel(false);
        tokio::time::timeout(
            Duration::from_secs(5),
            run_loose_links_worker(store(), cfg, rx),
        )
        .await
        .expect("zero-interval worker must return");
    }

    #[tokio::test]
    async fn worker_stops_on_shutdown_signal() {
        let cfg = LooseLinksConfig {
            enabled: true,
            interval_secs: 3_600,
            ..LooseLinksConfig::default()
        };
        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(run_loose_links_worker(store(), cfg, rx));

        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(true).unwrap();

        let joined = tokio::time::timeout(Duration::from_secs(5), handle).await;
        assert!(
            joined.is_ok(),
            "worker did not exit after the shutdown signal"
        );
    }

    #[tokio::test]
    async fn first_tick_builds_edges_and_second_is_a_no_op() {
        let store = store();
        put(&store, "a", "rust-rag");
        put(&store, "b", "rust-rag");
        put(&store, "c", "other");
        let cfg = LooseLinksConfig::default();

        let first = run_once(&store, &cfg).await.expect("first run");
        assert!(!first.unchanged);
        assert_eq!(first.clusters, 1);
        assert_eq!(first.edges, 2);
        assert_eq!(count_loose_edges(&store), 2);
        assert_eq!(first.rule_matches.len(), 1);
        assert_eq!(first.rule_matches[0].metadata_key, "repo");
        assert_eq!(first.rule_matches[0].distinct_values, 2);

        // Idle ticks must not churn the table.
        let second = run_once(&store, &cfg).await.expect("second run");
        assert!(second.unchanged, "idle tick reported a write");
        assert_eq!(count_loose_edges(&store), 2);
    }

    #[tokio::test]
    async fn loop_rebuilds_on_its_own() {
        let store = store();
        put(&store, "a", "rust-rag");
        put(&store, "b", "rust-rag");
        let cfg = LooseLinksConfig {
            enabled: true,
            interval_secs: 1,
            ..LooseLinksConfig::default()
        };
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(run_loose_links_worker(store.clone(), cfg, rx));

        // The first tick fires immediately.
        let deadline = Duration::from_secs(10);
        tokio::time::timeout(deadline, async {
            while count_loose_edges(&store) < 2 {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("worker never built the loose links");

        handle.abort();
    }

    #[tokio::test]
    async fn run_once_is_a_no_op_without_rules() {
        let cfg = LooseLinksConfig {
            rules: Vec::new(),
            ..LooseLinksConfig::default()
        };
        let store = store();
        let report = run_once(&store, &cfg).await.expect("run_once");
        assert_eq!(report.clusters, 0);
        assert_eq!(report.edges, 0);
        assert!(report.rule_matches.is_empty());
        assert_eq!(count_loose_edges(&store), 0);
    }

    #[tokio::test]
    async fn respects_the_configured_clique_cap() {
        let store = store();
        for id in ["a", "b", "c"] {
            put(&store, id, "rust-rag");
        }
        let cfg = LooseLinksConfig {
            clique_max_members: 3,
            ..LooseLinksConfig::default()
        };

        let report = run_once(&store, &cfg).await.expect("run_once");
        // 3 hub edges + 3 pairwise edges.
        assert_eq!(report.edges, 6);
    }
}
