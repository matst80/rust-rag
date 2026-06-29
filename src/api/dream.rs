use axum::{extract::State, http::StatusCode};
use tracing::error;

use super::dreaming;
use super::error::ApiError;
use super::state::AppState;

pub(crate) async fn dreaming_endpoint(
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    if !state.analysis.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "dreaming requires analysis (LLM) to be configured".to_owned(),
        ));
    }

    // Spawn in background so the HTTP request doesn't timeout
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = dreaming::process_dreaming_round(&state_clone).await {
            error!("manual dreaming error: {e}");
        }
    });

    Ok(StatusCode::ACCEPTED)
}
