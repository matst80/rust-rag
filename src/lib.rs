pub mod acp_discovery;
pub mod acp_ws;
pub mod api;
pub mod chunking_md;
pub mod config;
pub mod crypto;
pub mod db;
pub mod embedding;
pub mod manager;
pub mod mcp;
pub mod ontology;
pub mod reranker;
pub mod validation;

use axum::Router;

pub fn build_app(state: api::AppState) -> Router {
    api::router(state)
}
