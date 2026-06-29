use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::error::ApiError;
use super::state::AppState;
use super::store_search::DeleteResponse;

#[derive(Serialize)]
pub(crate) struct AcpInstancesResponse {
    instances: Vec<crate::acp_discovery::AcpInstance>,
    active: Option<String>,
    /// Per-instance WS worker status, sorted by `instance_id`. Mirrors the
    /// `instances` list but adds runtime fields (`connected`, `session_count`,
    /// `pending_permissions`) so the frontend can pick a default and surface
    /// liveness without polling each worker individually.
    workers: Vec<crate::acp_ws::InstanceStatus>,
}

pub(crate) async fn list_acp_instances(
    State(state): State<AppState>,
) -> Result<Json<AcpInstancesResponse>, ApiError> {
    let instances = if let Some(disc) = state.acp_discovery.as_ref() {
        disc.list().await
    } else {
        Vec::new()
    };
    let active = match state.acp_discovery.as_ref() {
        Some(disc) => disc.active().await.map(|i| i.name),
        None => None,
    };
    let workers = if let Some(reg) = state.acp_ws.as_ref() {
        reg.statuses().await
    } else {
        Vec::new()
    };
    Ok(Json(AcpInstancesResponse {
        instances,
        active,
        workers,
    }))
}

#[derive(Deserialize)]
pub(crate) struct SelectAcpInstanceRequest {
    name: String,
}

pub(crate) async fn select_acp_instance(
    State(state): State<AppState>,
    Json(req): Json<SelectAcpInstanceRequest>,
) -> Result<Json<crate::acp_discovery::AcpInstance>, ApiError> {
    let disc = state
        .acp_discovery
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("acp discovery not enabled".to_owned()))?;
    let inst = disc
        .select(&req.name)
        .await
        .ok_or_else(|| ApiError::BadRequest(format!("unknown acp instance: {}", req.name)))?;
    Ok(Json(inst))
}

#[derive(Debug, Deserialize)]
pub(crate) struct RegisterAcpInstanceRequest {
    name: String,
    host: String,
    port: u16,
    /// Optional fully-qualified URL. When omitted, server builds
    /// `ws://host:port/`. Use this to register a `wss://...` endpoint or
    /// any non-default path.
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    txt: Option<HashMap<String, String>>,
}

pub(crate) async fn register_acp_instance(
    State(state): State<AppState>,
    Json(req): Json<RegisterAcpInstanceRequest>,
) -> Result<Json<crate::acp_discovery::AcpInstance>, ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name cannot be empty".to_owned()));
    }
    if req.host.trim().is_empty() {
        return Err(ApiError::BadRequest("host cannot be empty".to_owned()));
    }
    let disc = state
        .acp_discovery
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("acp discovery not enabled".to_owned()))?;
    let instance = crate::acp_discovery::AcpInstance {
        name: req.name,
        host: req.host,
        port: req.port,
        url: req.url.unwrap_or_default(),
        txt: req.txt.unwrap_or_default(),
        source: crate::acp_discovery::AcpInstanceSource::Registered,
    };
    let stored = disc.register(instance).await;
    Ok(Json(stored))
}

#[derive(Debug, Deserialize)]
pub(crate) struct HeartbeatAcpInstanceRequest {
    name: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct HeartbeatAcpResponse {
    refreshed: bool,
}

pub(crate) async fn heartbeat_acp_instance(
    State(state): State<AppState>,
    Json(req): Json<HeartbeatAcpInstanceRequest>,
) -> Result<Json<HeartbeatAcpResponse>, ApiError> {
    let disc = state
        .acp_discovery
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("acp discovery not enabled".to_owned()))?;
    let refreshed = disc.heartbeat(&req.name).await;
    Ok(Json(HeartbeatAcpResponse { refreshed }))
}

pub(crate) async fn unregister_acp_instance(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<DeleteResponse>, ApiError> {
    let disc = state
        .acp_discovery
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("acp discovery not enabled".to_owned()))?;
    let removed = disc.unregister(&name).await;
    Ok(Json(DeleteResponse {
        id: name,
        deleted: removed,
    }))
}

/// Browser-facing WebSocket proxy. Bridges a same-origin `wss://…/api/acp/ws`
/// upgrade onto a per-instance AcpWsHandle so multiple browser tabs can each
/// subscribe to the daemon of their choice. `?instance=<id>` selects the
/// target; omit when only one instance is registered. Auth inherited from
/// `require_api_key`.
pub(crate) async fn acp_ws_proxy(
    ws: axum::extract::WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, ApiError> {
    let registry = state
        .acp_ws
        .clone()
        .ok_or_else(|| ApiError::BadRequest("acp_ws not enabled".to_owned()))?;
    let requested = params.get("instance").map(String::as_str);
    let handle = match registry.resolve(requested).await {
        Some(h) => h,
        None => {
            let n = registry.len().await;
            let msg = if n == 0 {
                "no ACP instances registered".to_owned()
            } else if requested.is_some() {
                format!("unknown ACP instance '{}'", requested.unwrap_or("?"))
            } else {
                format!("multiple ACP instances registered ({n}); specify ?instance=")
            };
            return Err(ApiError::BadRequest(msg));
        }
    };
    Ok(ws.on_upgrade(move |socket| acp_ws_proxy_task(socket, handle)))
}

async fn acp_ws_proxy_task(
    socket: axum::extract::ws::WebSocket,
    handle: std::sync::Arc<crate::acp_ws::AcpWsHandle>,
) {
    use axum::extract::ws::Message as AxumMessage;
    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::broadcast::error::RecvError;

    let (mut sink, mut stream) = socket.split();
    let mut rx = handle.subscribe();

    // Kick the upstream to emit a fresh snapshot — closes the late-joiner
    // race where SessionStarted events landed after the cached snapshot.
    let _ = handle.request_list_sessions();

    if let Some(snap) = handle.subscriber_snapshot().await {
        if sink.send(AxumMessage::Text(snap.into())).await.is_err() {
            return;
        }
    }

    let mut ping_tick = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(text) => {
                    if sink.send(AxumMessage::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "acp_ws_proxy: subscriber lagged");
                    continue;
                }
                Err(RecvError::Closed) => break,
            },
            msg = stream.next() => match msg {
                Some(Ok(AxumMessage::Text(text))) => {
                    let value = match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(v) => v,
                        Err(err) => {
                            tracing::debug!(error = %err, "acp_ws_proxy: client sent non-JSON");
                            continue;
                        }
                    };
                    if let Err(err) = handle.send_raw(value) {
                        tracing::warn!(error = %err, "acp_ws_proxy: forward failed");
                        break;
                    }
                }
                Some(Ok(AxumMessage::Binary(_))) => {
                    tracing::debug!("acp_ws_proxy: dropping binary frame from client");
                }
                Some(Ok(AxumMessage::Ping(p))) => {
                    if sink.send(AxumMessage::Pong(p)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(AxumMessage::Pong(_))) => {}
                Some(Ok(AxumMessage::Close(_))) | None => break,
                Some(Err(err)) => {
                    tracing::debug!(error = %err, "acp_ws_proxy: client read error");
                    break;
                }
            },
            _ = ping_tick.tick() => {
                if sink.send(AxumMessage::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
    }
}
