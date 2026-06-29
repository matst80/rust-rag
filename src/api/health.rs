use axum::{Json, extract::State, http::StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::state::AppState;

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct HealthResponse {
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionResponse {
    pub authenticated: bool,
    pub auth_enabled: bool,
    pub user: Option<SessionUser>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionUser {
    pub name: Option<String>,
    pub email: Option<String>,
    pub preferred_username: Option<String>,
}

pub(crate) async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    state.embedder.health()
}
