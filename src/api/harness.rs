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

use serde_json::Value;

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
/// Free-form session-evidence statements the agents store while working
/// (`{"statement": …}`). Not part of the POC hierarchy, but they carry
/// session context, so the tree includes them as evidence nodes.
pub const HARNESS_FACT: &str = "harness_fact";

/// Every harness node type, including audit verdicts and POC-domain nodes.
pub const HARNESS_NODE_TYPES: [&str; 17] = [
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
    HARNESS_FACT,
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

/// Bridge relations between the sprint domain and the POC/memory domain.
/// `repo → plan` scopes planned work to a repo; `sprint → poc` and
/// `todo → memory` tie planned work to the session evidence that motivated it.
pub const REL_HAS_PLAN: &str = "HAS_PLAN";
pub const REL_DERIVES_FROM: &str = "DERIVES_FROM";
pub const REL_EVIDENCED_BY: &str = "EVIDENCED_BY";

pub const HARNESS_RELATIONS: [&str; 16] = [
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
    REL_HAS_PLAN,
    REL_DERIVES_FROM,
    REL_EVIDENCED_BY,
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
    /// Full typed payload (severity, framework, phases, ...) so dashboards can
    /// aggregate without per-node fetches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
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

/// A session memory joined into the tree via `harness_poc.session_id ==
/// memory.source_id`. These are regular store entries (not harness-typed), so
/// they are not `nodes`; consumers join them to a POC via `poc_id` and onward
/// to sprints/todos via `DERIVES_FROM` / `EVIDENCED_BY` edges.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct HarnessTreeMemory {
    pub id: String,
    /// POC node the memory's session belongs to.
    pub poc_id: String,
    /// Session the memory was captured in (`source_id` of the entry).
    pub session_id: String,
    pub type_name: Option<String>,
    /// Entry text, truncated at [`MEMORY_TEXT_LIMIT`] chars for payload size.
    pub text: String,
    pub truncated: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Maximum `text` chars carried per memory in the tree response.
pub const MEMORY_TEXT_LIMIT: usize = 2_000;

#[derive(Debug, Serialize, JsonSchema)]
pub struct HarnessTreeResponse {
    pub nodes: Vec<HarnessTreeNode>,
    pub edges: Vec<HarnessTreeEdge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memories: Vec<HarnessTreeMemory>,
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
        for key in ["title", "name", "goal", "role", "stream_name", "summary", "statement"] {
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
pub fn assemble_tree(
    items: Vec<ItemRecord>,
    edges: Vec<GraphEdgeRecord>,
    memories: Vec<HarnessTreeMemory>,
) -> HarnessTreeResponse {
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
            data: item.data.clone(),
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

    HarnessTreeResponse {
        nodes,
        edges,
        memories,
    }
}

/// POC nodes that carry a `session_id` in their `data`, as (poc_id, session_id).
fn poc_sessions(items: &[ItemRecord]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in items {
        if item.type_name.as_deref() != Some(HARNESS_POC) {
            continue;
        }
        let Some(session_id) = item
            .data
            .as_ref()
            .and_then(|d| d.get("session_id"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        out.push((item.id.clone(), session_id.to_owned()));
    }
    out
}

/// Build the memory side of the tree: entries fetched per POC session
/// (`source_id == session_id`), excluding harness-typed entries and anything
/// already present in the node list. A session shared by several POCs yields
/// one memory entry per POC.
fn assemble_memories(
    sessions: &[(String, String)],
    mut fetched: Vec<ItemRecord>,
    existing_ids: &HashSet<String>,
) -> Vec<HarnessTreeMemory> {
    fetched.retain(|item| {
        let harness_typed = item
            .type_name
            .as_deref()
            .map(|t| HARNESS_NODE_TYPES.contains(&t))
            .unwrap_or(false);
        !harness_typed && !existing_ids.contains(&item.id)
    });

    let mut out = Vec::new();
    for (poc_id, session_id) in sessions {
        let mut per_poc: Vec<&ItemRecord> = fetched
            .iter()
            .filter(|item| item.source_id == *session_id)
            .collect();
        per_poc.sort_by_key(|item| (item.created_at, item.id.as_str()));
        for item in per_poc {
            let char_count = item.text.chars().count();
            let truncated = char_count > MEMORY_TEXT_LIMIT;
            let text: String = if truncated {
                item.text.chars().take(MEMORY_TEXT_LIMIT).collect()
            } else {
                item.text.clone()
            };
            out.push(HarnessTreeMemory {
                id: item.id.clone(),
                poc_id: poc_id.clone(),
                session_id: session_id.clone(),
                type_name: item.type_name.clone(),
                text,
                truncated,
                created_at: item.created_at,
                updated_at: item.updated_at,
            });
        }
    }
    out
}

pub async fn harness_tree_core(
    state: &super::AppState,
    source_id: Option<String>,
) -> Result<HarnessTreeResponse, ApiError> {
    let store = state.store.clone();
    let (items, edges, memories) = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(Vec<ItemRecord>, Vec<GraphEdgeRecord>, Vec<HarnessTreeMemory>)> {
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
            // Session memories: entries whose `source_id` matches a POC's
            // `session_id`. Fetched per session so the tree carries the
            // session ↔ repo evidence alongside the sprint domain.
            let sessions = poc_sessions(&items);
            let existing_ids: HashSet<String> = items.iter().map(|i| i.id.clone()).collect();
            let mut fetched = Vec::new();
            let mut seen_sessions: HashSet<String> = HashSet::new();
            for (_, session_id) in &sessions {
                if !seen_sessions.insert(session_id.clone()) {
                    continue;
                }
                let (page, _) = store.list_items(ListItemsRequest {
                    source_id: Some(session_id.clone()),
                    limit: Some(1_000),
                    sort_order: SortOrder::Desc,
                    ..Default::default()
                })?;
                fetched.extend(page);
            }
            let memories = assemble_memories(&sessions, fetched, &existing_ids);
            Ok((items, edges, memories))
        },
    )
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;
    Ok(assemble_tree(items, edges, memories))
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
        let tree = assemble_tree(items, vec![edge("e1", "a1", "todo-1", REL_AUDITED)], Vec::new());
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
        let tree = assemble_tree(items, Vec::new(), Vec::new());
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
        let tree = assemble_tree(items, Vec::new(), Vec::new());
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
        let tree = assemble_tree(items, Vec::new(), Vec::new());
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
        let tree = assemble_tree(items, vec![edge("e1", "todo-1", "doc-1", REL_ENFORCES_DOC)], Vec::new());
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
        let tree = assemble_tree(items, Vec::new(), Vec::new());
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
        let tree = assemble_tree(items, edges, Vec::new());
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
        let tree = assemble_tree(items, Vec::new(), Vec::new());
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
        let tree = assemble_tree(items, Vec::new(), Vec::new());
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
        let tree = assemble_tree(items, edges, Vec::new());
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
        let tree = assemble_tree(items, Vec::new(), Vec::new());
        let poc = tree.nodes.iter().find(|n| n.id == "poc-1").unwrap();
        assert_eq!(poc.verdict.as_ref().unwrap().audit_id, "a1");
        // poc nodes are not in the badge-eligible set (yet) but verdicts resolve.
        assert_eq!(poc.badge, None);
    }

    fn memory_record(id: &str, source_id: &str, text: &str) -> ItemRecord {
        ItemRecord {
            id: id.to_owned(),
            text: text.to_owned(),
            metadata: serde_json::json!({}),
            source_id: source_id.to_owned(),
            created_at: 10,
            updated_at: 10,
            path: None,
            type_name: None,
            data: None,
            analysis: None,
        }
    }

    #[test]
    fn poc_sessions_extracts_nonblank_session_ids() {
        let items = vec![
            item(
                "poc-1",
                HARNESS_POC,
                json!({"repo": "r", "summary": "S", "timestamp": 1, "session_id": "736dac0a"}),
            ),
            item(
                "poc-2",
                HARNESS_POC,
                json!({"repo": "r", "summary": "no session", "timestamp": 2}),
            ),
            item(
                "poc-3",
                HARNESS_POC,
                json!({"repo": "r", "summary": "blank", "timestamp": 3, "session_id": "   "}),
            ),
            item("dec-1", HARNESS_DECISION, json!({"title": "D", "session_id": "decoy"})),
        ];
        assert_eq!(poc_sessions(&items), vec![("poc-1".to_owned(), "736dac0a".to_owned())]);
    }

    #[test]
    fn memories_join_by_session_and_exclude_harness_nodes() {
        let sessions = vec![("poc-1".to_owned(), "736dac0a".to_owned())];
        let fetched = vec![
            memory_record("m1", "736dac0a", "first memory"),
            memory_record("m2", "736dac0a", "second memory"),
            // Other sessions must not leak in.
            memory_record("m3", "other-session", "unrelated"),
            // Harness-typed entries and already-tree items are excluded.
            item("m4", HARNESS_RISK, json!({"title": "R"})),
            memory_record("poc-1", "736dac0a", "shadows a tree node id"),
        ];
        let memories = assemble_memories(&sessions, fetched, &HashSet::from(["poc-1".to_owned()]));
        assert_eq!(
            memories.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["m1", "m2"]
        );
        assert!(memories.iter().all(|m| m.poc_id == "poc-1" && m.session_id == "736dac0a"));
        assert!(!memories.iter().any(|m| m.truncated));
    }

    #[test]
    fn shared_session_yields_memories_per_poc_and_text_truncates() {
        let long_text = "x".repeat(MEMORY_TEXT_LIMIT + 5);
        let fetched = vec![memory_record("m1", "s-1", &long_text)];
        let sessions = vec![
            ("poc-1".to_owned(), "s-1".to_owned()),
            ("poc-2".to_owned(), "s-1".to_owned()),
        ];
        let memories = assemble_memories(&sessions, fetched, &HashSet::new());
        assert_eq!(memories.len(), 2);
        assert_eq!(memories[0].poc_id, "poc-1");
        assert_eq!(memories[1].poc_id, "poc-2");
        assert!(memories[0].truncated);
        assert_eq!(memories[0].text.chars().count(), MEMORY_TEXT_LIMIT);
    }

    #[test]
    fn memories_carry_tree_response_through_assemble_tree() {
        let items = vec![item(
            "poc-1",
            HARNESS_POC,
            json!({"repo": "r", "summary": "S", "timestamp": 1, "session_id": "s-1"}),
        )];
        let memories = vec![HarnessTreeMemory {
            id: "m1".to_owned(),
            poc_id: "poc-1".to_owned(),
            session_id: "s-1".to_owned(),
            type_name: None,
            text: "evidence".to_owned(),
            truncated: false,
            created_at: 1,
            updated_at: 1,
        }];
        let tree = assemble_tree(items, Vec::new(), memories);
        assert_eq!(tree.memories.len(), 1);
        assert_eq!(tree.memories[0].poc_id, "poc-1");
    }
}
