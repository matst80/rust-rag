//! WebSocket client for Telegram-ACP.
//!
//! Long-lived connection. Pushes incoming events into a per-session ring buffer
//! and tracks pending PermissionRequests. Manager tools call `command` to send
//! WS commands and the read methods to inspect recent state.
//!
//! Wire surface (frozen as Telegram-ACP WS protocol v1.3.0). See RAG entry
//! `telegram_acp_ws_protocol_v1` for the canonical doc.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex, Notify};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::AcpWsConfig;

fn is_snapshot(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("Snapshot")
        || kind.eq_ignore_ascii_case("snapshot")
        || kind == "state_snapshot"
        || kind == "commands_snapshot"
}
fn is_permission_request(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("PermissionRequest") || kind == "permission_request"
}
fn is_session_ended(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("SessionEnded") || kind == "session_ended"
}

/// One event captured from the WS stream.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AcpEvent {
    /// Local monotonic id assigned on receive (until telegram-acp ships
    /// server-side `event_id` per #7).
    pub local_seq: u64,
    /// Server-supplied event id when present (forward-compat with #7).
    pub event_id: Option<u64>,
    /// Variant tag from the WS payload (top-level enum key).
    pub kind: String,
    /// `acp_session_id` if present in the payload, otherwise `None`.
    pub session_id: Option<String>,
    /// Raw payload as received.
    pub payload: Value,
    /// Receive time (ms since epoch).
    pub received_at: i64,
}

#[derive(Debug, Default)]
struct SessionBuffer {
    events: VecDeque<AcpEvent>,
}

#[derive(Debug, Default)]
struct InnerState {
    next_seq: u64,
    /// Ring buffer per session id. Events without a session id go in the empty-string bucket.
    buffers: HashMap<String, SessionBuffer>,
    /// Latest Snapshot payload, if any.
    latest_snapshot: Option<AcpEvent>,
    /// Outstanding PermissionRequest events keyed by request_id.
    pending_permissions: HashMap<String, AcpEvent>,
    /// Connection status for diagnostics.
    connected: bool,
    last_error: Option<String>,
}

#[derive(Clone)]
pub struct AcpWsHandle {
    inner: Arc<Mutex<InnerState>>,
    outbound: mpsc::UnboundedSender<Value>,
    cap_per_session: usize,
    /// Wakes anyone watching for new events (optional).
    pub event_notify: Arc<Notify>,
    /// Target (url, optional token). Updating + signalling `target_changed`
    /// causes the run loop to drop its current connection and reconnect.
    target: Arc<Mutex<(String, Option<String>)>>,
    target_changed: Arc<Notify>,
}

impl AcpWsHandle {
    /// Swap the active WS target. Any existing connection is dropped and a
    /// new one opens against `url` with `token`.
    pub async fn set_target(&self, url: String, token: Option<String>) {
        {
            let mut g = self.target.lock().await;
            if g.0 == url && g.1 == token {
                return;
            }
            *g = (url, token);
        }
        self.target_changed.notify_waiters();
    }

    pub async fn current_target(&self) -> (String, Option<String>) {
        self.target.lock().await.clone()
    }
}

impl AcpWsHandle {
    /// Send a raw command JSON to the server. Caller is responsible for shape.
    pub fn send_raw(&self, value: Value) -> Result<()> {
        self.outbound
            .send(value)
            .map_err(|err| anyhow!("acp_ws outbound channel closed: {err}"))
    }

    /// Build the canonical lowercase-tagged envelope: `{ "type": "<variant>", ...payload }`.
    /// `variant` should be `snake_case`.
    pub fn command(&self, variant: &str, payload: Value) -> Result<()> {
        let mut map = match payload {
            Value::Object(m) => m,
            Value::Null => serde_json::Map::new(),
            other => {
                let mut m = serde_json::Map::new();
                m.insert("payload".to_string(), other);
                m
            }
        };
        map.insert("type".to_string(), Value::String(variant.to_string()));
        self.send_raw(Value::Object(map))
    }

    pub async fn status(&self) -> Value {
        let g = self.inner.lock().await;
        json!({
            "connected": g.connected,
            "last_error": g.last_error,
            "sessions_buffered": g.buffers.len(),
            "pending_permissions": g.pending_permissions.len(),
            "next_seq": g.next_seq,
        })
    }

    pub async fn recent_events(
        &self,
        session_id: Option<&str>,
        since_local_seq: Option<u64>,
        kinds: Option<&[String]>,
        limit: Option<usize>,
    ) -> Vec<AcpEvent> {
        let g = self.inner.lock().await;
        let limit = limit.unwrap_or(50).min(500);
        let mut out: Vec<AcpEvent> = match session_id {
            Some(sid) => g
                .buffers
                .get(sid)
                .map(|b| b.events.iter().cloned().collect())
                .unwrap_or_default(),
            None => g
                .buffers
                .values()
                .flat_map(|b| b.events.iter().cloned())
                .collect(),
        };
        out.retain(|ev| {
            since_local_seq
                .map(|s| ev.local_seq > s)
                .unwrap_or(true)
                && kinds
                    .map(|ks| ks.iter().any(|k| k == &ev.kind))
                    .unwrap_or(true)
        });
        out.sort_by_key(|ev| ev.local_seq);
        if out.len() > limit {
            let drop_n = out.len() - limit;
            out.drain(0..drop_n);
        }
        out
    }

    pub async fn pending_permissions(&self) -> Vec<AcpEvent> {
        let g = self.inner.lock().await;
        g.pending_permissions.values().cloned().collect()
    }

    pub async fn latest_snapshot(&self) -> Option<AcpEvent> {
        let g = self.inner.lock().await;
        g.latest_snapshot.clone()
    }
}

/// Spawn the long-lived WS client task. Returns the handle even when no URL
/// is configured up-front; callers can later point it at a discovered target
/// via `set_target`. Returns `None` only when ring buffer config is invalid.
pub fn spawn(cfg: AcpWsConfig) -> Option<AcpWsHandle> {
    let url = cfg.url.clone();
    let token = cfg.token.clone();
    let cap = cfg.ring_buffer_per_session.max(20);
    let initial = cfg.reconnect_initial_secs.max(1);
    let max = cfg.reconnect_max_secs.max(initial);

    let inner = Arc::new(Mutex::new(InnerState::default()));
    let (tx, rx) = mpsc::unbounded_channel::<Value>();
    let notify = Arc::new(Notify::new());
    let target = Arc::new(Mutex::new((url.unwrap_or_default(), token)));
    let target_changed = Arc::new(Notify::new());

    let handle = AcpWsHandle {
        inner: inner.clone(),
        outbound: tx,
        cap_per_session: cap,
        event_notify: notify.clone(),
        target: target.clone(),
        target_changed: target_changed.clone(),
    };

    tokio::spawn(run_loop(
        target,
        target_changed,
        cap,
        initial,
        max,
        inner,
        rx,
        notify,
    ));
    Some(handle)
}

async fn run_loop(
    target: Arc<Mutex<(String, Option<String>)>>,
    target_changed: Arc<Notify>,
    cap_per_session: usize,
    initial_backoff: u64,
    max_backoff: u64,
    inner: Arc<Mutex<InnerState>>,
    mut outbound_rx: mpsc::UnboundedReceiver<Value>,
    notify: Arc<Notify>,
) {
    let mut backoff = initial_backoff;

    loop {
        let (url, token) = target.lock().await.clone();
        if url.is_empty() {
            // No target yet; park until set_target signals.
            target_changed.notified().await;
            backoff = initial_backoff;
            continue;
        }
        info!("acp_ws: connecting target={url}");

        match connect(&url, token.as_deref()).await {
            Ok(ws) => {
                {
                    let mut g = inner.lock().await;
                    g.connected = true;
                    g.last_error = None;
                }
                backoff = initial_backoff;
                info!("acp_ws: connected");

                let (mut sink, mut stream) = ws.split();

                loop {
                    tokio::select! {
                        _ = target_changed.notified() => {
                            info!("acp_ws: target changed; dropping current connection");
                            break;
                        }
                        Some(value) = outbound_rx.recv() => {
                            let text = match serde_json::to_string(&value) {
                                Ok(t) => t,
                                Err(err) => {
                                    warn!("acp_ws: failed to serialize outbound: {err}");
                                    continue;
                                }
                            };
                            if let Err(err) = sink.send(Message::Text(text)).await {
                                warn!("acp_ws: send error, reconnecting: {err}");
                                break;
                            }
                        }
                        Some(msg) = stream.next() => {
                            match msg {
                                Ok(Message::Text(text)) => {
                                    handle_incoming(&inner, cap_per_session, &text).await;
                                    notify.notify_waiters();
                                }
                                Ok(Message::Binary(_)) => {
                                    debug!("acp_ws: ignoring binary frame");
                                }
                                Ok(Message::Ping(payload)) => {
                                    let _ = sink.send(Message::Pong(payload)).await;
                                }
                                Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
                                Ok(Message::Close(reason)) => {
                                    info!("acp_ws: server closed: {reason:?}");
                                    break;
                                }
                                Err(err) => {
                                    warn!("acp_ws: read error: {err}");
                                    break;
                                }
                            }
                        }
                        else => break,
                    }
                }
            }
            Err(err) => {
                let mut g = inner.lock().await;
                g.connected = false;
                g.last_error = Some(err.to_string());
                drop(g);
                warn!("acp_ws: connect failed, retry in {backoff}s: {err}");
            }
        }

        {
            let mut g = inner.lock().await;
            g.connected = false;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {}
            _ = target_changed.notified() => {
                backoff = initial_backoff;
                continue;
            }
        }
        backoff = (backoff * 2).min(max_backoff);
    }
}

async fn connect(
    url: &str,
    token: Option<&str>,
) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>
{
    let mut req = url.into_client_request()?;
    if let Some(t) = token {
        let value = format!("Bearer {t}");
        req.headers_mut()
            .insert(AUTHORIZATION, value.parse().map_err(|e| anyhow!("bad token header: {e}"))?);
    }
    let (ws, _resp) = tokio_tungstenite::connect_async(req).await?;
    Ok(ws)
}

async fn handle_incoming(inner: &Arc<Mutex<InnerState>>, cap: usize, text: &str) {
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(err) => {
            debug!("acp_ws: ignoring non-JSON message: {err}");
            return;
        }
    };

    let (kind, payload) = match extract_envelope(&value) {
        Some(t) => t,
        None => {
            debug!("acp_ws: ignoring unrecognized envelope");
            return;
        }
    };

    let session_id = payload
        .get("acp_session_id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("session_id").and_then(Value::as_str))
        .map(str::to_owned);

    let event_id = payload.get("event_id").and_then(Value::as_u64);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let mut g = inner.lock().await;
    g.next_seq += 1;
    let seq = g.next_seq;

    let event = AcpEvent {
        local_seq: seq,
        event_id,
        kind: kind.clone(),
        session_id: session_id.clone(),
        payload: payload.clone(),
        received_at: now_ms,
    };

    if is_snapshot(&kind) {
        g.latest_snapshot = Some(event.clone());
    }

    if is_permission_request(&kind) {
        if let Some(req_id) = payload.get("request_id").and_then(Value::as_str) {
            g.pending_permissions
                .insert(req_id.to_owned(), event.clone());
        }
    }

    // Permission resolution: a SessionEnded for a session clears its pending perms.
    if is_session_ended(&kind) {
        if let Some(sid) = &session_id {
            g.pending_permissions
                .retain(|_, ev| ev.session_id.as_deref() != Some(sid.as_str()));
        }
    }

    let bucket = session_id.unwrap_or_default();
    let buf = g.buffers.entry(bucket).or_default();
    buf.events.push_back(event);
    while buf.events.len() > cap {
        buf.events.pop_front();
    }
}

/// Resolves both `{ "PermissionRequest": { ... } }` shape and tagged `{ "kind": "...", ... }` shape.
fn extract_envelope(value: &Value) -> Option<(String, Value)> {
    if let Value::Object(map) = value {
        if map.len() == 1 {
            let (k, v) = map.iter().next().unwrap();
            if v.is_object() {
                return Some((k.clone(), v.clone()));
            }
        }
        if let Some(kind) = map.get("kind").and_then(Value::as_str) {
            return Some((kind.to_owned(), value.clone()));
        }
        if let Some(kind) = map.get("type").and_then(Value::as_str) {
            return Some((kind.to_owned(), value.clone()));
        }
    }
    None
}

/// Mark a pending permission as resolved. Manager calls this from the
/// `acp_permission_respond` tool wrapper after sending the WS response.
pub async fn mark_permission_resolved(handle: &AcpWsHandle, request_id: &str) {
    let mut g = handle.inner.lock().await;
    g.pending_permissions.remove(request_id);
}

/// Ensure cap_per_session is reachable from outside if needed in future.
#[allow(dead_code)]
impl AcpWsHandle {
    pub fn cap_per_session(&self) -> usize {
        self.cap_per_session
    }
}
