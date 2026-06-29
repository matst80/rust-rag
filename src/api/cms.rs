use axum::{
    Json,
    extract::{Path, State},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::GraphEdgeRecord;

use super::error::{ApiError, metadata_schema};
use super::items::AdminItemPayload;
use super::state::AppState;

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CmsEdgePayload {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relationship: String,
    pub edge_type: String,
    pub sort_order: String,
    pub weight: f32,
    pub directed: bool,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CmsTreeChildPayload {
    pub edge: CmsEdgePayload,
    pub node: Box<CmsTreeNodePayload>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CmsTreeNodePayload {
    pub entry: AdminItemPayload,
    pub children: Vec<CmsTreeChildPayload>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CmsTreeResponse {
    pub root_id: String,
    pub tree: CmsTreeNodePayload,
}

impl From<GraphEdgeRecord> for CmsEdgePayload {
    fn from(value: GraphEdgeRecord) -> Self {
        Self {
            id: value.id,
            source_id: value.from_item_id,
            target_id: value.to_item_id,
            relationship: value
                .relation
                .clone()
                .unwrap_or_else(|| value.edge_type.as_str().to_owned()),
            edge_type: value.edge_type.as_str().to_owned(),
            sort_order: value.sort_order,
            weight: value.weight,
            directed: value.directed,
            metadata: value.metadata,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<crate::cms::CmsTreeChild> for CmsTreeChildPayload {
    fn from(value: crate::cms::CmsTreeChild) -> Self {
        Self {
            edge: value.edge.into(),
            node: Box::new(value.node.into()),
        }
    }
}

impl From<crate::cms::CmsTreeNode> for CmsTreeNodePayload {
    fn from(value: crate::cms::CmsTreeNode) -> Self {
        Self {
            entry: value.entry.into(),
            children: value.children.into_iter().map(Into::into).collect(),
        }
    }
}

pub(crate) async fn get_cms_tree(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CmsTreeResponse>, ApiError> {
    let cms_runtime = state.cms_runtime.clone();
    let root_id = id.clone();
    let tree = tokio::task::spawn_blocking(move || cms_runtime.build_tree(&root_id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(CmsTreeResponse {
        root_id: id,
        tree: tree.into(),
    }))
}
