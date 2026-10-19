use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::{
    DuplicateEdgeGroup, GraphConfig, GraphEdgeRecord, GraphEdgeType, GraphNodeDistance,
    LooseLinkHub, LooseLinkReport, LooseLinkRuleCount, bool_to_sqlite, current_timestamp_millis,
    format_edge_sort_order, parse_json_column,
};

pub(super) fn list_duplicate_edges_internal(
    connection: &Connection,
) -> Result<Vec<DuplicateEdgeGroup>> {
    let mut stmt = connection.prepare(
        "
        SELECT from_item_id, to_item_id
        FROM graph_edges
        GROUP BY from_item_id, to_item_id
        HAVING COUNT(*) > 1
        ",
    )?;

    let pairs = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut groups = Vec::new();
    for pair in pairs {
        let (from, to) = pair?;
        let mut edge_stmt = connection.prepare(
            "
            SELECT id, from_item_id, to_item_id, edge_type, relation, sort_order, weight, directed, metadata, created_at, updated_at
            FROM graph_edges
            WHERE from_item_id = ?1 AND to_item_id = ?2
            ORDER BY sort_order ASC, updated_at DESC, id ASC
            ",
        )?;
        let edges = edge_stmt.query_map(params![from, to], map_graph_edge_row)?;
        let mut group_edges = Vec::new();
        for edge in edges {
            group_edges.push(edge?);
        }
        groups.push(DuplicateEdgeGroup {
            from_item_id: from,
            to_item_id: to,
            edges: group_edges,
        });
    }

    Ok(groups)
}

pub(super) fn list_graph_edges_internal(
    connection: &Connection,
    item_id: Option<&str>,
    edge_type: Option<GraphEdgeType>,
    status: Option<&str>,
) -> Result<Vec<GraphEdgeRecord>> {
    let edge_type = edge_type.map(GraphEdgeType::as_str);
    let mut statement = connection.prepare(
        "
        SELECT
            id,
            from_item_id,
            to_item_id,
            edge_type,
            relation,
            sort_order,
            weight,
            directed,
            metadata,
            created_at,
            updated_at
        FROM graph_edges
        WHERE (?1 IS NULL OR from_item_id = ?1 OR to_item_id = ?1)
          AND (?2 IS NULL OR edge_type = ?2)
          AND (?3 IS NULL OR json_extract(metadata, '$.status') = ?3)
        ORDER BY sort_order ASC, updated_at DESC, id ASC
        ",
    )?;
    let rows = statement.query_map(params![item_id, edge_type, status], map_graph_edge_row)?;

    let mut edges = Vec::new();
    for row in rows {
        edges.push(row?);
    }
    Ok(edges)
}

pub(super) fn list_pairwise_distances_for_ids(
    connection: &Connection,
    ids: &[String],
) -> Result<Vec<GraphNodeDistance>> {
    if ids.len() < 2 {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "
        SELECT
            left_vec.id,
            right_vec.id,
            CAST(vec_distance_L2(left_vec.embedding, right_vec.embedding) AS REAL) AS distance
        FROM vec_items AS left_vec
        JOIN vec_items AS right_vec ON left_vec.id < right_vec.id
        WHERE left_vec.id IN ({placeholders})
          AND right_vec.id IN ({placeholders})
        ORDER BY left_vec.id ASC, right_vec.id ASC
        "
    );

    let mut statement = connection.prepare(&sql)?;
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() * 2);
    for id in ids {
        params_vec.push(id);
    }
    for id in ids {
        params_vec.push(id);
    }

    let rows = statement.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok(GraphNodeDistance {
            from_item_id: row.get(0)?,
            to_item_id: row.get(1)?,
            distance: row.get(2)?,
        })
    })?;

    let mut distances = Vec::new();
    for row in rows {
        distances.push(row?);
    }
    Ok(distances)
}

/// Pull `key` out of an entry's `data` / `metadata` blob and normalize it into
/// a cluster identity.
///
/// Lookup order matches the contextual-embed prefix builder in
/// `api::store_search::build_contextual_embed_prefix`: typed `data` wins over
/// `metadata[key]`, which wins over a nested `metadata.data[key]`. Values are
/// trimmed and lowercased so `"Rust-RAG"` and `"rust-rag "` cluster together.
pub(super) fn extract_metadata_key(
    metadata: Option<&Value>,
    data: Option<&Value>,
    key: &str,
) -> Option<String> {
    let normalize = |value: &str| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_lowercase())
    };
    if let Some(Value::Object(data_map)) = data
        && let Some(found) = data_map.get(key).and_then(Value::as_str)
        && let Some(normalized) = normalize(found)
    {
        return Some(normalized);
    }
    if let Some(Value::Object(map)) = metadata {
        if let Some(found) = map.get(key).and_then(Value::as_str)
            && let Some(normalized) = normalize(found)
        {
            return Some(normalized);
        }
        if let Some(found) = map
            .get("data")
            .and_then(|nested| nested.get(key))
            .and_then(Value::as_str)
            && let Some(normalized) = normalize(found)
        {
            return Some(normalized);
        }
    }
    None
}

fn extract_repo_from_row(metadata_str: Option<&str>, data_str: Option<&str>) -> Option<String> {
    let parse = |raw: Option<&str>| -> Option<Value> {
        raw.and_then(|s| serde_json::from_str::<Value>(s).ok())
    };
    extract_metadata_key(
        parse(metadata_str).as_ref(),
        parse(data_str).as_ref(),
        "repo",
    )
}

// ---------------------------------------------------------------------------
// Loose links ("wiki backlinks from shared metadata")
// ---------------------------------------------------------------------------

/// FNV-1a 64-bit. Used to disambiguate hub ids after sanitization.
/// Hand-rolled rather than `DefaultHasher` because hub ids are persisted —
/// they must be stable across processes and Rust releases, which the std hasher
/// does not guarantee.
fn fnv1a(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Make a string safe for an `ext:{kind}:{key}` id, which
/// `parse_external_ref_id` restricts to `[a-zA-Z0-9_-]+`.
///
/// Values that are already valid are passed through verbatim so hubs stay
/// readable (`ext:repo:rust-rag`). Anything else is sanitized *and* suffixed
/// with a hash of the original — sanitizing alone would silently merge distinct
/// values that collapse to the same replacement text (`src/a-b.rs` and
/// `src/a_b.rs` both become `src-a-b-rs`).
fn sanitize_external_segment(value: &str) -> String {
    let is_valid = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    if is_valid(value) {
        return value.to_owned();
    }
    let mut sanitized = String::with_capacity(value.len() + 9);
    for c in value.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            sanitized.push(c);
        } else {
            sanitized.push('-');
        }
    }
    sanitized.truncate(64);
    format!("{sanitized}-{:08x}", fnv1a(value) as u32)
}

/// Id of the placeholder entry that hubs a metadata cluster. The metadata key
/// becomes the `kind` and the normalized value the `key`, so the id reads as
/// `ext:repo:rust-rag`.
pub(super) fn loose_link_hub_id(metadata_key: &str, value: &str) -> String {
    format!(
        "ext:{}:{}",
        sanitize_external_segment(metadata_key),
        sanitize_external_segment(value)
    )
}

pub(super) struct PlannedLooseEdge {
    pub id: String,
    pub from_item_id: String,
    pub to_item_id: String,
    pub relation: String,
    pub sort_order: String,
    pub weight: f32,
    pub metadata: Value,
}

pub(super) struct LooseLinkPlan {
    pub edges: Vec<PlannedLooseEdge>,
    /// Placeholder endpoints that must exist before `edges` can be inserted
    /// (the graph FKs reference `documents`/`items`).
    pub hubs: Vec<LooseLinkHub>,
    pub report: LooseLinkReport,
}

/// Build the full set of loose-link edges for `rows`.
///
/// For every rule, entries sharing a normalized `metadata_key` value form a
/// cluster. Each cluster member gets an edge to a shared `ext:` hub, which
/// keeps the graph O(n) instead of O(n²) — a 500-entry repo would otherwise
/// produce 124,750 pairwise edges. Reaching the cluster is a depth-2 BFS from
/// any member, and the existing `graph_neighborhood` traversal picks that up
/// with no changes.
///
/// `clique_max_members` additionally emits pairwise edges inside a cluster, but
/// *only* when the whole cluster fits under the cap — an all-or-nothing rule
/// rather than a truncated prefix, so two small clusters stay uniformly
/// connected. `0` disables pairwise edges entirely.
///
/// `sort_order_base` is the current `MAX(sort_order)` in the table; new edges
/// append after it so loose links never jump ahead of hand-curated ones in the
/// CMS tree, which renders manual edges as a hierarchy ordered by `sort_order`.
pub(super) fn plan_loose_links(
    rows: &[(String, Option<Value>, Option<Value>)],
    rules: &[super::LooseLinkRule],
    clique_max_members: usize,
    sort_order_base: i64,
) -> LooseLinkPlan {
    let timestamp = current_timestamp_millis().unwrap_or(0);
    let mut edges: Vec<PlannedLooseEdge> = Vec::new();
    let mut hubs: Vec<LooseLinkHub> = Vec::new();
    let mut report = LooseLinkReport::default();
    let mut seen_hubs: HashSet<String> = HashSet::new();

    // Sorted for deterministic output: HashMap iteration order would make
    // sort_order and edge ids vary run to run.
    for rule in rules {
        if rule.weight <= 0.0 || rule.metadata_key.trim().is_empty() {
            continue;
        }
        let mut clusters: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (id, metadata, data) in rows {
            if let Some(value) =
                extract_metadata_key(metadata.as_ref(), data.as_ref(), &rule.metadata_key)
            {
                clusters.entry(value).or_default().push(id.clone());
            }
        }
        report.rule_matches.push(LooseLinkRuleCount {
            metadata_key: rule.metadata_key.clone(),
            relation: rule.relation.clone(),
            distinct_values: clusters.len(),
            linked_entries: clusters.values().map(Vec::len).sum(),
        });

        for (value, mut members) in clusters {
            members.sort();
            members.dedup();
            if members.len() < 2 {
                report.clusters_skipped_singleton += 1;
                continue;
            }
            report.clusters += 1;

            let hub_id = loose_link_hub_id(&rule.metadata_key, &value);
            if seen_hubs.insert(hub_id.clone()) {
                hubs.push(LooseLinkHub {
                    id: hub_id.clone(),
                    metadata_key: rule.metadata_key.clone(),
                    value: value.clone(),
                    members: members.len(),
                });
            }

            for member in &members {
                let (from_item_id, to_item_id) = canonical_edge_pair(member, &hub_id);
                push_planned_edge(
                    &mut edges,
                    from_item_id,
                    to_item_id,
                    &EdgeShape {
                        relation: &rule.relation,
                        weight: rule.weight,
                        matched: serde_json::json!({
                            "key": rule.metadata_key,
                            "value": value,
                        }),
                    },
                    timestamp,
                    sort_order_base,
                );
                report.hub_edges += 1;
            }

            if clique_max_members > 0 && members.len() <= clique_max_members {
                for (index, left) in members.iter().enumerate() {
                    for right in &members[index + 1..] {
                        let (from_item_id, to_item_id) = canonical_edge_pair(left, right);
                        push_planned_edge(
                            &mut edges,
                            from_item_id,
                            to_item_id,
                            &EdgeShape {
                                relation: &rule.relation,
                                weight: rule.weight,
                                matched: serde_json::json!({
                                    "key": rule.metadata_key,
                                    "value": value,
                                    "link_kind": "clique",
                                }),
                            },
                            timestamp,
                            sort_order_base,
                        );
                        report.clique_edges += 1;
                    }
                }
            }
        }
    }

    report.edges_committed = edges.len();
    report.hubs = hubs.clone();
    LooseLinkPlan {
        edges,
        hubs,
        report,
    }
}

/// Everything about an edge that comes from the rule rather than the endpoints.
struct EdgeShape<'a> {
    relation: &'a str,
    weight: f32,
    /// What matched — `{"key": "repo", "value": "rust-rag"}`.
    matched: Value,
}

fn push_planned_edge(
    edges: &mut Vec<PlannedLooseEdge>,
    from_item_id: String,
    to_item_id: String,
    shape: &EdgeShape<'_>,
    timestamp: i64,
    sort_order_base: i64,
) {
    let index = edges.len() as i64;
    // `relation` is part of the id because two rules can legitimately produce
    // different relations for the same (from, to) pair in one pass — a
    // timestamp-only id would collide and blow up the primary key.
    edges.push(PlannedLooseEdge {
        id: format!("loose_link:{timestamp}:{index}:{}", shape.relation),
        from_item_id,
        to_item_id,
        relation: shape.relation.to_owned(),
        // +1 so the first edge clears `sort_order_base` instead of colliding
        // with whatever already sits at it.
        sort_order: format_edge_sort_order(
            sort_order_base + (index + 1) * super::EDGE_SORT_ORDER_STEP,
        ),
        weight: shape.weight,
        metadata: serde_json::json!({
            "source": super::LOOSE_LINK_SOURCE,
            "status": "confirmed",
            "auto": true,
            "extra": shape.matched,
        }),
    });
}

pub(super) fn rebuild_similarity_graph_locked(
    connection: &mut Connection,
    graph_config: GraphConfig,
) -> Result<usize> {
    if !graph_config.enabled {
        return Ok(0);
    }

    let mut item_statement = connection.prepare(
        "
        SELECT items.id, items.metadata, items.data
        FROM items
        JOIN vec_items ON vec_items.id = items.id
        ORDER BY items.id ASC
        ",
    )?;
    let item_rows = item_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(item_statement);

    let mut item_ids = Vec::with_capacity(item_rows.len());
    let mut item_repos: HashMap<String, Option<String>> = HashMap::with_capacity(item_rows.len());
    for (id, meta, data) in item_rows {
        let repo = extract_repo_from_row(meta.as_deref(), data.as_deref());
        item_repos.insert(id.clone(), repo);
        item_ids.push(id);
    }

    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM graph_edges WHERE edge_type = 'similarity'", [])?;

    let mut inserted_pairs = HashSet::new();
    let timestamp = current_timestamp_millis()?;
    let mut inserted = 0usize;

    for item_id in &item_ids {
        let base_repo = item_repos.get(item_id).and_then(|r| r.as_ref());
        let candidate_limit =
            (graph_config.similarity_top_k * 2).max(graph_config.similarity_top_k + 10);
        let candidates = {
            let mut statement = transaction.prepare(
                "
                SELECT
                    other.id,
                    CAST(vec_distance_L2(base.embedding, other_vec.embedding) AS REAL) AS distance
                FROM vec_items AS base
                JOIN items AS base_item ON base_item.id = base.id
                JOIN vec_items AS other_vec ON other_vec.id != base.id
                JOIN items AS other ON other.id = other_vec.id
                WHERE base.id = ?1
                  AND (?2 = 1 OR other.source_id = base_item.source_id)
                ORDER BY distance ASC, other.id ASC
                LIMIT ?3
                ",
            )?;
            let rows = statement.query_map(
                params![
                    item_id,
                    bool_to_sqlite(graph_config.cross_source),
                    candidate_limit as i64
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?)),
            )?;

            let mut candidates = Vec::new();
            for row in rows {
                let (other_id, raw_dist) = row?;
                let other_repo = item_repos.get(&other_id).and_then(|r| r.as_ref());
                let (adjusted_dist, same_repo) = match (base_repo, other_repo) {
                    (Some(b), Some(o)) if b == o => (raw_dist * 0.80, Some(true)),
                    (Some(b), Some(o)) if b != o => (raw_dist * 1.15, Some(false)),
                    _ => (raw_dist, None),
                };
                candidates.push((other_id, adjusted_dist, raw_dist, same_repo));
            }
            candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(graph_config.similarity_top_k);
            candidates
        };

        for (other_id, distance, raw_distance, same_repo) in candidates {
            if distance > graph_config.similarity_max_distance {
                continue;
            }
            let (from_item_id, to_item_id) = canonical_edge_pair(item_id, &other_id);
            if !inserted_pairs.insert((from_item_id.clone(), to_item_id.clone())) {
                continue;
            }

            let weight = 1.0 / (1.0 + distance);
            let mut metadata = serde_json::json!({
                "distance": distance,
                "raw_distance": raw_distance,
            });
            if let Some(sr) = same_repo {
                metadata["same_repo"] = serde_json::json!(sr);
            }
            transaction.execute(
                "
                INSERT INTO graph_edges (
                    id, from_item_id, to_item_id, edge_type, relation, sort_order, weight, directed, metadata, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, 'similarity', NULL, ?4, ?5, 0, ?6, ?7, ?7)
                ",
                params![
                    format!("similarity:{from_item_id}:{to_item_id}"),
                    from_item_id,
                    to_item_id,
                    format_edge_sort_order(inserted as i64 + 1),
                    weight,
                    serde_json::to_string(&metadata)?,
                    timestamp
                ],
            )?;
            inserted += 1;
        }
    }

    transaction.commit()?;
    Ok(inserted)
}

pub(super) fn map_graph_edge_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GraphEdgeRecord> {
    let edge_type_raw = row.get::<_, String>(3)?;
    let edge_type = GraphEdgeType::from_str(&edge_type_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;

    Ok(GraphEdgeRecord {
        id: row.get(0)?,
        from_item_id: row.get(1)?,
        to_item_id: row.get(2)?,
        edge_type,
        relation: row.get(4)?,
        sort_order: row.get(5)?,
        weight: row.get(6)?,
        directed: row.get::<_, i64>(7)? != 0,
        metadata: parse_json_column(row.get::<_, String>(8)?, 8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(super) fn canonical_edge_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
}

#[cfg(test)]
mod loose_link_tests {
    use super::*;
    use crate::db::LooseLinkRule;

    fn rule(key: &str, relation: &str) -> LooseLinkRule {
        LooseLinkRule {
            metadata_key: key.to_owned(),
            relation: relation.to_owned(),
            weight: 0.6,
        }
    }

    fn item(id: &str, repo: &str) -> (String, Option<Value>, Option<Value>) {
        (id.to_owned(), Some(serde_json::json!({"repo": repo})), None)
    }

    #[test]
    fn clusters_by_normalized_repo() {
        // "Rust-RAG" and " rust-rag " must land in the same cluster.
        let rows = vec![
            item("a", "Rust-RAG"),
            item("b", " rust-rag "),
            item("c", "other"),
        ];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 0, 0);

        assert_eq!(plan.report.clusters, 1);
        assert_eq!(plan.report.clusters_skipped_singleton, 1);
        assert_eq!(plan.report.hub_edges, 2);
        assert_eq!(plan.report.clique_edges, 0);
        assert_eq!(plan.hubs.len(), 1);
        assert_eq!(plan.hubs[0].id, "ext:repo:rust-rag");
        assert_eq!(plan.hubs[0].members, 2);
    }

    #[test]
    fn hub_edges_reach_every_cluster_member() {
        let rows = vec![
            item("a", "rust-rag"),
            item("b", "rust-rag"),
            item("c", "rust-rag"),
        ];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 0, 0);

        // O(n), not O(n²): three members produce three hub edges, not three pairs.
        assert_eq!(plan.edges.len(), 3);
        for edge in &plan.edges {
            assert!(
                edge.from_item_id == "ext:repo:rust-rag" || edge.to_item_id == "ext:repo:rust-rag"
            );
            assert_eq!(edge.metadata["source"], "loose_links");
            // Weight must stay positive: negative weights are the anti-edge
            // signal and would actively penalize search results.
            assert!(edge.weight > 0.0);
        }
    }

    #[test]
    fn singleton_clusters_produce_no_edges_or_hubs() {
        let rows = vec![item("a", "rust-rag")];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 0, 0);

        assert!(plan.edges.is_empty());
        assert!(plan.hubs.is_empty());
        assert_eq!(plan.report.clusters_skipped_singleton, 1);
    }

    #[test]
    fn clique_is_all_or_nothing_per_cluster() {
        let rows = vec![item("a", "small"), item("b", "small")];

        // Under the cap: 2 hub edges + the 1 pair.
        let capped = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 4, 0);
        assert_eq!(capped.report.hub_edges, 2);
        assert_eq!(capped.report.clique_edges, 1);

        // At the cap: still complete.
        let at_cap = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 2, 0);
        assert_eq!(at_cap.report.clique_edges, 1);

        // Over the cap: no partial prefix, hub edges only.
        let over = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 1, 0);
        assert_eq!(over.report.clique_edges, 0);
        assert_eq!(over.report.hub_edges, 2);
    }

    #[test]
    fn clique_edges_are_canonicalized_and_never_self_loops() {
        let rows = vec![item("a", "x"), item("b", "x"), item("c", "x")];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 3, 0);

        assert_eq!(plan.report.clique_edges, 3);
        for edge in plan.edges.iter().filter(|e| e.to_item_id != "ext:repo:x") {
            assert_ne!(edge.from_item_id, edge.to_item_id);
            assert!(edge.from_item_id < edge.to_item_id);
        }
    }

    #[test]
    fn edge_ids_are_unique_across_relations() {
        // Two rules over the same key produce different relations for the same
        // pair; a timestamp-only id would collide and blow up the PK.
        let rows = vec![item("a", "x"), item("b", "x")];
        let plan = plan_loose_links(
            &rows,
            &[rule("repo", "shares_repo"), rule("repo", "relates_to")],
            0,
            0,
        );
        let ids: HashSet<&str> = plan.edges.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), plan.edges.len());
        // Same cluster, same hub — the hub is registered once.
        assert_eq!(plan.hubs.len(), 1);
    }

    #[test]
    fn rules_with_invalid_config_are_skipped() {
        let rows = vec![item("a", "x"), item("b", "x")];
        let zero_weight = LooseLinkRule {
            metadata_key: "repo".to_owned(),
            relation: "shares_repo".to_owned(),
            weight: 0.0,
        };
        let blank_key = rule("  ", "shares_repo");
        let plan = plan_loose_links(&rows, &[zero_weight, blank_key], 0, 0);
        assert!(plan.edges.is_empty());
    }

    #[test]
    fn sort_order_appends_after_the_existing_maximum() {
        let rows = vec![item("a", "x"), item("b", "x")];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 0, 5000);

        let orders: Vec<i64> = plan
            .edges
            .iter()
            .map(|e| e.sort_order.parse::<i64>().expect("sort_order is numeric"))
            .collect();
        assert!(orders.iter().all(|order| *order > 5000));
        assert!(orders.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn plan_is_deterministic_across_runs() {
        let rows: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| item(id, "rust-rag"))
            .collect();
        let first = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 3, 0);
        let second = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 3, 0);
        let ids = |plan: &LooseLinkPlan| -> Vec<String> {
            plan.edges
                .iter()
                .map(|e| format!("{}|{}|{}", e.from_item_id, e.to_item_id, e.sort_order))
                .collect()
        };
        assert_eq!(ids(&first), ids(&second));
    }

    #[test]
    fn data_column_wins_over_metadata() {
        let rows = vec![
            (
                "a".to_owned(),
                Some(serde_json::json!({"repo": "wrong"})),
                Some(serde_json::json!({"repo": "right"})),
            ),
            (
                "b".to_owned(),
                Some(serde_json::json!({"repo": "right"})),
                None,
            ),
        ];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 0, 0);
        assert_eq!(plan.report.clusters, 1);
        assert_eq!(plan.hubs[0].value, "right");
    }

    #[test]
    fn nested_metadata_data_is_a_fallback() {
        let rows = vec![
            (
                "a".to_owned(),
                Some(serde_json::json!({"data": {"repo": "x"}})),
                None,
            ),
            (
                "b".to_owned(),
                Some(serde_json::json!({"data": {"repo": "x"}})),
                None,
            ),
        ];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 0, 0);
        assert_eq!(plan.report.clusters, 1);
    }

    #[test]
    fn hub_ids_are_valid_external_refs() {
        // Values with characters `parse_external_ref_id` rejects must still
        // produce a usable id, and distinct values must not collide.
        let rows = vec![
            (
                "a".to_owned(),
                Some(serde_json::json!({"repo_path": "src/a-b.rs"})),
                None,
            ),
            (
                "b".to_owned(),
                Some(serde_json::json!({"repo_path": "src/a_b.rs"})),
                None,
            ),
            (
                "c".to_owned(),
                Some(serde_json::json!({"repo_path": "src/a-b.rs"})),
                None,
            ),
            (
                "d".to_owned(),
                Some(serde_json::json!({"repo_path": "src/a_b.rs"})),
                None,
            ),
        ];
        let plan = plan_loose_links(&rows, &[rule("repo_path", "relates_to")], 0, 0);
        assert_eq!(plan.report.clusters, 2);
        let ids: HashSet<&str> = plan.hubs.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        for id in ids {
            assert!(
                super::super::parse_external_ref_id(id).is_some(),
                "{id} is not a valid ext ref"
            );
        }
    }

    #[test]
    fn clean_hub_ids_stay_readable() {
        assert_eq!(loose_link_hub_id("repo", "rust-rag"), "ext:repo:rust-rag");
        assert_eq!(
            loose_link_hub_id("repo", "Rust_RAG-2"),
            "ext:repo:Rust_RAG-2"
        );
    }

    #[test]
    fn sanitization_is_stable_across_calls() {
        // Hub ids are persisted, so the hash suffix must not drift.
        let value = "org/team/project";
        assert_eq!(
            sanitize_external_segment(value),
            sanitize_external_segment(value)
        );
        assert_ne!(
            sanitize_external_segment("org/team/project"),
            sanitize_external_segment("org-team-project")
        );
    }

    #[test]
    fn report_counts_match_the_planned_edges() {
        let rows = vec![
            item("a", "x"),
            item("b", "x"),
            item("c", "y"),
            item("d", "y"),
            item("e", "y"),
        ];
        let plan = plan_loose_links(&rows, &[rule("repo", "shares_repo")], 3, 0);
        assert_eq!(
            plan.report.hub_edges + plan.report.clique_edges,
            plan.edges.len()
        );
        assert_eq!(plan.report.edges_committed, plan.edges.len());
        assert_eq!(plan.report.clusters, 2);
        // cluster "y" is 3 members → 3 hubs + 3 pairs; cluster "x" → 2 hubs + 1 pair.
        assert_eq!(plan.report.hub_edges, 5);
        assert_eq!(plan.report.clique_edges, 4);
        assert_eq!(plan.report.rule_matches[0].distinct_values, 2);
        assert_eq!(plan.report.rule_matches[0].linked_entries, 5);
    }
}
