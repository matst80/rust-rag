use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse {
    pub(crate) error: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    ServiceUnavailable(String),
    #[error(transparent)]
    Internal(anyhow::Error),
    #[error(transparent)]
    TaskJoin(#[from] tokio::task::JoinError),
    /// Structured-data validation failure. Returned as 400 with a JSON body
    /// listing the field-level errors.
    #[error("schema validation failed for type `{type_name}`")]
    SchemaValidation {
        type_name: String,
        errors: Vec<String>,
    },
    #[error("unknown type `{0}`")]
    UnknownType(String),
    #[error("invalid schema: {0}")]
    InvalidSchema(String),
    #[error("conflict: {0}")]
    Conflict(String),
}

pub(crate) fn api_validation_error(e: crate::validation::ValidationError) -> ApiError {
    use crate::validation::ValidationError;
    match e {
        ValidationError::UnknownType(t) => ApiError::UnknownType(t),
        ValidationError::InvalidSchema { message, .. } => ApiError::InvalidSchema(message),
        ValidationError::Failed { type_name, errors } => {
            ApiError::SchemaValidation { type_name, errors }
        }
        ValidationError::Storage(e) => ApiError::Internal(e),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::SchemaValidation { type_name, errors } = &self {
            let body = serde_json::json!({
                "error": "schema_validation_failed",
                "type": type_name,
                "validation_errors": errors,
            });
            return (StatusCode::BAD_REQUEST, Json(body)).into_response();
        }
        let (status, error_message) = match &self {
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message.clone()),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message.clone()),
            Self::ServiceUnavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message.clone()),
            Self::Internal(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::TaskJoin(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::SchemaValidation { .. } => unreachable!(),
            Self::UnknownType(t) => (StatusCode::BAD_REQUEST, format!("unknown type `{t}`")),
            Self::InvalidSchema(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Self::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
        };

        if status.is_server_error() {
            tracing::error!(
                status = %status,
                error = %self,
                "api request failed"
            );
        }

        (
            status,
            Json(ErrorResponse {
                error: error_message,
            }),
        )
            .into_response()
    }
}

pub(crate) fn map_missing_item(kind: &str, error: anyhow::Error) -> ApiError {
    let error_string = error.to_string();
    if error_string.contains("not found") {
        ApiError::NotFound(error_string)
    } else {
        ApiError::Internal(error.context(format!("failed to update {kind}")))
    }
}

pub(crate) fn map_graph_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("graph support is disabled") {
        ApiError::ServiceUnavailable(message)
    } else if message.contains("not found") {
        ApiError::NotFound(message)
    } else if message.contains("must") || message.contains("distinct") {
        ApiError::BadRequest(message)
    } else {
        ApiError::Internal(error.context("graph operation failed"))
    }
}

pub(crate) fn current_timestamp_millis() -> Result<i64, ApiError> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;
    Ok(now.as_millis() as i64)
}

pub(crate) fn validate_metadata(metadata: &serde_json::Value) -> Result<(), ApiError> {
    if !metadata.is_object() {
        return Err(ApiError::BadRequest(
            "metadata must be a JSON object".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_source_id(source_id: &str) -> Result<(), ApiError> {
    if source_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "source_id must not be empty".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_non_empty(field: &str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::BadRequest(format!("{field} must not be empty")));
    }
    Ok(())
}

pub(crate) fn resolve_store_id(id: Option<String>) -> String {
    match id {
        Some(id) => {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                uuid::Uuid::now_v7().to_string()
            } else {
                trimmed.to_owned()
            }
        }
        None => uuid::Uuid::now_v7().to_string(),
    }
}

pub(crate) fn validate_graph_depth(depth: usize) -> Result<(), ApiError> {
    if depth > 5 {
        return Err(ApiError::BadRequest(
            "depth must be less than or equal to 5".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_graph_limit(limit: usize) -> Result<(), ApiError> {
    if limit == 0 || limit > 500 {
        return Err(ApiError::BadRequest(
            "limit must be between 1 and 500".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) fn default_metadata() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

pub(crate) fn metadata_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let serde_json::Value::Object(map) = serde_json::json!({
        "type": ["object", "null"],
        "additionalProperties": true,
        "description": "Free-form JSON object of string-keyed metadata.",
    }) else {
        unreachable!()
    };
    schemars::Schema::from(map)
}
