use axum::{
    extract::{Path, Query, State},
    Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::{SchemaRecord, VectorStore};
use crate::validation;

use super::{api_validation_error, ApiError, AppState};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SchemaPayload {
    pub type_name: String,
    pub json_schema: Value,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Count of items currently typed with this schema.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub item_count: Option<i64>,
}

impl SchemaPayload {
    pub fn from_record_pub(record: SchemaRecord, item_count: Option<i64>) -> Self {
        Self::from_record(record, item_count)
    }
    fn from_record(record: SchemaRecord, item_count: Option<i64>) -> Self {
        Self {
            type_name: record.type_name,
            json_schema: record.json_schema,
            title: record.title,
            description: record.description,
            created_at: record.created_at,
            updated_at: record.updated_at,
            item_count,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SchemaListResponse {
    pub schemas: Vec<SchemaPayload>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpsertSchemaRequest {
    pub type_name: Option<String>,
    pub json_schema: Value,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct DeleteSchemaQuery {
    #[serde(default)]
    pub force: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSchemaResponse {
    pub type_name: String,
    pub deleted: bool,
    /// Number of items whose `type` was set to NULL as a result of force-delete.
    pub items_unset: usize,
}

/// Fetch `(record, item_count)` for one schema on a blocking worker.
fn fetch_with_count(
    store: &dyn VectorStore,
    type_name: &str,
) -> anyhow::Result<Option<(SchemaRecord, i64)>> {
    let Some(record) = store.get_schema(type_name)? else {
        return Ok(None);
    };
    let count = store.count_items_by_type(&record.type_name).unwrap_or(0);
    Ok(Some((record, count)))
}

pub async fn list_schemas(
    State(state): State<AppState>,
) -> Result<Json<SchemaListResponse>, ApiError> {
    let store = state.store.clone();
    let pairs = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<(SchemaRecord, i64)>> {
        let records = store.list_schemas()?;
        let mut out = Vec::with_capacity(records.len());
        for record in records {
            let count = store.count_items_by_type(&record.type_name).unwrap_or(0);
            out.push((record, count));
        }
        Ok(out)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;
    let schemas = pairs
        .into_iter()
        .map(|(record, count)| SchemaPayload::from_record(record, Some(count)))
        .collect();
    Ok(Json(SchemaListResponse { schemas }))
}

pub async fn get_schema(
    State(state): State<AppState>,
    Path(type_name): Path<String>,
) -> Result<Json<SchemaPayload>, ApiError> {
    let store = state.store.clone();
    let tn = type_name.clone();
    let pair = tokio::task::spawn_blocking(move || fetch_with_count(store.as_ref(), &tn))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound(format!("schema `{type_name}` not found")))?;
    Ok(Json(SchemaPayload::from_record(pair.0, Some(pair.1))))
}

async fn upsert_inner(
    state: &AppState,
    type_name: String,
    request: UpsertSchemaRequest,
) -> Result<(SchemaRecord, i64), ApiError> {
    if type_name.trim().is_empty() {
        return Err(ApiError::BadRequest("type_name is required".into()));
    }
    validation::validate_meta_schema(&request.json_schema).map_err(api_validation_error)?;
    let record = SchemaRecord {
        type_name: type_name.clone(),
        json_schema: request.json_schema,
        title: request.title,
        description: request.description,
        created_at: 0,
        updated_at: 0,
    };
    let store = state.store.clone();
    let to_store = record.clone();
    let tn = type_name.clone();
    let pair = tokio::task::spawn_blocking(move || -> anyhow::Result<(SchemaRecord, i64)> {
        store.upsert_schema(to_store)?;
        match fetch_with_count(store.as_ref(), &tn)? {
            Some(p) => Ok(p),
            None => Ok((
                SchemaRecord {
                    type_name: tn,
                    json_schema: Value::Null,
                    title: None,
                    description: None,
                    created_at: 0,
                    updated_at: 0,
                },
                0,
            )),
        }
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;
    state.schema_cache.invalidate(&type_name);
    // Fall back to the in-memory record when the store returned a stub.
    let (stored, count) = if pair.0.json_schema.is_null() {
        (record, pair.1)
    } else {
        pair
    };
    Ok((stored, count))
}

pub async fn create_schema(
    State(state): State<AppState>,
    Json(request): Json<UpsertSchemaRequest>,
) -> Result<Json<SchemaPayload>, ApiError> {
    let type_name = request
        .type_name
        .clone()
        .ok_or_else(|| ApiError::BadRequest("type_name is required".into()))?;
    let (record, count) = upsert_inner(&state, type_name, request).await?;
    Ok(Json(SchemaPayload::from_record(record, Some(count))))
}

pub async fn upsert_schema(
    State(state): State<AppState>,
    Path(type_name): Path<String>,
    Json(request): Json<UpsertSchemaRequest>,
) -> Result<Json<SchemaPayload>, ApiError> {
    let (record, count) = upsert_inner(&state, type_name, request).await?;
    Ok(Json(SchemaPayload::from_record(record, Some(count))))
}

pub async fn delete_schema(
    State(state): State<AppState>,
    Path(type_name): Path<String>,
    Query(query): Query<DeleteSchemaQuery>,
) -> Result<Json<DeleteSchemaResponse>, ApiError> {
    let force = query.force.unwrap_or(false);
    let store = state.store.clone();
    let tn = type_name.clone();
    let (deleted, unset) = tokio::task::spawn_blocking(move || store.delete_schema(&tn, force))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(|e| {
            if e.to_string().contains("referenced by") {
                ApiError::Conflict(e.to_string())
            } else {
                ApiError::Internal(e)
            }
        })?;
    if !deleted {
        return Err(ApiError::NotFound(format!(
            "schema `{type_name}` not found"
        )));
    }
    state.schema_cache.invalidate(&type_name);
    Ok(Json(DeleteSchemaResponse {
        type_name,
        deleted,
        items_unset: unset,
    }))
}
