pub mod acp_discovery;
pub mod acp_mcp;
pub mod acp_ws;
pub mod admin_mcp;
pub mod api;
pub mod chunking_md;
pub mod cms;
pub mod config;
pub mod crypto;
pub mod db;
pub mod embedding;
pub mod integrations;
pub mod manager;
pub mod mcp;
pub mod notify;
pub mod ontology;
pub mod projection;
pub mod reranker;
pub mod validation;

use axum::Router;

pub fn build_app(state: api::AppState) -> Router {
    api::router(state)
}
