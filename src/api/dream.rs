use axum::{extract::State, http::StatusCode};
use tracing::error;

use super::dreaming;
use super::error::ApiError;
use super::state::AppState;

pub(crate) async fn dreaming_endpoint(
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    // Deterministic corpus round: no LLM needed. Spawn in background so the
    // HTTP request doesn't timeout.
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = dreaming::process_dreaming_round(&state_clone).await {
            error!("manual dreaming error: {e}");
        }
    });

    Ok(StatusCode::ACCEPTED)
}
