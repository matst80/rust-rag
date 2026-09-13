//! Harness graph domain: node types, edge relation conventions, and the
//! cockpit tree assembly behind `GET /api/harness/tree`.
//!
//! Harness nodes are typed entries (`type` + schema-validated `data`) stored
//! through the regular `/api/store` pipeline; structural relationships are
//! manual directed graph edges. This module only adds what cannot be composed
//! from existing endpoints: resolving the latest `harness_audit` verdict per
//! node and computing cockpit status badges.

use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Query, State},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::db::{GraphEdgeRecord, GraphEdgeType, ItemRecord, ListItemsRequest, SortOrder};

use super::ApiError;

pub const HARNESS_DOC: &str = "harness_doc";
pub const HARNESS_PLAN: &str = "harness_plan";
pub const HARNESS_SPRINT: &str = "harness_sprint";
pub const HARNESS_TODO: &str = "harness_todo";
pub const HARNESS_AGENT: &str = "harness_agent";
pub const HARNESS_STREAM: &str = "harness_stream";
pub const HARNESS_AUDIT: &str = "harness_audit";

/// POC-domain node types: one repo aggregates POC sessions; each session
/// raises promoted memories (decisions, risks, …) as individual nodes.
pub const HARNESS_REPO: &str = "harness_repo";
pub const HARNESS_POC: &str = "harness_poc";
pub const HARNESS_DECISION: &str = "harness_decision";
pub const HARNESS_RISK: &str = "harness_risk";
pub const HARNESS_COMPLIANCE: &str = "harness_compliance";
pub const HARNESS_RESOURCE: &str = "harness_resource";
pub const HARNESS_SCALING: &str = "harness_scaling";
pub const HARNESS_VALIDATION: &str = "harness_validation";
pub const HARNESS_ROLLOUT: &str = "harness_rollout";

/// Every harness node type, including audit verdicts and POC-domain nodes.
pub const HARNESS_NODE_TYPES: [&str; 16] = [
    HARNESS_DOC,
    HARNESS_PLAN,
    HARNESS_SPRINT,
    HARNESS_TODO,
    HARNESS_AGENT,
    HARNESS_STREAM,
    HARNESS_AUDIT,
    HARNESS_REPO,
    HARNESS_POC,
    HARNESS_DECISION,
    HARNESS_RISK,
    HARNESS_COMPLIANCE,
    HARNESS_RESOURCE,
    HARNESS_SCALING,
    HARNESS_VALIDATION,
    HARNESS_ROLLOUT,
];

/// Structural edge relations between harness nodes. Stored as manual directed
/// edges (`POST /admin/graph/edges` with `directed: true`).
pub const REL_GOVERNED_BY: &str = "GOVERNED_BY";
pub const REL_BREAKS_INTO: &str = "BREAKS_INTO";
pub const REL_CONTAINS_TODO: &str = "CONTAINS_TODO";
pub const REL_ENFORCES_DOC: &str = "ENFORCES_DOC";
pub const REL_DELEGATES_TO: &str = "DELEGATES_TO";
pub const REL_MUTATES_STREAM: &str = "MUTATES_STREAM";
pub const REL_CONFLICTS_WITH: &str = "CONFLICTS_WITH";
/// Audit verdict node → audited target.
pub const REL_AUDITED: &str = "AUDITED";

/// POC-domain relations: repo → poc session, session → promoted memory,
/// risk → validation, rollout plan → resource, decision → superseded decision.
pub const REL_HAD_POC: &str = "HAD_POC";
pub const REL_RAISED: &str = "RAISED";
pub const REL_ADDRESSED_BY: &str = "ADDRESSED_BY";
pub const REL_REQUIRES: &str = "REQUIRES";
pub const REL_SUPERSEDES: &str = "SUPERSEDES";

pub const HARNESS_RELATIONS: [&str; 13] = [
    REL_GOVERNED_BY,
    REL_BREAKS_INTO,
    REL_CONTAINS_TODO,
    REL_ENFORCES_DOC,
    REL_DELEGATES_TO,
    REL_MUTATES_STREAM,
    REL_CONFLICTS_WITH,
    REL_AUDITED,
    REL_HAD_POC,
    REL_RAISED,
    REL_ADDRESSED_BY,
    REL_REQUIRES,
    REL_SUPERSEDES,
];

/// Relations that anchor a node to a governing doc. A plan/todo without any
/// of these is "unanchored" (yellow badge when it has no audit yet).
const ANCHOR_RELATIONS: [&str; 2] = [REL_GOVERNED_BY, REL_ENFORCES_DOC];

/// Node types that can carry a status badge in the cockpit.
const AUDITABLE_TYPES: [&str; 4] = [HARNESS_DOC, HARNESS_PLAN, HARNESS_SPRINT, HARNESS_TODO];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Badge {
    Green,
    Yellow,
    Red,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct AuditViolation {
    pub doc_id: String,
    pub title: String,
    pub violation_reason: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SuggestedEdit {
    pub doc_id: String,
    pub suggestion: String,
}

/// Latest audit verdict resolved for a node.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AuditVerdict {
    pub audit_id: String,
    pub passed: bool,
    pub score: f64,
    pub violations: Vec<AuditViolation>,
    pub unanchored_assumptions: Vec<String>,
    #[serde(default)]
    pub suggested_edits: Vec<SuggestedEdit>,
    pub audited_at: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct HarnessTreeNode {
    pub id: String,
    pub type_name: String,
    pub title: String,
    pub state: Option<String>,
    pub source_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Cockpit status badge. `None` (rendered grey) for unaudited-but-anchored
    /// nodes and for types that never carry a badge (agent, stream).
    pub badge: Option<Badge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<AuditVerdict>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct HarnessTreeEdge {
    pub id: String,
    pub from_item_id: String,
    pub to_item_id: String,
    pub relation: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct HarnessTreeResponse {
    pub nodes: Vec<HarnessTreeNode>,
    pub edges: Vec<HarnessTreeEdge>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct HarnessTreeQuery {
    /// Restrict the tree to one namespace. Omit to assemble across all.
    pub source_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuditData {
    target_id: String,
    passed: bool,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    violations: Vec<AuditViolation>,
    #[serde(default)]
    unanchored_assumptions: Vec<String>,
    #[serde(default)]
    suggested_edits: Vec<SuggestedEdit>,
    #[serde(default)]
    audited_at: Option<i64>,
}

fn display_title(item: &ItemRecord) -> String {
    if let Some(data) = &item.data {
        for key in ["title", "name", "goal", "role", "stream_name", "summary"] {
            if let Some(t) = data.get(key).and_then(|v| v.as_str()) {
                if !t.trim().is_empty() {
                    return t.to_owned();
                }
            }
        }
    }
    item.text.lines().next().unwrap_or(&item.id).to_owned()
}

fn data_state(item: &ItemRecord) -> Option<String> {
    item.data
        .as_ref()
        .and_then(|d| d.get("state"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

fn parse_audit(item: &ItemRecord) -> Option<(AuditData, i64)> {
    let data = item.data.as_ref()?;
    let audit: AuditData = serde_json::from_value(data.clone()).ok()?;
    let audited_at = audit.audited_at.unwrap_or(item.updated_at);
    Some((audit, audited_at))
}

/// Resolve the latest audit per target id (newest `updated_at` wins, id as a
/// deterministic tie-break) and compute badges.
pub fn assemble_tree(items: Vec<ItemRecord>, edges: Vec<GraphEdgeRecord>) -> HarnessTreeResponse {
    let node_ids: HashSet<String> = items.iter().map(|i| i.id.clone()).collect();

    let mut latest_audit: HashMap<String, (&ItemRecord, i64)> = HashMap::new();
    let mut anchored: HashSet<String> = HashSet::new();
    for edge in &edges {
        if !node_ids.contains(&edge.from_item_id) || !node_ids.contains(&edge.to_item_id) {
            continue;
        }
        if let Some(relation) = edge.relation.as_deref() {
            if ANCHOR_RELATIONS.contains(&relation) {
                anchored.insert(edge.from_item_id.clone());
            }
        }
    }

    for item in &items {
        if item.type_name.as_deref() != Some(HARNESS_AUDIT) {
            continue;
        }
        let Some((audit, audited_at)) = parse_audit(item) else {
            continue;
        };
        let entry = latest_audit.entry(audit.target_id.clone());
        entry
            .and_modify(|(current, current_at)| {
                if (*current_at, current.id.as_str()) < (audited_at, item.id.as_str()) {
                    *current = item;
                    *current_at = audited_at;
                }
            })
            .or_insert((item, audited_at));
    }

    let mut nodes = Vec::with_capacity(items.len());
    for item in &items {
        let type_name = item.type_name.clone().unwrap_or_default();
        let verdict = latest_audit.get(&item.id).and_then(|(audit, audited_at)| {
            parse_audit(audit).map(|(a, _)| AuditVerdict {
                audit_id: audit.id.clone(),
                passed: a.passed,
                score: a.score,
                violations: a.violations,
                unanchored_assumptions: a.unanchored_assumptions,
                suggested_edits: a.suggested_edits,
                audited_at: *audited_at,
            })
        });
        let badge = if AUDITABLE_TYPES.contains(&type_name.as_str()) {
            match &verdict {
                Some(v) => {
                    if v.violations.iter().any(|x| x.severity == "BLOCKING") {
                        Some(Badge::Red)
                    } else if v.passed {
                        Some(Badge::Green)
                    } else {
                        Some(Badge::Yellow)
                    }
                }
                None => {
                    // Docs are the anchors themselves; plans/sprints/todos are
                    // yellow until audited or anchored to a doc.
                    if type_name == HARNESS_DOC || anchored.contains(&item.id) {
                        None
                    } else {
                        Some(Badge::Yellow)
                    }
                }
            }
        } else {
            None
        };
        nodes.push(HarnessTreeNode {
            id: item.id.clone(),
            type_name,
            title: display_title(item),
            state: data_state(item),
            source_id: item.source_id.clone(),
            created_at: item.created_at,
            updated_at: item.updated_at,
            badge,
            verdict,
        });
    }
    nodes.sort_by(|a, b| {
        (a.type_name.as_str(), a.id.as_str()).cmp(&(b.type_name.as_str(), b.id.as_str()))
    });

    let edges = edges
        .into_iter()
        .filter(|e| node_ids.contains(&e.from_item_id) && node_ids.contains(&e.to_item_id))
        .map(|e| HarnessTreeEdge {
            id: e.id,
            from_item_id: e.from_item_id,
            to_item_id: e.to_item_id,
            relation: e.relation,
        })
        .collect();

    HarnessTreeResponse { nodes, edges }
}

pub async fn harness_tree_core(
    state: &super::AppState,
    source_id: Option<String>,
) -> Result<HarnessTreeResponse, ApiError> {
    let store = state.store.clone();
    let (items, edges) = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(Vec<ItemRecord>, Vec<GraphEdgeRecord>)> {
            let mut items = Vec::new();
            for type_name in HARNESS_NODE_TYPES {
                let (page, _) = store.list_items(ListItemsRequest {
                    source_id: source_id.clone(),
                    type_name: Some(type_name.to_owned()),
                    limit: Some(10_000),
                    sort_order: SortOrder::Desc,
                    ..Default::default()
                })?;
                items.extend(page);
            }
            let edges = match store.list_graph_edges(None, Some(GraphEdgeType::Manual), None) {
                Ok(edges) => edges,
                // Graph-less deployments: the tree still assembles, minus edges.
                Err(e) if e.to_string().contains("graph support is disabled") => Vec::new(),
                Err(e) => return Err(e),
            };
            Ok((items, edges))
        },
    )
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;
    Ok(assemble_tree(items, edges))
}

pub async fn harness_tree(
    State(state): State<super::AppState>,
    Query(query): Query<HarnessTreeQuery>,
) -> Result<Json<HarnessTreeResponse>, ApiError> {
    Ok(Json(harness_tree_core(&state, query.source_id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, type_name: &str, data: serde_json::Value) -> ItemRecord {
        ItemRecord {
            id: id.to_owned(),
            text: format!("content of {id}"),
            metadata: serde_json::json!({}),
            source_id: "harness".to_owned(),
            created_at: 1,
            updated_at: 1,
            path: None,
            type_name: Some(type_name.to_owned()),
            data: Some(data),
            analysis: None,
        }
    }

    fn edge(id: &str, from: &str, to: &str, relation: &str) -> GraphEdgeRecord {
        GraphEdgeRecord {
            id: id.to_owned(),
            from_item_id: from.to_owned(),
            to_item_id: to.to_owned(),
            edge_type: GraphEdgeType::Manual,
            relation: Some(relation.to_owned()),
            sort_order: "0".to_owned(),
            weight: 1.0,
            directed: true,
            metadata: serde_json::json!({}),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn audit_node(id: &str, target: &str, passed: bool, blocking: bool) -> ItemRecord {
        item(
            id,
            HARNESS_AUDIT,
            json!({
                "target_id": target,
                "target_type": HARNESS_TODO,
                "passed": passed,
                "score": if passed { 0.9 } else { 0.4 },
                "violations": if blocking {
                    vec![json!({
                        "doc_id": "adr-1",
                        "title": "ADR 1",
                        "violation_reason": "conflicts with pooling invariant",
                        "severity": "BLOCKING"
                    })]
                } else {
                    Vec::new()
                },
                "unanchored_assumptions": Vec::<String>::new(),
                "audited_at": 100
            }),
        )
    }

    #[test]
    fn green_when_audit_passed_without_blocking() {
        let items = vec![
            item(
                "todo-1",
                HARNESS_TODO,
                json!({"sprint_id": "s1", "title": "T", "action_spec": {}, "state": "PENDING", "sequence_order": 0}),
            ),
            audit_node("a1", "todo-1", true, false),
        ];
        let tree = assemble_tree(items, vec![edge("e1", "a1", "todo-1", REL_AUDITED)]);
        let node = tree.nodes.iter().find(|n| n.id == "todo-1").unwrap();
        assert_eq!(node.badge, Some(Badge::Green));
        assert_eq!(node.verdict.as_ref().unwrap().audit_id, "a1");
    }

    #[test]
    fn red_when_blocking_violation_even_if_passed() {
        let items = vec![
            item(
                "plan-1",
                HARNESS_PLAN,
                json!({"title": "P", "state": "PROPOSED"}),
            ),
            audit_node("a1", "plan-1", true, true),
        ];
        let tree = assemble_tree(items, Vec::new());
        let node = tree.nodes.iter().find(|n| n.id == "plan-1").unwrap();
        assert_eq!(node.badge, Some(Badge::Red));
    }

    #[test]
    fn yellow_on_failed_audit_without_blocking() {
        let items = vec![
            item(
                "todo-1",
                HARNESS_TODO,
                json!({"sprint_id": "s1", "title": "T", "action_spec": {}, "state": "PENDING", "sequence_order": 0}),
            ),
            audit_node("a1", "todo-1", false, false),
        ];
        let tree = assemble_tree(items, Vec::new());
        let node = tree.nodes.iter().find(|n| n.id == "todo-1").unwrap();
        assert_eq!(node.badge, Some(Badge::Yellow));
    }

    #[test]
    fn yellow_when_unanchored_and_unaudited() {
        let items = vec![item(
            "todo-1",
            HARNESS_TODO,
            json!({"sprint_id": "s1", "title": "T", "action_spec": {}, "state": "PENDING", "sequence_order": 0}),
        )];
        let tree = assemble_tree(items, Vec::new());
        let node = tree.nodes.iter().find(|n| n.id == "todo-1").unwrap();
        assert_eq!(node.badge, Some(Badge::Yellow));
        assert!(node.verdict.is_none());
    }

    #[test]
    fn no_badge_when_anchored_and_unaudited() {
        let items = vec![
            item(
                "todo-1",
                HARNESS_TODO,
                json!({"sprint_id": "s1", "title": "T", "action_spec": {}, "state": "PENDING", "sequence_order": 0}),
            ),
            item(
                "doc-1",
                HARNESS_DOC,
                json!({"doc_type": "ADR", "title": "D", "version": "1", "status": "ACTIVE"}),
            ),
        ];
        let tree = assemble_tree(items, vec![edge("e1", "todo-1", "doc-1", REL_ENFORCES_DOC)]);
        let todo = tree.nodes.iter().find(|n| n.id == "todo-1").unwrap();
        assert_eq!(todo.badge, None);
        // A doc without an audit carries no badge either.
        let doc = tree.nodes.iter().find(|n| n.id == "doc-1").unwrap();
        assert_eq!(doc.badge, None);
    }

    #[test]
    fn latest_audit_wins() {
        let mut older = audit_node("a1", "todo-1", true, false);
        older.updated_at = 50;
        older.data.as_mut().unwrap()["audited_at"] = json!(50);
        let newer = audit_node("a2", "todo-1", false, false);
        let items = vec![
            item(
                "todo-1",
                HARNESS_TODO,
                json!({"sprint_id": "s1", "title": "T", "action_spec": {}, "state": "PENDING", "sequence_order": 0}),
            ),
            older,
            newer,
        ];
        let tree = assemble_tree(items, Vec::new());
        let node = tree.nodes.iter().find(|n| n.id == "todo-1").unwrap();
        assert_eq!(node.verdict.as_ref().unwrap().audit_id, "a2");
        assert_eq!(node.badge, Some(Badge::Yellow));
    }

    #[test]
    fn edges_filtered_to_harness_nodes_only() {
        let items = vec![item(
            "plan-1",
            HARNESS_PLAN,
            json!({"title": "P", "state": "APPROVED"}),
        )];
        let edges = vec![
            edge("e1", "plan-1", "some-random-item", REL_GOVERNED_BY),
            edge("e2", "plan-1", "plan-1", REL_CONFLICTS_WITH),
        ];
        let tree = assemble_tree(items, edges);
        assert_eq!(tree.edges.len(), 1);
        assert_eq!(tree.edges[0].id, "e2");
    }

    #[test]
    fn agent_and_stream_nodes_have_no_badge() {
        let items = vec![
            item(
                "agent-1",
                HARNESS_AGENT,
                json!({"role": "coder", "capabilities": ["rust"]}),
            ),
            item(
                "stream-1",
                HARNESS_STREAM,
                json!({"stream_name": "orders", "aggregate_type": "Order", "schema_definition": {}}),
            ),
        ];
        let tree = assemble_tree(items, Vec::new());
        for node in &tree.nodes {
            assert_eq!(node.badge, None);
        }
    }

    #[test]
    fn title_falls_back_through_data_fields() {
        let items = vec![
            item(
                "sprint-1",
                HARNESS_SPRINT,
                json!({"plan_id": "p1", "cadence": "weekly", "goal": "Ship embedding cache", "state": "ACTIVE"}),
            ),
            item(
                "agent-1",
                HARNESS_AGENT,
                json!({"role": "reviewer", "capabilities": []}),
            ),
        ];
        let tree = assemble_tree(items, Vec::new());
        let sprint = tree.nodes.iter().find(|n| n.id == "sprint-1").unwrap();
        assert_eq!(sprint.title, "Ship embedding cache");
        let agent = tree.nodes.iter().find(|n| n.id == "agent-1").unwrap();
        assert_eq!(agent.title, "reviewer");
    }

    #[test]
    fn poc_hierarchy_edges_and_titles_survive_assembly() {
        let items = vec![
            item(
                "repo-1",
                HARNESS_REPO,
                json!({"name": "rust-rag", "url": "https://github.com/matst80/rust-rag"}),
            ),
            item(
                "poc-1",
                HARNESS_POC,
                json!({"repo": "repo-1", "summary": "RAG benchmark POC", "timestamp": 100}),
            ),
            item("dec-2", HARNESS_DECISION, json!({"title": "Use pgvector"})),
            item(
                "dec-1",
                HARNESS_DECISION,
                json!({"title": "Use sqlite-vec"}),
            ),
            item(
                "risk-1",
                HARNESS_RISK,
                json!({"title": "Embedding latency", "severity": "high"}),
            ),
            item(
                "val-1",
                HARNESS_VALIDATION,
                json!({"title": "Load test", "status": "needed"}),
            ),
            item(
                "roll-1",
                HARNESS_ROLLOUT,
                json!({"title": "Phased rollout", "phases": [{"name": "shadow"}], "prerequisites": []}),
            ),
            item(
                "res-1",
                HARNESS_RESOURCE,
                json!({"title": "GPU pool", "kind": "k8s"}),
            ),
        ];
        let edges = vec![
            edge("e1", "repo-1", "poc-1", REL_HAD_POC),
            edge("e2", "poc-1", "dec-2", REL_RAISED),
            edge("e3", "poc-1", "risk-1", REL_RAISED),
            edge("e4", "poc-1", "roll-1", REL_RAISED),
            edge("e5", "dec-2", "dec-1", REL_SUPERSEDES),
            edge("e6", "risk-1", "val-1", REL_ADDRESSED_BY),
            edge("e7", "roll-1", "res-1", REL_REQUIRES),
        ];
        let tree = assemble_tree(items, edges);
        // POC-domain nodes never carry audit badges.
        for node in &tree.nodes {
            assert_eq!(node.badge, None);
        }
        // All structural edges survive; titles resolve from poc summary / name.
        assert_eq!(tree.edges.len(), 7);
        let poc = tree.nodes.iter().find(|n| n.id == "poc-1").unwrap();
        assert_eq!(poc.title, "RAG benchmark POC");
        let repo = tree.nodes.iter().find(|n| n.id == "repo-1").unwrap();
        assert_eq!(repo.title, "rust-rag");
    }

    #[test]
    fn audit_can_target_poc_domain_nodes() {
        let items = vec![
            item(
                "poc-1",
                HARNESS_POC,
                json!({"repo": "r", "summary": "S", "timestamp": 1}),
            ),
            audit_node("a1", "poc-1", true, false),
        ];
        let tree = assemble_tree(items, Vec::new());
        let poc = tree.nodes.iter().find(|n| n.id == "poc-1").unwrap();
        assert_eq!(poc.verdict.as_ref().unwrap().audit_id, "a1");
        // poc nodes are not in the badge-eligible set (yet) but verdicts resolve.
        assert_eq!(poc.badge, None);
    }
}
