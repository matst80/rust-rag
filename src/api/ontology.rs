//! Admin endpoints that force the ontology worker to run on demand —
//! useful for verifying the LLM wiring without waiting for the next tick
//! and for re-extracting edges on a specific entry after it's been edited.

use super::{ApiError, AppState};
use crate::ontology::{self, OntologyRunReport};
use anyhow::anyhow;
use axum::{Json, extract::{Path, State}};
use tracing::info;

#[tracing::instrument(name = "ontology.admin.run_batch", skip(state))]
pub async fn run_batch(
    State(state): State<AppState>,
) -> Result<Json<OntologyRunReport>, ApiError> {
    let model = ensure_configured(&state)?;
    info!(
        model = %model,
        base_url = %state.ontology_llm.base_url.as_deref().unwrap_or(""),
        "admin: forcing ontology batch run"
    );
    let report = ontology::run_once(
        &state.store,
        &state.embedder,
        &state.http_client,
        &state.ontology_llm,
        &model,
        &state.ontology,
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.starts_with("embedder not ready") {
            ApiError::ServiceUnavailable(msg)
        } else {
            ApiError::Internal(anyhow!(msg))
        }
    })?;
    Ok(Json(report))
}

#[tracing::instrument(name = "ontology.admin.run_for_item", skip(state), fields(item_id = %id))]
pub async fn run_for_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OntologyRunReport>, ApiError> {
    let model = ensure_configured(&state)?;
    info!(
        model = %model,
        item_id = %id,
        "admin: forcing ontology run for item"
    );
    let report = ontology::run_for_item(
        &state.store,
        &state.embedder,
        &state.http_client,
        &state.ontology_llm,
        &model,
        &state.ontology,
        &id,
    )
    .await
    .map_err(|e| classify(e, &id))?;
    Ok(Json(report))
}

/// Map worker errors to the right HTTP status. `anyhow::Error` is opaque, so
/// we string-match the well-known prefixes the ontology module emits — keeps
/// "item not found" out of the 500 bucket where it hides real failures.
fn classify(err: anyhow::Error, id: &str) -> ApiError {
    let msg = err.to_string();
    if msg.starts_with("item not found") {
        return ApiError::NotFound(format!("item not found: {id}"));
    }
    if msg.starts_with("embedder not ready") {
        return ApiError::ServiceUnavailable(msg);
    }
    ApiError::Internal(anyhow!(msg))
}

fn ensure_configured(state: &AppState) -> Result<String, ApiError> {
    if !state.ontology_llm.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "ontology LLM not configured (set RAG_ANALYSIS_BASE_URL/MODEL or RAG_OPENAI_API_BASE_URL/MODEL)"
                .to_owned(),
        ));
    }
    state
        .ontology_llm
        .default_model
        .clone()
        .ok_or_else(|| {
            ApiError::ServiceUnavailable("ontology LLM model not set".to_owned())
        })
}
