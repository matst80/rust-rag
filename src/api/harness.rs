//! Harness graph domain: node types, edge relation conventions, and the
//! cockpit tree assembly behind `GET /api/harness/tree`.
//!
//! Harness nodes are typed entries (`type` + schema-validated `data`) stored
//! through the regular `/api/store` pipeline; structural relationships are
//! manual directed graph edges. This module only adds what cannot be composed
//! from existing endpoints: resolving the latest `harness_audit` verdict per
//! node and computing cockpit status badges.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Query, State},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use serde_json::Value;

use crate::api::{StoreRequest, store_entry_core};
use crate::db::{
    GraphEdgeRecord, GraphEdgeType, ItemRecord, ListItemsRequest, ManualEdgeInput, SortOrder,
};

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
/// POC-raised decisions reuse the generic `decision` type (`assets/schemas/decision.json`)
/// instead of a thinner harness-only shape — `harness_decision` (title +
/// status only) was a near-duplicate of it missing context/consequences/
/// deciders. Store with `context`, `decision`, `consequences`, `status`
/// (all required by the shared schema), link to the POC via the existing
/// `RAISED` edge — no `poc_id` data field needed. See `docs/harness.md`.
pub const HARNESS_DECISION: &str = "decision";
pub const HARNESS_RISK: &str = "harness_risk";
pub const HARNESS_COMPLIANCE: &str = "harness_compliance";
pub const HARNESS_RESOURCE: &str = "harness_resource";
pub const HARNESS_SCALING: &str = "harness_scaling";
pub const HARNESS_VALIDATION: &str = "harness_validation";
pub const HARNESS_ROLLOUT: &str = "harness_rollout";
/// Free-form session-evidence statements the agents store while working
/// (`{"statement": …}`, no schema). Not part of the POC hierarchy, but they
/// carry session context, so the tree includes them as evidence nodes.
/// Named `harness_evidence` (was `harness_fact`) — it collided with the
/// generic, structured `fact` type (claim/source/confidence) while meaning
/// something else entirely: an unstructured evidence blob, not a citeable
/// claim.
pub const HARNESS_EVIDENCE: &str = "harness_evidence";

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
    HARNESS_EVIDENCE,
];

/// Structural edge relations between harness nodes. Stored as manual directed
/// edges (`POST /admin/graph/edges` with `directed: true`).
///
/// Where a harness relation duplicated a canonical ontology predicate (see
/// `default_ontology_predicates` / `list_memory_conventions`), it was folded
/// into the canonical one instead of keeping a parallel UPPER_SNAKE name —
/// that keeps harness edges visible to the ontology worker and cross-agent
/// graph traversal instead of being a private vocabulary. What remains here
/// is genuinely harness-domain: audit/POC lifecycle relations with no
/// canonical equivalent. See `docs/harness.md` ("Edge relations").
pub const REL_BREAKS_INTO: &str = "BREAKS_INTO";
pub const REL_ENFORCES_DOC: &str = "ENFORCES_DOC";
pub const REL_DELEGATES_TO: &str = "DELEGATES_TO";
pub const REL_MUTATES_STREAM: &str = "MUTATES_STREAM";
/// Audit verdict node → audited target.
pub const REL_AUDITED: &str = "AUDITED";

/// POC-domain relations: repo → poc session, session → promoted memory,
/// rollout plan → resource, decision → superseded decision.
pub const REL_HAD_POC: &str = "HAD_POC";
pub const REL_RAISED: &str = "RAISED";
pub const REL_ADDRESSED_BY: &str = "ADDRESSED_BY";

/// Bridge relation tying planned work (todo) to the session evidence that
/// motivated it.
pub const REL_EVIDENCED_BY: &str = "EVIDENCED_BY";

/// Canonical ontology predicates reused for harness edges that have a plain
/// equivalent: `CONTAINS_TODO`/`HAS_PLAN` → `contains`, `CONFLICTS_WITH` →
/// `contradicts`, `REQUIRES`/`DERIVES_FROM` → `depends_on` (both already read
/// "from depends on to", so direction needed no change), harness `SUPERSEDES`
/// → canonical `supersedes` (was a pure case duplicate).
pub const REL_CONTAINS: &str = "contains";
pub const REL_CONTRADICTS: &str = "contradicts";
pub const REL_DEPENDS_ON: &str = "depends_on";
pub const REL_SUPERSEDES: &str = "supersedes";
pub const REL_PART_OF: &str = "part_of";
pub const REL_IMPLEMENTED_BY: &str = "implemented_by";

pub const HARNESS_RELATIONS: [&str; 15] = [
    REL_BREAKS_INTO,
    REL_ENFORCES_DOC,
    REL_DELEGATES_TO,
    REL_MUTATES_STREAM,
    REL_AUDITED,
    REL_HAD_POC,
    REL_RAISED,
    REL_ADDRESSED_BY,
    REL_SUPERSEDES,
    REL_EVIDENCED_BY,
    REL_CONTAINS,
    REL_CONTRADICTS,
    REL_DEPENDS_ON,
    REL_PART_OF,
    REL_IMPLEMENTED_BY,
];

/// Relations that nest `to_item_id` underneath `from_item_id` in the cockpit
/// tree (`GET /api/harness/tree`). This is the single source of truth for
/// "what counts as a parent/child edge" — [`HarnessTreeEdge::nests`] is
/// computed from it server-side so the frontend never has to guess or
/// hand-maintain a duplicate list.
///
/// Deliberately a *subset* of [`HARNESS_RELATIONS`]: `AUDITED` (audit
/// verdicts are resolved onto their target's badge, not nested as a child)
/// and `contradicts` (a peer conflict, not a hierarchy) are structural but
/// don't nest. Also includes pre-fold legacy names (`CONTAINS_TODO`,
/// `GOVERNED_BY`, `REQUIRES`) so edges written before those were folded into
/// their canonical equivalents still render. Matched case-insensitively —
/// agents emit both SCREAMING_CASE and lower_snake_case for the same relation.
pub const NESTING_RELATIONS: [&str; 15] = [
    REL_BREAKS_INTO,
    REL_ENFORCES_DOC,
    REL_DELEGATES_TO,
    REL_MUTATES_STREAM,
    REL_HAD_POC,
    REL_RAISED,
    REL_ADDRESSED_BY,
    REL_SUPERSEDES,
    REL_EVIDENCED_BY,
    REL_CONTAINS,
    REL_DEPENDS_ON,
    REL_PART_OF,
    REL_IMPLEMENTED_BY,
    // Legacy pre-fold aliases (see module doc comment on REL_CONTAINS etc.).
    "CONTAINS_TODO",
    "GOVERNED_BY",
];

fn relation_nests(relation: Option<&str>) -> bool {
    let Some(relation) = relation else {
        return false;
    };
    NESTING_RELATIONS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(relation))
        // REQUIRES was folded into depends_on but the name collides with
        // nothing else, so it's safe to accept unconditionally here too.
        || relation.eq_ignore_ascii_case("REQUIRES")
}

/// Relations that anchor a node to a governing doc. A plan/todo without any
/// of these is "unanchored" (yellow badge when it has no audit yet).
/// `GOVERNED_BY` was a duplicate of `ENFORCES_DOC` and was folded into it.
const ANCHOR_RELATIONS: [&str; 1] = [REL_ENFORCES_DOC];

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
    /// Whether this edge nests `to_item_id` under `from_item_id` in the
    /// cockpit tree — computed server-side from [`NESTING_RELATIONS`], the
    /// canonical list. Consumers should branch on this instead of
    /// hand-maintaining their own relation allowlist.
    pub nests: bool,
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

pub(crate) fn display_title(item: &ItemRecord) -> String {
    if let Some(data) = &item.data {
        for key in ["title", "name", "goal", "role", "stream_name", "summary", "statement"] {
            if let Some(t) = data.get(key).and_then(|v| v.as_str()) {
                if !t.trim().is_empty() {
                    return t.to_owned();
                }
            }
        }
    }
    let first_line = item.text.lines().next().unwrap_or(&item.id);
    let stripped = first_line.trim_start_matches('#').trim();
    if stripped.is_empty() {
        item.id.clone()
    } else {
        stripped.to_owned()
    }
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
            nests: relation_nests(e.relation.as_deref()),
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IngestDocRequest {
    /// Repository slug (e.g. "eventsourced-ai").
    pub repo: String,
    /// Repo-relative file path (e.g. "docs/architecture/overview.md").
    pub repo_path: String,
    /// Document type: ADR, SPEC, INVARIANT, OVERVIEW, ARCHITECTURE, GUIDE.
    pub doc_type: String,
    /// Document title.
    pub title: String,
    /// Full markdown content of the document.
    pub content: String,
    /// Optional document version (defaults to "1.0").
    #[serde(default)]
    pub version: Option<String>,
    /// Document status: ACTIVE, DRAFT, SUPERSEDED, DEPRECATED (defaults to "ACTIVE").
    #[serde(default)]
    pub status: Option<String>,
    /// Optional Git commit SHA or tag.
    #[serde(default)]
    pub git_sha: Option<String>,
    /// Optional executive summary. If omitted, automatically extracted from markdown.
    #[serde(default)]
    pub summary: Option<String>,
    /// Optional context (problem/situation). If omitted, automatically extracted from markdown.
    #[serde(default)]
    pub context: Option<String>,
    /// Optional decision made. If omitted, automatically extracted from markdown.
    #[serde(default)]
    pub decision: Option<String>,
    /// Optional consequences (trade-offs, constraints). If omitted, automatically extracted from markdown.
    #[serde(default)]
    pub consequences: Option<String>,
    /// Optional list of repo source files or directories this doc governs or is implemented by.
    #[serde(default)]
    pub source_files: Option<Vec<String>>,
    /// Optional list of key invariants / architectural rules asserted by this doc.
    #[serde(default)]
    pub invariants: Option<Vec<String>>,
    /// Optional parent document ID (creates `part_of` and `contains` edges).
    #[serde(default)]
    pub parent_doc_id: Option<String>,
    /// Optional ID of an older document this replaces (creates `supersedes` edge).
    #[serde(default)]
    pub supersedes: Option<String>,
    /// Optional custom stable ID. If omitted, generated deterministically as `doc:<repo>:<doc_type_lower>:<slug>`.
    #[serde(default)]
    pub id: Option<String>,
    /// Optional custom source_id namespace (defaults to `repo:<repo>:knowledge`).
    #[serde(default)]
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct IngestDocResponse {
    pub id: String,
    pub repo_id: String,
    pub path: String,
    pub doc_type: String,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consequences: Option<String>,
    pub sections_count: usize,
    pub edges_created: Vec<String>,
}

pub(crate) fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[derive(Debug, Default)]
pub(crate) struct ExtractedDoc {
    pub summary: Option<String>,
    pub context: Option<String>,
    pub decision: Option<String>,
    pub consequences: Option<String>,
    pub sections: Vec<Value>,
}

pub(crate) fn extract_doc_outline(text: &str) -> ExtractedDoc {
    let mut summary = None;
    let mut context = None;
    let mut decision = None;
    let mut consequences = None;
    let mut sections = Vec::new();
    let mut current_section: Option<(String, String)> = None;
    let mut found_first_heading = false;
    let mut pre_heading_paragraphs = Vec::new();

    let mut active_prefix: Option<&'static str> = None;
    let mut prefix_buf = String::new();

    let flush_prefix = |prefix: Option<&'static str>,
                            buf: &mut String,
                            ctx: &mut Option<String>,
                            dec: &mut Option<String>,
                            cons: &mut Option<String>| {
        if let Some(p) = prefix {
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                match p {
                    "context" if ctx.is_none() => *ctx = Some(trimmed.to_string()),
                    "decision" if dec.is_none() => *dec = Some(trimmed.to_string()),
                    "consequences" if cons.is_none() => *cons = Some(trimmed.to_string()),
                    _ => {}
                }
            }
            buf.clear();
        }
    };

    for line in text.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("CONTEXT:") || upper.starts_with("CONTEXT (") {
            flush_prefix(
                active_prefix,
                &mut prefix_buf,
                &mut context,
                &mut decision,
                &mut consequences,
            );
            active_prefix = Some("context");
            let after = if let Some(idx) = trimmed.find(':') {
                &trimmed[idx + 1..]
            } else {
                ""
            };
            prefix_buf.push_str(after.trim());
        } else if upper.starts_with("DECISION:") || upper.starts_with("DECISION (") {
            flush_prefix(
                active_prefix,
                &mut prefix_buf,
                &mut context,
                &mut decision,
                &mut consequences,
            );
            active_prefix = Some("decision");
            let after = if let Some(idx) = trimmed.find(':') {
                &trimmed[idx + 1..]
            } else {
                ""
            };
            prefix_buf.push_str(after.trim());
        } else if upper.starts_with("CONSEQUENCES:") || upper.starts_with("CONSEQUENCES (") {
            flush_prefix(
                active_prefix,
                &mut prefix_buf,
                &mut context,
                &mut decision,
                &mut consequences,
            );
            active_prefix = Some("consequences");
            let after = if let Some(idx) = trimmed.find(':') {
                &trimmed[idx + 1..]
            } else {
                ""
            };
            prefix_buf.push_str(after.trim());
        } else if active_prefix.is_some() {
            if trimmed.is_empty() {
                flush_prefix(
                    active_prefix,
                    &mut prefix_buf,
                    &mut context,
                    &mut decision,
                    &mut consequences,
                );
                active_prefix = None;
            } else {
                if !prefix_buf.is_empty() {
                    prefix_buf.push(' ');
                }
                prefix_buf.push_str(trimmed);
            }
        }

        if trimmed.starts_with('#') {
            found_first_heading = true;
            if let Some((stitle, sbuf)) = current_section.take() {
                let s_summary = sbuf.trim().lines().next().unwrap_or("").trim().to_string();
                let anchor = slugify(&stitle);
                let stitle_lower = stitle.to_lowercase();
                if stitle_lower == "context" && context.is_none() && !sbuf.trim().is_empty() {
                    context = Some(sbuf.trim().to_string());
                } else if stitle_lower == "decision" && decision.is_none() && !sbuf.trim().is_empty() {
                    decision = Some(sbuf.trim().to_string());
                } else if stitle_lower == "consequences" && consequences.is_none() && !sbuf.trim().is_empty() {
                    consequences = Some(sbuf.trim().to_string());
                }
                sections.push(serde_json::json!({
                    "title": stitle,
                    "anchor": anchor,
                    "summary": if s_summary.is_empty() { Value::Null } else { Value::String(s_summary) },
                }));
            }
            let title = trimmed.trim_start_matches('#').trim().to_string();
            current_section = Some((title, String::new()));
        } else if !trimmed.is_empty() {
            if !found_first_heading {
                pre_heading_paragraphs.push(trimmed);
            } else if let Some((_, ref mut sbuf)) = current_section {
                if !sbuf.is_empty() {
                    sbuf.push(' ');
                }
                sbuf.push_str(trimmed);
            }
        }
    }

    flush_prefix(
        active_prefix,
        &mut prefix_buf,
        &mut context,
        &mut decision,
        &mut consequences,
    );

    if let Some((stitle, sbuf)) = current_section {
        let s_summary = sbuf.trim().lines().next().unwrap_or("").trim().to_string();
        let anchor = slugify(&stitle);
        let stitle_lower = stitle.to_lowercase();
        if stitle_lower == "context" && context.is_none() && !sbuf.trim().is_empty() {
            context = Some(sbuf.trim().to_string());
        } else if stitle_lower == "decision" && decision.is_none() && !sbuf.trim().is_empty() {
            decision = Some(sbuf.trim().to_string());
        } else if stitle_lower == "consequences" && consequences.is_none() && !sbuf.trim().is_empty() {
            consequences = Some(sbuf.trim().to_string());
        }
        sections.push(serde_json::json!({
            "title": stitle,
            "anchor": anchor,
            "summary": if s_summary.is_empty() { Value::Null } else { Value::String(s_summary) },
        }));
    }

    if !pre_heading_paragraphs.is_empty() {
        summary = Some(pre_heading_paragraphs.join(" "));
    } else if let Some(first_sec) = sections.first() {
        if let Some(s) = first_sec.get("summary").and_then(|v| v.as_str()) {
            if !s.is_empty() {
                summary = Some(s.to_string());
            }
        }
    }

    ExtractedDoc {
        summary,
        context,
        decision,
        consequences,
        sections,
    }
}

pub async fn ingest_doc_core(
    state: &super::AppState,
    req: IngestDocRequest,
) -> Result<IngestDocResponse, ApiError> {
    let repo_slug = slugify(&req.repo);
    let repo_id = format!("repo:{repo_slug}");
    let doc_type_normalized = req.doc_type.trim().to_uppercase();
    let doc_type_lower = doc_type_normalized.to_lowercase();
    let title_slug = slugify(&req.title);
    let doc_id = req
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("doc:{repo_slug}:{doc_type_lower}:{title_slug}"));

    // 1. Ensure harness_repo root exists
    let store = state.store.clone();
    let check_repo_id = repo_id.clone();
    let repo_exists = tokio::task::spawn_blocking(move || {
        store.get_item(&check_repo_id).map(|opt| opt.is_some())
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    if !repo_exists {
        let repo_name = req.repo.clone();
        let repo_req = StoreRequest {
            id: Some(repo_id.clone()),
            text: format!(
                "# Repository: {repo_name}\nGoverning repository root node for {repo_name} documentation, architecture specifications, and POC sessions."
            ),
            metadata: serde_json::json!({
                "repo": repo_name,
                "tags": ["repository", "root", repo_name],
                "author": "system"
            }),
            source_id: "knowledge".to_string(),
            path: Some(format!("repos/{repo_slug}")),
            type_name: Some(HARNESS_REPO.to_string()),
            data: Some(serde_json::json!({
                "name": repo_name,
                "default_branch": "main"
            })),
            chunk: None,
        };
        store_entry_core(state, repo_req, None).await?;
    }

    // 2. Extract outline, summary, decision, consequences, context
    let extracted = extract_doc_outline(&req.content);
    let final_summary = req
        .summary
        .filter(|s| !s.trim().is_empty())
        .or(extracted.summary)
        .unwrap_or_else(|| req.title.clone());
    let final_context = req.context.or(extracted.context);
    let final_decision = req.decision.or(extracted.decision);
    let final_consequences = req.consequences.or(extracted.consequences);

    // 3. Assemble data payload
    let mut doc_data = serde_json::json!({
        "doc_type": doc_type_normalized,
        "title": req.title,
        "version": req.version.as_deref().unwrap_or("1.0"),
        "status": req.status.as_deref().unwrap_or("ACTIVE").to_uppercase(),
        "repo": req.repo,
        "repo_path": req.repo_path,
        "summary": final_summary,
        "sections": extracted.sections,
    });
    if let Some(ref c) = final_context {
        doc_data["context"] = Value::String(c.clone());
    }
    if let Some(ref d) = final_decision {
        doc_data["decision"] = Value::String(d.clone());
    }
    if let Some(ref c) = final_consequences {
        doc_data["consequences"] = Value::String(c.clone());
    }
    if let Some(sha) = req.git_sha.as_deref().filter(|s| !s.trim().is_empty()) {
        doc_data["git_sha"] = Value::String(sha.to_string());
    }
    if let Some(files) = req.source_files.as_ref() {
        doc_data["source_files"] = serde_json::to_value(files).unwrap_or_default();
    }
    if let Some(invs) = req.invariants.as_ref() {
        doc_data["invariants"] = serde_json::to_value(invs).unwrap_or_default();
    }
    if let Some(parent) = req.parent_doc_id.as_deref().filter(|s| !s.trim().is_empty()) {
        doc_data["parent_doc_id"] = Value::String(parent.to_string());
    }
    if let Some(sup) = req.supersedes.as_deref().filter(|s| !s.trim().is_empty()) {
        doc_data["supersedes"] = Value::String(sup.to_string());
    }

    // 4. Store document entry
    let source_id = req
        .source_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("repo:{repo_slug}:knowledge"));
    let doc_path = if req.repo_path.starts_with("docs/") {
        req.repo_path.clone()
    } else {
        format!("docs/{}", req.repo_path.trim_start_matches('/'))
    };

    let store_req = StoreRequest {
        id: Some(doc_id.clone()),
        text: req.content,
        metadata: serde_json::json!({
            "repo": req.repo,
            "repo_path": req.repo_path,
            "doc_type": doc_type_normalized,
            "tags": ["documentation", doc_type_lower, req.repo],
            "author": "agent"
        }),
        source_id,
        path: Some(doc_path.clone()),
        type_name: Some(HARNESS_DOC.to_string()),
        data: Some(doc_data),
        chunk: None,
    };
    store_entry_core(state, store_req, None).await?;

    // 5. Create graph edges
    let mut edges_created = Vec::new();
    let store = state.store.clone();
    let target_doc_id = doc_id.clone();
    let target_repo_id = repo_id.clone();
    let parent_id_opt = req.parent_doc_id.clone();
    let supersedes_opt = req.supersedes.clone();

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        // repo contains doc
        store.add_manual_edge(ManualEdgeInput {
            from_item_id: target_repo_id.clone(),
            to_item_id: target_doc_id.clone(),
            relation: Some(Cow::Borrowed(REL_CONTAINS)),
            sort_order: None,
            weight: 1.0,
            directed: true,
            metadata: serde_json::json!({}),
        })?;

        // doc part_of parent && parent contains doc
        if let Some(parent_id) = parent_id_opt {
            if !parent_id.trim().is_empty() {
                store.add_manual_edge(ManualEdgeInput {
                    from_item_id: target_doc_id.clone(),
                    to_item_id: parent_id.clone(),
                    relation: Some(Cow::Borrowed(REL_PART_OF)),
                    sort_order: None,
                    weight: 1.0,
                    directed: true,
                    metadata: serde_json::json!({}),
                })?;
                store.add_manual_edge(ManualEdgeInput {
                    from_item_id: parent_id,
                    to_item_id: target_doc_id.clone(),
                    relation: Some(Cow::Borrowed(REL_CONTAINS)),
                    sort_order: None,
                    weight: 1.0,
                    directed: true,
                    metadata: serde_json::json!({}),
                })?;
            }
        }

        // doc supersedes older doc
        if let Some(old_id) = supersedes_opt {
            if !old_id.trim().is_empty() {
                store.add_manual_edge(ManualEdgeInput {
                    from_item_id: target_doc_id.clone(),
                    to_item_id: old_id,
                    relation: Some(Cow::Borrowed(REL_SUPERSEDES)),
                    sort_order: None,
                    weight: 1.0,
                    directed: true,
                    metadata: serde_json::json!({}),
                })?;
            }
        }

        Ok(())
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    edges_created.push(format!("{repo_id} -[contains]-> {doc_id}"));
    if let Some(ref parent) = req.parent_doc_id {
        edges_created.push(format!("{doc_id} -[part_of]-> {parent}"));
        edges_created.push(format!("{parent} -[contains]-> {doc_id}"));
    }
    if let Some(ref sup) = req.supersedes {
        edges_created.push(format!("{doc_id} -[supersedes]-> {sup}"));
    }

    let sections_count = extracted.sections.len();
    Ok(IngestDocResponse {
        id: doc_id,
        repo_id,
        path: doc_path,
        doc_type: doc_type_normalized,
        title: req.title,
        summary: final_summary,
        context: final_context,
        decision: final_decision,
        consequences: final_consequences,
        sections_count,
        edges_created,
    })
}

pub async fn ingest_harness_doc(
    State(state): State<super::AppState>,
    Json(req): Json<IngestDocRequest>,
) -> Result<Json<IngestDocResponse>, ApiError> {
    Ok(Json(ingest_doc_core(&state, req).await?))
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
            edge("e1", "plan-1", "some-random-item", REL_ENFORCES_DOC),
            edge("e2", "plan-1", "plan-1", REL_CONTRADICTS),
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
            edge("e7", "roll-1", "res-1", REL_DEPENDS_ON),
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

    #[test]
    fn slugify_produces_clean_kebab_case() {
        assert_eq!(slugify("Eventstore Single Writer"), "eventstore-single-writer");
        assert_eq!(slugify("eventsourced-ai / streamproc"), "eventsourced-ai-streamproc");
        assert_eq!(slugify("  --hello__world--  "), "hello-world");
    }

    #[test]
    fn extract_doc_outline_extracts_headings_and_summary() {
        let md = r#"# Architecture Overview

eventsourced-ai is an event-sourced agent runtime.
Sessions are append-only streams.

## Topology

Binaries live under cmd/.

## Domain Core

Domain model implements pure folding.
"#;
        let extracted = extract_doc_outline(md);
        assert!(extracted.summary.unwrap().contains("eventsourced-ai is an event-sourced agent runtime."));
        assert_eq!(extracted.sections.len(), 3);
        assert_eq!(extracted.sections[0]["title"], "Architecture Overview");
        assert_eq!(extracted.sections[1]["title"], "Topology");
        assert_eq!(extracted.sections[1]["anchor"], "topology");
        assert_eq!(extracted.sections[2]["title"], "Domain Core");

        let adr_md = r#"# ADR: Storage
CONTEXT: We had a storage outage.
DECISION: Use single writer.
CONSEQUENCES: Downtime on crash."#;
        let adr_extracted = extract_doc_outline(adr_md);
        assert_eq!(adr_extracted.context.as_deref(), Some("We had a storage outage."));
        assert_eq!(adr_extracted.decision.as_deref(), Some("Use single writer."));
        assert_eq!(adr_extracted.consequences.as_deref(), Some("Downtime on crash."));
    }
}
