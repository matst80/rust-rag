pub mod api;
pub mod config;
pub mod db;
pub mod embedding;
pub mod mcp;
pub mod ontology;

use axum::Router;

pub fn build_app(state: api::AppState) -> Router {
    api::router(state)
}
