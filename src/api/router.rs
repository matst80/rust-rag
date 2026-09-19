use axum::{
    Router, middleware,
    routing::{delete, get, post},
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};

use super::auth;
use super::auth_guard::require_api_key;
use super::state::AppState;
use super::{
    acp, analysis, attachments, cms, code, collab, dream, graph, harness, health, ingest_url,
    integrations, items, map, messages, multimodal, ontology, openai, openapi, push, query,
    schemas, store_search, whisper,
};

pub fn router(state: AppState) -> Router {
    let protected_routes = Router::new()
        .route("/store", post(store_search::store))
        .route("/api/store", post(store_search::store))
        .route("/api/store/smart", post(store_search::smart_store))
        .route("/api/store/analyze", post(analysis::analyze_endpoint))
        .route("/search", post(store_search::search))
        .route("/api/search", post(store_search::search))
        .route(
            "/api/openai/v1/chat/completions",
            post(openai::chat_completions),
        )
        .route("/api/query/assisted", post(query::assisted_query))
        .route("/graph/status", get(graph::graph_status))
        .route("/api/graph/status", get(graph::graph_status))
        .route("/graph/edges", get(graph::list_graph_edges))
        .route("/api/graph/edges", get(graph::list_graph_edges))
        .route("/graph/neighborhood/{id}", get(graph::graph_neighborhood))
        .route(
            "/api/graph/neighborhood/{id}",
            get(graph::graph_neighborhood),
        )
        .route("/harness/tree", get(harness::harness_tree))
        .route("/api/harness/tree", get(harness::harness_tree))
        .route("/api/harness/docs", post(harness::ingest_harness_doc))
        .route("/api/cms/tree/{id}", get(cms::get_cms_tree))
        .route("/api/map", get(map::get_map))
        .route("/admin/map/rebuild", post(map::rebuild_map))
        .route("/admin/categories", get(items::list_categories))
        .route("/admin/items", get(items::list_items))
        .route("/admin/tokens/count", post(store_search::count_tokens))
        .route(
            "/admin/items/{id}",
            get(items::get_item)
                .put(items::update_item)
                .delete(items::delete_item),
        )
        .route(
            "/admin/items/{id}/share",
            get(items::get_item_share)
                .post(items::create_item_share)
                .delete(items::revoke_item_share),
        )
        .route(
            "/api/items/{id}/share",
            get(items::get_item_share)
                .post(items::create_item_share)
                .delete(items::revoke_item_share),
        )
        .route("/admin/items/{id}/reanalyze", post(items::reanalyze_item))
        .route(
            "/admin/items/{id}/rechunk",
            post(store_search::rechunk_item),
        )
        .route(
            "/admin/items/{id}/llm-rechunk",
            post(store_search::llm_rechunk_item),
        )
        .route("/admin/graph/rebuild", post(graph::rebuild_graph))
        .route("/admin/graph/duplicates", get(graph::list_duplicate_edges))
        .route("/admin/graph/edges", post(graph::create_manual_edge))
        .route("/admin/ontology/run", post(ontology::run_batch))
        .route("/admin/ontology/run/{id}", post(ontology::run_for_item))
        .route(
            "/admin/graph/edges/{id}",
            axum::routing::patch(graph::update_graph_edge).delete(graph::delete_graph_edge),
        )
        .route("/api/ingest/image", post(multimodal::ingest_image))
        .route("/api/ingest/url", post(ingest_url::ingest_url))
        .route("/api/code/upload", post(code::upload_codesearch_db).layer(axum::extract::DefaultBodyLimit::max(code::CODE_SNAPSHOT_MAX_BYTES as usize)))
        .route("/api/code/search", post(code::search_code))
        .route("/api/code/repos", get(code::list_code_repos))
        .route("/api/code/repos/{name}", delete(code::delete_code_repo))
        .route(
            "/api/code/repos/{name}/files",
            get(code::list_code_files),
        )
        .route(
            "/api/code/repos/{name}/files/{*path}",
            get(code::get_code_file),
        )
        .route("/api/attachments", post(attachments::upload_multipart))
        .route(
            "/api/attachments/from-url",
            post(attachments::attach_from_url),
        )
        .route(
            "/api/attachments/{id}",
            delete(attachments::delete_attachment),
        )
        .route(
            "/api/items/{id}/attachments",
            get(attachments::list_for_item),
        )
        .route("/api/entries/tree", get(attachments::entries_tree))
        .route("/api/entries/paths", get(attachments::entries_paths))
        .route(
            "/api/schemas",
            get(schemas::list_schemas).post(schemas::create_schema),
        )
        .route(
            "/api/schemas/{type_name}",
            get(schemas::get_schema)
                .put(schemas::upsert_schema)
                .delete(schemas::delete_schema),
        )
        .route(
            "/api/messages",
            post(messages::send_message).get(messages::list_messages),
        )
        .route(
            "/api/messages/channels",
            get(messages::list_message_channels),
        )
        .route("/api/acp/instances", get(acp::list_acp_instances))
        .route("/api/acp/select", post(acp::select_acp_instance))
        .route("/api/acp/register", post(acp::register_acp_instance))
        .route("/api/acp/heartbeat", post(acp::heartbeat_acp_instance))
        .route("/api/acp/ws", get(acp::acp_ws_proxy))
        .route("/api/collab/ws", get(collab::collab_ws_handler))
        .route("/api/whisper/ws", get(whisper::whisper_proxy))
        .route(
            "/api/acp/register/{name}",
            delete(acp::unregister_acp_instance),
        )
        .route(
            "/api/messages/channels/{channel}",
            delete(messages::clear_message_channel),
        )
        .route(
            "/api/messages/{id}",
            axum::routing::patch(messages::update_message).delete(messages::delete_message),
        )
        .route(
            "/api/integrations/google/status",
            get(integrations::google::status),
        )
        .route(
            "/api/integrations/google/start",
            get(integrations::google::start),
        )
        .route(
            "/api/integrations/google/callback",
            get(integrations::google::callback),
        )
        .route(
            "/api/integrations/google/disconnect",
            post(integrations::google::disconnect),
        )
        .route(
            "/api/integrations/google/drive/search",
            get(integrations::google::drive_search),
        )
        .route(
            "/api/integrations/google/drive/fetch/{id}",
            get(integrations::google::drive_fetch),
        )
        .route("/api/push/vapid-public-key", get(push::vapid_public_key))
        .route("/api/push/subscribe", post(push::subscribe))
        .route("/api/push/subscriptions", get(push::list_subscriptions))
        .route(
            "/api/push/subscriptions/{id}",
            delete(push::delete_subscription),
        )
        .route("/api/notify", post(push::notify))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ))
        .with_state(state.clone());

    let mcp_cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([axum::http::HeaderName::from_static("mcp-session-id")]);
    let mcp_router = Router::new()
        .route_service("/mcp", crate::mcp::streamable_http_service(state.clone()))
        .route_service(
            "/mcp/acp",
            crate::acp_mcp::streamable_http_service(state.clone()),
        )
        .route_service(
            "/mcp/admin",
            crate::admin_mcp::streamable_http_service(state.clone()),
        )
        .route("/api/dream", post(dream::dreaming_endpoint))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ))
        .layer(mcp_cors)
        .with_state(state.clone());

    let upload_path = state.upload_path.as_str().to_owned();
    Router::new()
        .route("/healthz", get(health::health))
        .route("/openapi.json", get(openapi::openapi_endpoint))
        .route("/api/openapi.json", get(openapi::openapi_endpoint))
        .route("/api/docs", get(openapi::swagger_ui_endpoint))
        .route("/api/public/entries/{token}", get(items::get_public_entry))
        .nest_service("/assets", ServeDir::new(&upload_path))
        .merge(auth::public_routes())
        .merge(auth::session_routes(state.clone()))
        .merge(protected_routes)
        .merge(mcp_router)
        .fallback_service(ServeDir::new("static-frontend/dist"))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
