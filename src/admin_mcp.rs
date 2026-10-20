//! Admin MCP server for graph/ontology curation and projection-map
//! maintenance, mounted at `/mcp/admin`.
//!
//! The main `/mcp` server ([`crate::mcp`]) keeps the memory, messaging, and
//! harness surfaces lean; graph edge/ontology-review tools and the
//! projection-map tools live here so everyday memory clients don't carry
//! their schemas in context. Same bearer-token and Host-header guards as the
//! other endpoints (see `readme_mcp.md`).

use crate::api::{
    AppState, CreateManualEdgeRequest, DeleteResponse, GraphEdgePayload, GraphEdgesResponse,
    GraphNeighborhoodQuery, GraphNeighborhoodResponse, GraphRebuildResponse, GraphStatusResponse,
    ListGraphEdgesQuery, edge_payload, metadata_schema, titles_for_edges,
};

/// First `max` chars of `text`, on a char boundary, with an ellipsis when
/// truncated. Byte-slicing would panic on multi-byte content.
fn snippet(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_owned();
    }
    let cut: String = trimmed.chars().take(max).collect();
    format!("{cut}…")
}

/// Resolve the loose-link peers of `params.id`.
///
/// Reads the materialized graph rather than re-scanning metadata, so this
/// reflects the last rebuild. Groups are keyed by the configured rule that
/// matched, mirroring [`crate::loose_links`]: each group is a metadata cluster
/// the anchor belongs to, reached through its `ext:` hub.
pub(crate) async fn resolve_link_peers(
    store: Arc<dyn VectorStore>,
    rules: Vec<crate::db::LooseLinkRule>,
    params: LinkPeersParams,
) -> Result<Json<LinkPeersResponse>, String> {
    let LinkPeersParams { id, key, limit } = params;
    let limit = limit.unwrap_or(50).clamp(1, 500);
    let anchor_id = id.clone();

    let groups = tokio::task::spawn_blocking(move || {
        let anchor = store
            .get_item(&anchor_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("item {anchor_id} not found"))?;

        let mut groups: Vec<LinkPeerGroup> = Vec::new();
        for rule in rules {
            if let Some(wanted) = key.as_deref()
                && !wanted.eq_ignore_ascii_case(&rule.metadata_key)
            {
                continue;
            }
            // Same normalization the planner used (trim + lowercase, `data`
            // before `metadata`), so this resolves the same cluster.
            let Some(value) = crate::db::extract_metadata_key(
                Some(&anchor.metadata),
                anchor.data.as_ref(),
                &rule.metadata_key,
            ) else {
                continue;
            };
            let hub_id = crate::db::loose_link_hub_id(&rule.metadata_key, &value);
            // The hub stores no row — it dangles. Probe for worker-written
            // edges incident to it instead: none means "no peers" (cluster
            // too small or rebuild hasn't run), not an error — otherwise a
            // fresh entry would fail the whole call.
            let has_cluster = store
                .list_graph_edges(Some(&hub_id), Some(GraphEdgeType::Manual), None)
                .map_err(|error| error.to_string())?
                .iter()
                .any(|edge| {
                    edge.metadata.get("source").and_then(|v| v.as_str())
                        == Some(crate::db::LOOSE_LINK_SOURCE)
                });
            if !has_cluster {
                continue;
            }
            // The BFS limit bounds *nodes visited*, and the anchor and the hub
            // are both visited then filtered out — so ask for headroom or
            // `limit` peers would be short by two.
            let neighborhood = store
                .graph_neighborhood(&hub_id, 1, limit + 2, Some(GraphEdgeType::Manual))
                .map_err(|error| error.to_string())?;

            // Only follow edges the loose-link worker actually wrote; the hub
            // may also carry hand-curated edges we must not report here.
            let written_by_worker = |edge: &crate::db::GraphEdgeRecord| {
                edge.metadata.get("source").and_then(|v| v.as_str())
                    == Some(crate::db::LOOSE_LINK_SOURCE)
            };
            let member_ids: std::collections::HashSet<&str> = neighborhood
                .edges
                .iter()
                .filter(|edge| written_by_worker(edge))
                .flat_map(|edge| [edge.from_item_id.as_str(), edge.to_item_id.as_str()])
                .collect();
            let relation = neighborhood
                .edges
                .iter()
                .find(|edge| written_by_worker(edge))
                .and_then(|edge| edge.relation.clone());

            let mut peers: Vec<LinkPeer> = neighborhood
                .nodes
                .iter()
                // Skip the anchor itself and the virtual hub; the hub has no
                // body, so it isn't a useful peer.
                .filter(|node| node.id != anchor_id && node.id != hub_id)
                .filter(|node| member_ids.contains(node.id.as_str()))
                .map(|node| LinkPeer {
                    title: Some(crate::api::harness::display_title(node)),
                    snippet: snippet(&node.text, 280),
                    id: node.id.clone(),
                })
                .collect();
            peers.sort_unstable_by(|a, b| a.id.cmp(&b.id));
            peers.truncate(limit);

            if peers.is_empty() {
                continue;
            }
            groups.push(LinkPeerGroup {
                metadata_key: rule.metadata_key,
                value,
                hub_id,
                relation,
                peers,
            });
        }
        Ok::<_, String>(groups)
    })
    .await
    .map_err(|error| error.to_string())??;

    let stale = groups.is_empty();
    Ok(Json(LinkPeersResponse { id, stale, groups }))
}
use crate::db::{GraphEdgeType, ManualEdgeInput, VectorStore};
use crate::mcp::{IdParams, stringify_api_error};
use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, sync::Arc, time::Duration};

const ADMIN_SERVER_NAME: &str = "rust-rag-admin";
const ADMIN_SERVER_INSTRUCTIONS: &str = "rust-rag admin surface: graph/ontology curation and projection-map maintenance.\
\
GRAPH: `graph_status` / `list_graph_edges` / `graph_neighborhood` to inspect, `create_manual_edge` to link items (prefer the canonical predicates listed in its schema), `rebuild_graph` to regenerate similarity edges.\
LOOSE LINKS: `link_peers` answers \"what else shares this entry's repo/component?\" for entries that are metadata-related but not textually similar.\
ONTOLOGY REVIEWS: `list_ontology_reviews` shows edges with status 'suggested'; resolve them with `accept_ontology_review` / `reject_ontology_review`.\
MAP: `map_clusters` (cheap index) then `map_get` / `map_nearest` to explore the projection, `map_reassign` to pin items to clusters, `map_rebuild` to re-run clustering.\
\
Memory storage lives on the main rust-rag MCP server at /mcp; ACP session control at /mcp/acp.";

#[derive(Clone)]
pub struct AdminMcpServer {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl AdminMcpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AdminMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                ADMIN_SERVER_NAME.to_owned(),
                env!("CARGO_PKG_VERSION").to_owned(),
            ))
            .with_instructions(ADMIN_SERVER_INSTRUCTIONS.to_owned())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GraphNeighborhoodParams {
    pub id: String,
    pub depth: Option<usize>,
    pub limit: Option<usize>,
    pub edge_type: Option<GraphEdgeType>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LinkPeersParams {
    /// Entry id to resolve peers for.
    pub id: String,
    /// Restrict to one metadata key (e.g. `repo`). Defaults to every key
    /// configured in `RAG_LOOSE_LINK_RULES`.
    pub key: Option<String>,
    /// Max peers per key. Defaults to 50.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LinkPeersResponse {
    pub id: String,
    /// No loose-link edges touch this entry — either the batch job hasn't run
    /// since it was stored, or none of its metadata keys match a rule.
    pub stale: bool,
    pub groups: Vec<LinkPeerGroup>,
}

/// Peers of the anchor sharing one metadata key/value.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LinkPeerGroup {
    pub metadata_key: String,
    pub value: String,
    /// The `ext:` hub all members of this cluster link through.
    pub hub_id: String,
    pub relation: Option<String>,
    pub peers: Vec<LinkPeer>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LinkPeer {
    pub id: String,
    pub title: Option<String>,
    /// First ~280 chars of the entry body.
    pub snippet: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcceptOntologyReviewParams {
    pub id: String,
    #[serde(default)]
    pub relation: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateGraphEdgeParams {
    pub id: String,
    #[serde(default)]
    pub relation: Option<String>,
    #[serde(default)]
    pub sort_order: Option<String>,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MapRebuildResponse {
    pub status: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MapGetResponse {
    pub points: Vec<crate::projection::MapPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<MapSummary>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MapSummary {
    pub total_points: usize,
    pub clusters: Vec<MapClusterRow>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MapClusterRow {
    pub cluster: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub count: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MapClustersResponse {
    pub clusters: Vec<MapClusterRow>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MapGetParams {
    /// Anchor point. Result is sorted by 3D distance from it.
    #[serde(default)]
    pub center_id: Option<String>,
    /// Only return points within this 3D distance of `center_id`.
    #[serde(default)]
    pub radius: Option<f32>,
    /// Only return points in this cluster (post-override).
    #[serde(default)]
    pub cluster: Option<usize>,
    /// Explicit subset of item ids.
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    /// Cap result count after sorting.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Drop snippet/tags/cluster_description to shrink payload (~6× smaller).
    #[serde(default)]
    pub compact: Option<bool>,
    /// Populate `distance` even when not filtering by radius (requires `center_id`).
    #[serde(default)]
    pub include_distance: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MapNearestParams {
    /// Anchor point.
    pub center_id: String,
    /// Number of neighbours to return. Default 20.
    #[serde(default)]
    pub k: Option<usize>,
    /// Skip points already in this cluster. Handy for hunting misclassified
    /// items near a cluster the anchor belongs to.
    #[serde(default)]
    pub exclude_cluster: Option<usize>,
    /// Drop snippet/tags/cluster_description (default true for this tool).
    #[serde(default)]
    pub compact: Option<bool>,
}

fn build_map_summary(points: &[crate::projection::MapPoint]) -> Vec<MapClusterRow> {
    use std::collections::BTreeMap;
    let mut by: BTreeMap<usize, MapClusterRow> = BTreeMap::new();
    for p in points {
        let row = by.entry(p.cluster).or_insert_with(|| MapClusterRow {
            cluster: p.cluster,
            name: None,
            description: None,
            count: 0,
        });
        row.count += 1;
        if row.name.is_none() {
            row.name = p.cluster_name.clone();
        }
        if row.description.is_none() {
            row.description = p.cluster_description.clone();
        }
    }
    by.into_values().collect()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MapReassignParams {
    /// Item id to move.
    pub id: String,
    /// RECOMMENDED. Id of an item already in the target cluster. The reassignment
    /// pins to that item so it survives even when numeric cluster ids shuffle
    /// on the next rebuild.
    #[serde(default)]
    pub anchor_id: Option<String>,
    /// Legacy numeric cluster id. Will break across rebuilds if HDBSCAN
    /// renumbers — prefer `anchor_id`.
    #[serde(default)]
    pub cluster: Option<usize>,
    /// Remove any override (numeric or anchor) and revert to the
    /// algorithm-assigned cluster on next rebuild.
    #[serde(default)]
    pub clear: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MapReassignResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_id: Option<String>,
    pub cleared: bool,
}

#[tool_router(router = tool_router)]
impl AdminMcpServer {
    #[tool(description = "Return current graph configuration and edge counts.")]
    async fn graph_status(&self) -> Result<Json<GraphStatusResponse>, String> {
        let store = self.state.store.clone();
        let status = tokio::task::spawn_blocking(move || store.graph_status())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(status.into()))
    }

    #[tool(description = "List graph edges, optionally filtered by item_id or edge type.")]
    async fn list_graph_edges(
        &self,
        Parameters(query): Parameters<ListGraphEdgesQuery>,
    ) -> Result<Json<GraphEdgesResponse>, String> {
        let store = self.state.store.clone();
        let edges = tokio::task::spawn_blocking(move || {
            let edges = store.list_graph_edges(
                query.item_id.as_deref(),
                query.edge_type,
                query.status.as_deref(),
            )?;
            let titles = titles_for_edges(store.as_ref(), &edges);
            Ok::<_, anyhow::Error>(
                edges
                    .into_iter()
                    .map(|e| edge_payload(e, &titles))
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        Ok(Json(GraphEdgesResponse { edges }))
    }

    #[tool(description = "Return the graph neighborhood around a center item id.")]
    async fn graph_neighborhood(
        &self,
        Parameters(params): Parameters<GraphNeighborhoodParams>,
    ) -> Result<Json<GraphNeighborhoodResponse>, String> {
        let store = self.state.store.clone();
        let GraphNeighborhoodParams {
            id,
            depth,
            limit,
            edge_type,
        } = params;
        let query = GraphNeighborhoodQuery {
            depth,
            limit,
            edge_type,
        };
        let depth = query.depth.unwrap_or(1);
        let limit = query.limit.unwrap_or(100);
        let edge_type = query.edge_type;
        let neighborhood = tokio::task::spawn_blocking(move || {
            store.graph_neighborhood(&id, depth, limit, edge_type)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        Ok(Json(neighborhood.into()))
    }

    #[tool(
        description = "Find entries linked to `id` purely because they share a metadata value — e.g. every other memory with the same `repo`. \
This is the loose-link view: it answers \"what else touches this repo?\" for entries whose text never resembles each other, which semantic search and the ontology worker both miss. \
It reads the materialized graph, so it reflects the last `rebuild_graph` pass over loose links; if `stale` is true the batch job has not run since this entry was stored, or none of its metadata keys match a rule. \
Peers share an `ext:{key}:{value}` hub, so this is one hop rather than a full pairwise listing. Pass `key` to focus on one metadata key."
    )]
    async fn link_peers(
        &self,
        Parameters(params): Parameters<LinkPeersParams>,
    ) -> Result<Json<LinkPeersResponse>, String> {
        let store = self.state.store.clone();
        let rules = self.state.loose_links.rules.clone();
        resolve_link_peers(store, rules, params).await
    }

    #[tool(description = "Rebuild similarity edges across the graph.")]
    async fn rebuild_graph(&self) -> Result<Json<GraphRebuildResponse>, String> {
        let store = self.state.store.clone();
        let rebuilt_edges = tokio::task::spawn_blocking(move || store.rebuild_similarity_graph())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(GraphRebuildResponse { rebuilt_edges }))
    }

    #[tool(description = "Create a manual graph edge between two items. \
PREDICATES: use one of the canonical predicates (`is_a`, `part_of`, `caused_by`, `works_for`, `contradicts`, `depends_on`, `contains`, `implemented_by`) so the edge composes with ontology-worker edges, graph traversal, and the analyze pipeline. See `list_memory_conventions` for direction semantics (e.g. `from is_a to` means FROM is a subtype of TO, NOT the reverse). Off-list `relation` strings are accepted but are essentially private to your caller — other agents and the ontology worker will not recognize them. \
WEIGHT defaults to 1.0; use -1.0 (with `directed: false`) for anti-edges that should DEMOTE the target in graph-related search. \
DIRECTED defaults to false — set to true when the predicate's direction is meaningful (it always is for the canonical set). \
EXTERNAL ENDPOINTS: pass `from_item_id`/`to_item_id` as `ext:{kind}:{key}` (kind/key limited to `[a-zA-Z0-9_-]+`) to link to something outside the item store — no row is stored for it; the endpoint dangles and resolves synthetically (never embedded, never returned by search). \
IDEMPOTENT RETRY (Postgres only): set `metadata.source` to a non-empty string OTHER than `ontology_worker` to get upsert semantics keyed on (from, to, relation, source) — retrying the same call converges to the latest weight/metadata instead of duplicating the edge. Edges with no `source` tag always insert fresh.")]
    async fn create_manual_edge(
        &self,
        Parameters(request): Parameters<CreateManualEdgeRequest>,
    ) -> Result<Json<GraphEdgePayload>, String> {
        let store = self.state.store.clone();
        let input = ManualEdgeInput {
            from_item_id: request.from_item_id,
            to_item_id: request.to_item_id,
            relation: request.relation.map(Cow::Owned),
            sort_order: request.sort_order,
            weight: request.weight.unwrap_or(1.0),
            directed: request.directed.unwrap_or(false),
            metadata: request.metadata,
        };
        let edge = tokio::task::spawn_blocking(move || {
            let edge = store.add_manual_edge(input)?;
            let titles = titles_for_edges(store.as_ref(), std::slice::from_ref(&edge));
            Ok::<_, anyhow::Error>(edge_payload(edge, &titles))
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        crate::api::invalidate_cms_nodes(
            &self.state,
            [edge.from_item_id.clone(), edge.to_item_id.clone()],
        )
        .await
        .map_err(stringify_api_error)?;
        Ok(Json(edge))
    }

    #[tool(description = "Delete a graph edge by id.")]
    async fn delete_graph_edge(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        let store = self.state.store.clone();
        let edge_before_delete = tokio::task::spawn_blocking({
            let store = store.clone();
            let id = id.clone();
            move || store.get_graph_edge(&id)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        let target_id = id.clone();
        let deleted = tokio::task::spawn_blocking(move || store.delete_graph_edge(&target_id))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        if !deleted {
            return Err(format!("graph edge {id} not found"));
        }
        if let Some(edge) = edge_before_delete {
            crate::api::invalidate_cms_nodes(&self.state, [edge.from_item_id, edge.to_item_id])
                .await
                .map_err(stringify_api_error)?;
        }
        Ok(Json(DeleteResponse { id, deleted }))
    }

    #[tool(
        description = "Update the metadata of an existing graph edge (e.g., to confirm a suggested edge by setting metadata.status = 'confirmed')."
    )]
    async fn update_graph_edge(
        &self,
        Parameters(params): Parameters<UpdateGraphEdgeParams>,
    ) -> Result<Json<GraphEdgePayload>, String> {
        let store = self.state.store.clone();
        let id = params.id.clone();
        let relation = params.relation;
        let metadata = params.metadata;
        let sort_order = params.sort_order;
        let edge = tokio::task::spawn_blocking(move || {
            let edge = store.update_graph_edge(&id, relation, metadata, sort_order)?;
            let titles = titles_for_edges(store.as_ref(), std::slice::from_ref(&edge));
            Ok::<_, anyhow::Error>(edge_payload(edge, &titles))
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        crate::api::invalidate_cms_nodes(
            &self.state,
            [edge.from_item_id.clone(), edge.to_item_id.clone()],
        )
        .await
        .map_err(stringify_api_error)?;
        Ok(Json(edge))
    }

    #[tool(
        description = "List graph edges awaiting review. Returns edges where status is 'suggested'."
    )]
    async fn list_ontology_reviews(&self) -> Result<Json<GraphEdgesResponse>, String> {
        let store = self.state.store.clone();
        let edges = tokio::task::spawn_blocking(move || {
            let edges =
                store.list_graph_edges(None, Some(GraphEdgeType::Manual), Some("suggested"))?;
            let titles = titles_for_edges(store.as_ref(), &edges);
            Ok::<_, anyhow::Error>(
                edges
                    .into_iter()
                    .map(|e| edge_payload(e, &titles))
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        Ok(Json(GraphEdgesResponse { edges }))
    }

    #[tool(
        description = "Accept a suggested graph edge. Optionally update its relation (predicate)."
    )]
    async fn accept_ontology_review(
        &self,
        Parameters(params): Parameters<AcceptOntologyReviewParams>,
    ) -> Result<Json<GraphEdgePayload>, String> {
        let store = self.state.store.clone();
        let id = params.id.clone();
        let relation = params.relation;

        let edge = tokio::task::spawn_blocking(move || {
            let record = store
                .get_graph_edge(&id)?
                .ok_or_else(|| anyhow::anyhow!("edge {} not found", id))?;

            let mut metadata = record.metadata.as_object().cloned().unwrap_or_default();
            metadata.insert(
                "status".to_string(),
                serde_json::Value::String("confirmed".to_string()),
            );

            let edge = store.update_graph_edge(
                &id,
                relation,
                serde_json::Value::Object(metadata),
                None,
            )?;
            let titles = titles_for_edges(store.as_ref(), std::slice::from_ref(&edge));
            Ok::<_, anyhow::Error>(edge_payload(edge, &titles))
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;

        Ok(Json(edge))
    }

    #[tool(description = "Reject a suggested graph edge by deleting it.")]
    async fn reject_ontology_review(
        &self,
        Parameters(IdParams { id }): Parameters<IdParams>,
    ) -> Result<Json<DeleteResponse>, String> {
        let store = self.state.store.clone();
        let target = id.clone();
        let deleted = tokio::task::spawn_blocking(move || store.delete_graph_edge(&target))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        Ok(Json(DeleteResponse { id, deleted }))
    }

    // ===== Projection map =================================================

    #[tool(
        description = "Return the projection map (points + clusters). For large stores, \
filter to keep the response small. Optional params: \
`center_id` (sort by 3D distance from this point; populates `distance`), \
`radius` (only points within this distance of `center_id`), \
`cluster` (only this cluster id, post-override), \
`ids` (explicit subset), \
`limit` (cap result count after sort), \
`compact` (drop snippet/tags/cluster_description — ~6× smaller), \
`include_distance` (force distance field even without filtering). \
Response also includes a `summary` block with per-cluster counts so callers \
can browse without fetching every point."
    )]
    async fn map_get(
        &self,
        Parameters(params): Parameters<MapGetParams>,
    ) -> Result<Json<MapGetResponse>, String> {
        let all = crate::api::build_map_points(self.state.clone())
            .await
            .map_err(|e| e.to_string())?;
        let total = all.len();

        // Summary built from full set (pre-filter) so callers always see the
        // whole cluster landscape.
        let summary = build_map_summary(&all);

        let center = params
            .center_id
            .as_ref()
            .and_then(|cid| all.iter().find(|p| &p.id == cid).map(|p| (p.x, p.y, p.z)));
        if params.center_id.is_some() && center.is_none() {
            return Err(format!(
                "center_id not found in map: {}",
                params.center_id.unwrap_or_default()
            ));
        }

        let ids_filter: Option<std::collections::HashSet<String>> =
            params.ids.map(|v| v.into_iter().collect());

        let want_distance = center.is_some()
            && (params.include_distance.unwrap_or(false)
                || params.radius.is_some()
                || params.center_id.is_some());

        let mut points: Vec<crate::projection::MapPoint> = all
            .into_iter()
            .filter(|p| match (&ids_filter, params.cluster) {
                (Some(set), _) if !set.contains(&p.id) => false,
                (_, Some(c)) if p.cluster != c => false,
                _ => true,
            })
            .map(|mut p| {
                if let Some((cx, cy, cz)) = center {
                    let dx = p.x - cx;
                    let dy = p.y - cy;
                    let dz = p.z - cz;
                    let d = (dx * dx + dy * dy + dz * dz).sqrt();
                    if want_distance {
                        p.distance = Some(d);
                    }
                }
                p
            })
            .filter(|p| match (params.radius, p.distance) {
                (Some(r), Some(d)) => d <= r,
                (Some(_), None) => false,
                _ => true,
            })
            .collect();

        if center.is_some() {
            points.sort_by(|a, b| {
                a.distance
                    .unwrap_or(f32::INFINITY)
                    .partial_cmp(&b.distance.unwrap_or(f32::INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        if let Some(lim) = params.limit {
            points.truncate(lim);
        }

        if params.compact.unwrap_or(false) {
            for p in &mut points {
                p.snippet = None;
                p.tags = None;
                p.cluster_description = None;
            }
        }

        Ok(Json(MapGetResponse {
            points,
            summary: Some(MapSummary {
                total_points: total,
                clusters: summary,
            }),
        }))
    }

    #[tool(
        description = "Lightweight cluster index: returns one row per cluster with \
`cluster` id, `name`, `description`, and `count`. Use this before `map_get` to \
pick a target cluster cheaply without loading every point."
    )]
    async fn map_clusters(&self) -> Result<Json<MapClustersResponse>, String> {
        let all = crate::api::build_map_points(self.state.clone())
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(MapClustersResponse {
            clusters: build_map_summary(&all),
        }))
    }

    #[tool(
        description = "Find the k nearest points (3D PCA distance) to `center_id`. \
Optional `exclude_cluster` skips points already in that cluster (useful when \
hunting misclassified neighbours). `k` defaults to 20. The center point itself \
is always excluded."
    )]
    async fn map_nearest(
        &self,
        Parameters(params): Parameters<MapNearestParams>,
    ) -> Result<Json<MapGetResponse>, String> {
        let all = crate::api::build_map_points(self.state.clone())
            .await
            .map_err(|e| e.to_string())?;
        let total = all.len();
        let center = all
            .iter()
            .find(|p| p.id == params.center_id)
            .map(|p| (p.x, p.y, p.z))
            .ok_or_else(|| format!("center_id not found in map: {}", params.center_id))?;
        let k = params.k.unwrap_or(20);

        let mut points: Vec<crate::projection::MapPoint> = all
            .into_iter()
            .filter(|p| p.id != params.center_id)
            .filter(|p| match params.exclude_cluster {
                Some(c) => p.cluster != c,
                None => true,
            })
            .map(|mut p| {
                let dx = p.x - center.0;
                let dy = p.y - center.1;
                let dz = p.z - center.2;
                p.distance = Some((dx * dx + dy * dy + dz * dz).sqrt());
                p
            })
            .collect();
        points.sort_by(|a, b| {
            a.distance
                .unwrap_or(f32::INFINITY)
                .partial_cmp(&b.distance.unwrap_or(f32::INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        points.truncate(k);

        if params.compact.unwrap_or(true) {
            for p in &mut points {
                p.snippet = None;
                p.tags = None;
                p.cluster_description = None;
            }
        }

        Ok(Json(MapGetResponse {
            points,
            summary: Some(MapSummary {
                total_points: total,
                clusters: vec![],
            }),
        }))
    }

    #[tool(description = "Move an item to a different cluster. Two ways: \
(a) `anchor_id` — pin to the cluster of another item; survives rebuilds even \
when numeric cluster ids shuffle (RECOMMENDED). \
(b) `cluster` — numeric id; legacy, breaks if the algorithm renumbers \
clusters. Pass `clear: true` to drop the override and revert to the \
algorithm-assigned cluster on next rebuild. The change is reflected \
immediately in `map_get`.")]
    async fn map_reassign(
        &self,
        Parameters(params): Parameters<MapReassignParams>,
    ) -> Result<Json<MapReassignResponse>, String> {
        let state = self.state.clone();
        let id = params.id.clone();
        let store = state.store.clone();
        let id_for_fetch = id.clone();
        let item = tokio::task::spawn_blocking(move || store.get_item(&id_for_fetch))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("item not found: {id}"))?;

        let mut metadata = item.metadata.clone();
        if !metadata.is_object() {
            metadata = serde_json::json!({});
        }
        let obj = metadata.as_object_mut().expect("metadata is object");
        let mut proj = obj
            .get("projection")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let clear = params.clear.unwrap_or(false);

        let mut resolved_cluster: Option<usize> = None;
        let mut resolved_anchor: Option<String> = None;

        if clear {
            proj.remove("cluster_override");
            proj.remove("cluster_anchor_id");
        } else if let Some(anchor_id) = params.anchor_id.as_ref() {
            // Resolve anchor's current raw cluster so `cluster` shows the
            // right bucket immediately. The anchor itself is what survives
            // rebuilds though.
            let store = state.store.clone();
            let anchor_lookup = anchor_id.clone();
            let anchor_item = tokio::task::spawn_blocking(move || store.get_item(&anchor_lookup))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("anchor_id not found: {anchor_id}"))?;
            let anchor_raw = anchor_item
                .metadata
                .get("projection")
                .and_then(|p| {
                    p.get("cluster_raw")
                        .or_else(|| p.get("cluster"))
                        .and_then(|v| v.as_u64())
                })
                .ok_or_else(|| format!("anchor_id has no projection cluster yet: {anchor_id}"))?
                as usize;
            proj.insert("cluster_anchor_id".into(), serde_json::json!(anchor_id));
            proj.remove("cluster_override");
            proj.insert("cluster".into(), serde_json::json!(anchor_raw));
            resolved_anchor = Some(anchor_id.clone());
            resolved_cluster = Some(anchor_raw);
        } else if let Some(cluster) = params.cluster {
            proj.insert("cluster_override".into(), serde_json::json!(cluster));
            proj.remove("cluster_anchor_id");
            proj.insert("cluster".into(), serde_json::json!(cluster));
            resolved_cluster = Some(cluster);
        } else {
            return Err("provide anchor_id, cluster, or clear=true".into());
        }

        obj.insert("projection".into(), serde_json::Value::Object(proj));

        let store = state.store.clone();
        let id_for_write = id.clone();
        let meta_for_write = metadata.clone();
        tokio::task::spawn_blocking(move || {
            store.update_item_metadata(&id_for_write, meta_for_write)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        Ok(Json(MapReassignResponse {
            id,
            cluster: resolved_cluster,
            anchor_id: resolved_anchor,
            cleared: clear,
        }))
    }

    #[tool(
        description = "Kick off a background rebuild of the projection map (KMeans + PCA \
on item embeddings, then LLM cluster labelling). Set `RAG_PROJECTION_ALGO=hdbscan` \
on the server to use HDBSCAN instead. Returns immediately; a second call while a \
rebuild is running is a no-op."
    )]
    async fn map_rebuild(&self) -> Result<Json<MapRebuildResponse>, String> {
        if self.state.projection_worker.is_processing().await {
            return Ok(Json(MapRebuildResponse {
                status: "already_running".into(),
            }));
        }
        self.state
            .projection_worker
            .run_rebuild(self.state.http_client.clone(), self.state.analysis.clone())
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(MapRebuildResponse {
            status: "started".into(),
        }))
    }
}

/// Build the `StreamableHttpService` tower service that serves admin MCP
/// traffic at `/mcp/admin`. Same guards as the other MCP endpoints.
pub fn streamable_http_service(
    state: AppState,
) -> StreamableHttpService<AdminMcpServer, LocalSessionManager> {
    let allowed_hosts = state.mcp_allowed_hosts();
    let factory_state = state;
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts)
        .with_sse_keep_alive(Some(Duration::from_secs(15)))
        .with_stateful_mode(false)
        .with_json_response(true);
    StreamableHttpService::new(
        move || Ok(AdminMcpServer::new(factory_state.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}

#[cfg(test)]
mod link_peers_tests {
    use super::*;
    use crate::db::{GraphConfig, ItemRecord, LooseLinkRule, ManualEdgeInput, SqliteVectorStore};
    use serde_json::json;
    use std::{
        borrow::Cow,
        sync::atomic::{AtomicUsize, Ordering},
    };

    fn store() -> Arc<dyn VectorStore> {
        static NEXT_DB_ID: AtomicUsize = AtomicUsize::new(0);
        let db_id = NEXT_DB_ID.fetch_add(1, Ordering::Relaxed);
        let uri = format!("file:link-peers-test-{db_id}?mode=memory&cache=shared");
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

    fn rules() -> Vec<LooseLinkRule> {
        vec![
            LooseLinkRule {
                metadata_key: "repo".to_owned(),
                relation: "shares_repo".to_owned(),
                weight: 0.6,
            },
            LooseLinkRule {
                metadata_key: "component".to_owned(),
                relation: "same_component".to_owned(),
                weight: 0.7,
            },
        ]
    }

    fn put(store: &Arc<dyn VectorStore>, id: &str, metadata: serde_json::Value) {
        store
            .upsert_item(
                ItemRecord {
                    id: id.to_owned(),
                    text: format!("body of {id}"),
                    metadata,
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

    fn params(id: &str) -> LinkPeersParams {
        LinkPeersParams {
            id: id.to_owned(),
            key: None,
            limit: None,
        }
    }

    async fn resolve(store: Arc<dyn VectorStore>, params: LinkPeersParams) -> LinkPeersResponse {
        let Json(response) = resolve_link_peers(store, rules(), params)
            .await
            .expect("resolving peers");
        response
    }

    #[tokio::test]
    async fn finds_peers_sharing_a_metadata_value() {
        let store = store();
        put(&store, "a", json!({"repo": "rust-rag"}));
        put(&store, "b", json!({"repo": "rust-rag"}));
        put(&store, "c", json!({"repo": "other"}));
        store.rebuild_loose_links(&rules(), 0).unwrap();

        let response = resolve(store, params("a")).await;

        assert!(!response.stale);
        assert_eq!(response.groups.len(), 1);
        let group = &response.groups[0];
        assert_eq!(group.metadata_key, "repo");
        assert_eq!(group.value, "rust-rag");
        assert_eq!(group.hub_id, "ext:repo:rust-rag");
        assert_eq!(group.relation.as_deref(), Some("shares_repo"));
        // b only — c is a different repo, and the anchor/hub are excluded.
        let ids: Vec<&str> = group.peers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["b"]);
    }

    #[tokio::test]
    async fn groups_by_every_matching_key() {
        let store = store();
        put(
            &store,
            "a",
            json!({"repo": "rust-rag", "component": "ingest"}),
        );
        put(&store, "b", json!({"repo": "rust-rag"}));
        put(&store, "c", json!({"component": "ingest"}));
        store.rebuild_loose_links(&rules(), 0).unwrap();

        let response = resolve(store, params("a")).await;

        assert_eq!(response.groups.len(), 2);
        let by_key = |key: &str| -> &LinkPeerGroup {
            response
                .groups
                .iter()
                .find(|g| g.metadata_key == key)
                .expect("group present")
        };
        let repo: Vec<&str> = by_key("repo").peers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(repo, vec!["b"]);
        let component: Vec<&str> = by_key("component")
            .peers
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(component, vec!["c"]);
    }

    #[tokio::test]
    async fn key_filter_narrows_to_one_group() {
        let store = store();
        put(
            &store,
            "a",
            json!({"repo": "rust-rag", "component": "ingest"}),
        );
        put(&store, "b", json!({"repo": "rust-rag"}));
        put(&store, "c", json!({"component": "ingest"}));
        store.rebuild_loose_links(&rules(), 0).unwrap();

        let mut request = params("a");
        request.key = Some("component".to_owned());
        let response = resolve(store, request).await;

        assert_eq!(response.groups.len(), 1);
        assert_eq!(response.groups[0].metadata_key, "component");
    }

    #[tokio::test]
    async fn normalizes_case_the_same_way_the_planner_does() {
        let store = store();
        put(&store, "a", json!({"repo": "Rust-RAG"}));
        put(&store, "b", json!({"repo": "  rust-rag "}));
        store.rebuild_loose_links(&rules(), 0).unwrap();

        // The anchor's raw value differs from its cluster's normalized value;
        // the tool must normalize the same way or it resolves the wrong hub.
        let response = resolve(store, params("a")).await;
        assert_eq!(response.groups.len(), 1);
        assert_eq!(response.groups[0].hub_id, "ext:repo:rust-rag");
        assert_eq!(response.groups[0].peers[0].id, "b");
    }

    #[tokio::test]
    async fn reports_stale_before_the_first_rebuild() {
        let store = store();
        put(&store, "a", json!({"repo": "rust-rag"}));
        put(&store, "b", json!({"repo": "rust-rag"}));

        // No rebuild yet: no loose-link edges exist, so there is nothing to read.
        let response = resolve(store, params("a")).await;
        assert!(response.stale);
        assert!(response.groups.is_empty());
    }

    #[tokio::test]
    async fn reports_stale_for_a_lone_entry() {
        let store = store();
        put(&store, "a", json!({"repo": "rust-rag"}));
        put(&store, "b", json!({"repo": "other"}));
        store.rebuild_loose_links(&rules(), 0).unwrap();

        let response = resolve(store, params("a")).await;
        assert!(response.stale, "singleton cluster has no peers to report");
    }

    #[tokio::test]
    async fn ignores_hand_curated_edges_on_the_hub() {
        // A peer linked to the hub by hand must not show up as a loose-link peer.
        let store = store();
        put(&store, "a", json!({"repo": "rust-rag"}));
        put(&store, "b", json!({"repo": "rust-rag"}));
        put(&store, "manual-only", json!({}));
        store.rebuild_loose_links(&rules(), 0).unwrap();
        store
            .add_manual_edge(ManualEdgeInput {
                from_item_id: "manual-only".to_owned(),
                to_item_id: "ext:repo:rust-rag".to_owned(),
                relation: Some(Cow::Borrowed("part_of")),
                sort_order: None,
                weight: 1.0,
                directed: false,
                metadata: json!({}),
            })
            .expect("seeding manual edge");

        let response = resolve(store, params("a")).await;
        let ids: Vec<&str> = response.groups[0]
            .peers
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(ids, vec!["b"], "hand-curated neighbor leaked into peers");
    }

    #[tokio::test]
    async fn respects_the_limit() {
        let store = store();
        put(&store, "a", json!({"repo": "rust-rag"}));
        for id in ["b", "c", "d"] {
            put(&store, id, json!({"repo": "rust-rag"}));
        }
        store.rebuild_loose_links(&rules(), 0).unwrap();

        let mut request = params("a");
        request.limit = Some(2);
        let response = resolve(store, request).await;
        assert_eq!(response.groups[0].peers.len(), 2);
    }

    #[tokio::test]
    async fn missing_entry_is_an_error_not_an_empty_result() {
        let store = store();
        put(&store, "a", json!({"repo": "rust-rag"}));
        put(&store, "b", json!({"repo": "rust-rag"}));

        let Err(err) = resolve_link_peers(store, rules(), params("nope")).await else {
            panic!("missing entry should fail loudly");
        };
        assert!(err.contains("nope"), "unhelpful error: {err}");
    }

    #[test]
    fn snippet_truncates_on_a_char_boundary() {
        assert_eq!(snippet("  hello  ", 10), "hello");
        assert_eq!(snippet("abc", 10), "abc");
        assert_eq!(snippet("åäö", 2), "åä…");
        assert_eq!(snippet("", 10), "");
        // Multi-byte input must not panic or split a code point.
        assert_eq!(snippet("日本語テキスト", 3), "日本語…");
    }
}
