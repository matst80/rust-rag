pub mod api;
pub mod config;
pub mod db;
pub mod embedding;

use axum::Router;

pub fn build_app(state: api::AppState) -> Router {
    api::router(state)
}
