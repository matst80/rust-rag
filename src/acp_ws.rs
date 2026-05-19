//! ACP WebSocket fan-in / fan-out.
//!
//! One [`AcpWsHandle`] per upstream ACP daemon. Each handle owns a single
//! long-lived WS connection, a per-session ring buffer, the last snapshot,
//! pending permissions, and a `broadcast::Sender` that fans the daemon's
//! frames out to every subscribed browser proxy.
//!
//! [`AcpWsRegistry`] holds the active set of handles keyed by instance id.
//! Discovery (mDNS or HTTP register) drives `register` / `unregister` so
//! every reachable daemon gets its own worker without any single-target
//! swap. Browser sessions specify which instance to subscribe to (or the
//! sole one when only one is registered).
//!
//! Wire surface frozen as Telegram-ACP WS protocol v1.3.0. See RAG entry
//! `telegram_acp_ws_protocol_v1` for canonical doc.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify, RwLock, broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tracing::{debug, info, warn};

pub use crate::config::AcpWsConfig;

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
fn is_session_started(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("SessionStarted") || kind == "session_started"
}
fn is_session_renamed(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("SessionRenamed") || kind == "session_renamed"
}
fn is_topic_removed(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("TopicRemoved") || kind == "topic_removed"
}

/// One event captured from the WS stream.
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
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
    /// Last snapshot decoded into struct form (for MCP `acp_get_snapshot`).
    latest_snapshot: Option<AcpEvent>,
    /// Verbatim text of the most recent snapshot frame, replayed verbatim
    /// to new subscribers when present.
    latest_snapshot_text: Option<String>,
    /// Live session id → most recently observed SessionInfo (from Snapshot
    /// or SessionStarted payloads). Late-joining browsers get a synthesized
    /// snapshot built from this map when the daemon hasn't recently emitted
    /// one — closes the "new browser sees 0 sessions even though daemon has
    /// N" window.
    live_sessions: HashMap<String, Value>,
    /// Projects list from the last Snapshot. Carried alongside `live_sessions`
    /// so synthesized snapshots look identical to daemon-emitted ones.
    live_projects: Vec<Value>,
    /// Outstanding PermissionRequest events keyed by request_id.
    pending_permissions: HashMap<String, AcpEvent>,
    /// Connection status for diagnostics.
    connected: bool,
    last_error: Option<String>,
}

/// Public status row for one instance.
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct InstanceStatus {
    pub instance_id: String,
    pub url: String,
    pub connected: bool,
    pub last_error: Option<String>,
    pub session_count: usize,
    pub pending_permissions: usize,
    pub buffered_events: usize,
}

/// Per-instance ACP WS worker. Cloning is cheap; clones share inner state.
#[derive(Clone)]
pub struct AcpWsHandle {
    instance_id: String,
    url: String,
    inner: Arc<Mutex<InnerState>>,
    outbound: mpsc::UnboundedSender<Value>,
    cap_per_session: usize,
    /// Wakes anyone watching for new events (MCP `wait_for_event`).
    pub event_notify: Arc<Notify>,
    /// Fan-out of every Text frame received from the daemon. Browser-side
    /// proxies subscribe here so a single daemon connection feeds many tabs.
    events_tx: broadcast::Sender<String>,
    /// Notify the run loop to drop its WS and exit when the registry removes
    /// this instance.
    shutdown: Arc<Notify>,
}

impl AcpWsHandle {
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Send a raw command JSON to the server. Caller is responsible for shape.
    pub fn send_raw(&self, value: Value) -> Result<()> {
        self.outbound
            .send(value)
            .map_err(|err| anyhow!("acp_ws outbound channel closed: {err}"))
    }

    /// Build the canonical lowercase-tagged envelope: `{ "type": "<variant>", ...payload }`.
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

    /// Ask the daemon for a fresh ListSessions snapshot. Best-effort.
    pub fn request_list_sessions(&self) -> Result<()> {
        self.command("list_sessions", json!({}))
    }

    pub async fn status(&self) -> InstanceStatus {
        let g = self.inner.lock().await;
        let buffered_events: usize = g.buffers.values().map(|b| b.events.len()).sum();
        InstanceStatus {
            instance_id: self.instance_id.clone(),
            url: self.url.clone(),
            connected: g.connected,
            last_error: g.last_error.clone(),
            session_count: g.live_sessions.len(),
            pending_permissions: g.pending_permissions.len(),
            buffered_events,
        }
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
            since_local_seq.map(|s| ev.local_seq > s).unwrap_or(true)
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

    /// Snapshot frame to forward to a freshly-connected subscriber.
    ///
    /// Prefers a frame synthesized from the live session/project maps over
    /// the daemon's last raw snapshot. The daemon only emits Snapshot at
    /// connect + on explicit `list_sessions`, so the raw cache can lag
    /// behind in-flight `SessionStarted`/`SessionEnded` deltas. The
    /// synthesized form merges those deltas in and stays current.
    pub async fn subscriber_snapshot(&self) -> Option<String> {
        let g = self.inner.lock().await;
        if !g.live_sessions.is_empty() || !g.live_projects.is_empty() {
            let sessions: Vec<Value> = g.live_sessions.values().cloned().collect();
            let payload = json!({
                "type": "state_snapshot",
                "sessions": sessions,
                "projects": g.live_projects,
            });
            return Some(payload.to_string());
        }
        g.latest_snapshot_text.clone()
    }

    /// Subscribe to the raw daemon-frame fanout. Each receiver gets every
    /// Text frame in arrival order; lagging consumers see `RecvError::Lagged`
    /// and should keep recv-ing.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.events_tx.subscribe()
    }

    pub async fn mark_permission_resolved(&self, request_id: &str) {
        let mut g = self.inner.lock().await;
        g.pending_permissions.remove(request_id);
    }

    /// Stop the worker. Closes the upstream WS; subscribers see broadcast
    /// receiver closed. Idempotent.
    pub fn shutdown(&self) {
        self.shutdown.notify_one();
    }
}

/// Registry of per-instance workers. One worker = one upstream WS.
pub struct AcpWsRegistry {
    workers: RwLock<HashMap<String, Arc<AcpWsHandle>>>,
    cap_per_session: usize,
    initial_backoff: u64,
    max_backoff: u64,
}

impl AcpWsRegistry {
    pub fn new(cfg: &AcpWsConfig) -> Arc<Self> {
        let initial = cfg.reconnect_initial_secs.max(1);
        let max = cfg.reconnect_max_secs.max(initial);
        Arc::new(Self {
            workers: RwLock::new(HashMap::new()),
            cap_per_session: cfg.ring_buffer_per_session.max(20),
            initial_backoff: initial,
            max_backoff: max,
        })
    }

    /// Register (or replace) an instance. Spawns a worker that connects to
    /// `url`. If `id` already exists, the old worker is shut down and a new
    /// one is created — supports re-resolution after a host change.
    pub async fn register(
        self: &Arc<Self>,
        id: String,
        url: String,
        token: Option<String>,
    ) -> Arc<AcpWsHandle> {
        let mut g = self.workers.write().await;
        if let Some(old) = g.remove(&id) {
            if old.url == url {
                let old_status = old.status().await;
                if old_status.connected {
                    // No change and still connected — put it back.
                    g.insert(id.clone(), old.clone());
                    return old;
                }
                info!(instance_id = %id, "acp_registry: old worker disconnected, replacing to force reconnect");
            } else {
                info!(instance_id = %id, "acp_registry: re-registering, dropping old worker");
            }
            old.shutdown();
        }
        let worker = spawn_worker(
            id.clone(),
            url,
            token,
            self.cap_per_session,
            self.initial_backoff,
            self.max_backoff,
        );
        g.insert(id, worker.clone());
        worker
    }

    pub async fn unregister(&self, id: &str) -> bool {
        let mut g = self.workers.write().await;
        if let Some(worker) = g.remove(id) {
            info!(instance_id = %id, "acp_registry: unregistering worker");
            worker.shutdown();
            true
        } else {
            false
        }
    }

    pub async fn get(&self, id: &str) -> Option<Arc<AcpWsHandle>> {
        let g = self.workers.read().await;
        g.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Arc<AcpWsHandle>> {
        let g = self.workers.read().await;
        g.values().cloned().collect()
    }

    pub async fn statuses(&self) -> Vec<InstanceStatus> {
        let workers = self.list().await;
        let mut out = Vec::with_capacity(workers.len());
        for w in workers {
            out.push(w.status().await);
        }
        out.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        out
    }

    /// Resolve a worker by optional explicit id. If `id` is `None`:
    /// - exactly one worker registered → return it
    /// - zero or more than one → return None (caller must error)
    pub async fn resolve(&self, id: Option<&str>) -> Option<Arc<AcpWsHandle>> {
        if let Some(id) = id {
            return self.get(id).await;
        }
        let g = self.workers.read().await;
        if g.len() == 1 {
            return g.values().next().cloned();
        }
        None
    }

    /// Total worker count. Lets callers distinguish "no instance registered"
    /// from "ambiguous instance" when `resolve(None)` returns `None`.
    pub async fn len(&self) -> usize {
        self.workers.read().await.len()
    }
}

fn spawn_worker(
    instance_id: String,
    url: String,
    token: Option<String>,
    cap_per_session: usize,
    initial_backoff: u64,
    max_backoff: u64,
) -> Arc<AcpWsHandle> {
    let inner = Arc::new(Mutex::new(InnerState::default()));
    let (tx, rx) = mpsc::unbounded_channel::<Value>();
    let notify = Arc::new(Notify::new());
    let (events_tx, _) = broadcast::channel::<String>(256);
    let shutdown = Arc::new(Notify::new());

    let handle = Arc::new(AcpWsHandle {
        instance_id: instance_id.clone(),
        url: url.clone(),
        inner: inner.clone(),
        outbound: tx,
        cap_per_session,
        event_notify: notify.clone(),
        events_tx: events_tx.clone(),
        shutdown: shutdown.clone(),
    });

    tokio::spawn(run_loop(
        instance_id,
        url,
        token,
        cap_per_session,
        initial_backoff,
        max_backoff,
        inner,
        rx,
        notify,
        events_tx,
        shutdown,
    ));
    handle
}

async fn run_loop(
    instance_id: String,
    url: String,
    token: Option<String>,
    cap_per_session: usize,
    initial_backoff: u64,
    max_backoff: u64,
    inner: Arc<Mutex<InnerState>>,
    mut outbound_rx: mpsc::UnboundedReceiver<Value>,
    notify: Arc<Notify>,
    events_tx: broadcast::Sender<String>,
    shutdown: Arc<Notify>,
) {
    let mut backoff = initial_backoff;

    loop {
        if url.is_empty() {
            info!(instance_id = %instance_id, "acp_ws: empty url, exiting worker");
            return;
        }
        info!(instance_id = %instance_id, url = %url, "acp_ws: connecting");

        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                info!(instance_id = %instance_id, "acp_ws: shutdown before connect");
                return;
            }
            connect_res = connect(&url, token.as_deref()) => {
                match connect_res {
                    Ok(ws) => {
                        {
                            let mut g = inner.lock().await;
                            g.connected = true;
                            g.last_error = None;
                        }
                        backoff = initial_backoff;
                        info!(instance_id = %instance_id, "acp_ws: connected");

                        let (mut sink, mut stream) = ws.split();

                        // Ask the daemon to emit a snapshot so the in-process
                        // state catches up before any subscribers attach.
                        let frame = json!({ "type": "list_sessions" }).to_string();
                        if let Err(err) = sink.send(Message::Text(frame.into())).await {
                            warn!(instance_id = %instance_id, "acp_ws: failed to request list_sessions on connect: {err}");
                        } else {
                            debug!(instance_id = %instance_id, "acp_ws: requested list_sessions on connect");
                        }

                        loop {
                            tokio::select! {
                                _ = shutdown.notified() => {
                                    info!(instance_id = %instance_id, "acp_ws: shutdown signalled; closing");
                                    let _ = sink.send(Message::Close(None)).await;
                                    return;
                                }
                                Some(value) = outbound_rx.recv() => {
                                    let text = match serde_json::to_string(&value) {
                                        Ok(t) => t,
                                        Err(err) => {
                                            warn!(instance_id = %instance_id, "acp_ws: failed to serialize outbound: {err}");
                                            continue;
                                        }
                                    };
                                    if let Err(err) = sink.send(Message::Text(text.into())).await {
                                        warn!(instance_id = %instance_id, "acp_ws: send error, reconnecting: {err}");
                                        break;
                                    }
                                }
                                Some(msg) = stream.next() => {
                                    match msg {
                                        Ok(Message::Text(text)) => {
                                            let _ = events_tx.send(text.to_string());
                                            handle_incoming(&inner, cap_per_session, &text).await;
                                            notify.notify_waiters();
                                        }
                                        Ok(Message::Binary(_)) => {
                                            debug!(instance_id = %instance_id, "acp_ws: ignoring binary frame");
                                        }
                                        Ok(Message::Ping(payload)) => {
                                            let _ = sink.send(Message::Pong(payload)).await;
                                        }
                                        Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
                                        Ok(Message::Close(reason)) => {
                                            info!(instance_id = %instance_id, "acp_ws: server closed: {reason:?}");
                                            break;
                                        }
                                        Err(err) => {
                                            warn!(instance_id = %instance_id, "acp_ws: read error: {err}");
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
                        warn!(instance_id = %instance_id, "acp_ws: connect failed, retry in {backoff}s: {err}");
                    }
                }
            }
        }

        {
            let mut g = inner.lock().await;
            g.connected = false;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {}
            _ = shutdown.notified() => {
                info!(instance_id = %instance_id, "acp_ws: shutdown during backoff");
                return;
            }
        }
        backoff = (backoff * 2).min(max_backoff);
    }
}

async fn connect(
    url: &str,
    token: Option<&str>,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let mut req = url.into_client_request()?;
    if let Some(t) = token {
        let value = format!("Bearer {t}");
        req.headers_mut().insert(
            AUTHORIZATION,
            value
                .parse()
                .map_err(|e| anyhow!("bad token header: {e}"))?,
        );
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
        g.latest_snapshot_text = Some(text.to_string());
        // Rebuild live session/project maps from the authoritative snapshot.
        if let Some(arr) = payload.get("sessions").and_then(Value::as_array) {
            g.live_sessions.clear();
            for s in arr {
                let sid = s
                    .get("acp_session_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(sid) = sid {
                    g.live_sessions.insert(sid.clone(), s.clone());

                    // Populate ring buffer from snapshot history
                    if let Some(hist) = s.get("history").and_then(Value::as_array) {
                        let mut synthesized = Vec::with_capacity(hist.len());
                        for h in hist {
                            g.next_seq += 1;
                            let seq = g.next_seq;
                            let h_kind = h.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string();
                            synthesized.push(AcpEvent {
                                local_seq: seq,
                                event_id: h.get("event_id").and_then(Value::as_u64),
                                kind: h_kind,
                                session_id: Some(sid.clone()),
                                payload: h.clone(),
                                received_at: now_ms,
                            });
                        }

                        let buf = g.buffers.entry(sid.clone()).or_default();
                        buf.events.clear();
                        for ev in synthesized {
                            buf.events.push_back(ev);
                        }
                        while buf.events.len() > cap {
                            buf.events.pop_front();
                        }
                    }
                }
            }
        }
        if let Some(arr) = payload.get("projects").and_then(Value::as_array) {
            g.live_projects = arr.clone();
        }
    }

    if (is_session_started(&kind) || is_session_renamed(&kind))
        && let Some(sid) = &session_id
    {
        // Capture as much SessionInfo as the event carries. Daemon emits a
        // partial — frontend treats missing fields as defaults. If a fuller
        // Snapshot arrives later it overwrites this entry.
        g.live_sessions
            .entry(sid.clone())
            .and_modify(|v| {
                if let Value::Object(existing) = v
                    && let Value::Object(incoming) = &payload
                {
                    for (k, val) in incoming {
                        existing.insert(k.clone(), val.clone());
                    }
                }
            })
            .or_insert_with(|| payload.clone());
    }

    if is_permission_request(&kind)
        && let Some(req_id) = payload.get("request_id").and_then(Value::as_str)
    {
        g.pending_permissions
            .insert(req_id.to_owned(), event.clone());
    }

    if is_session_ended(&kind) {
        if let Some(sid) = &session_id {
            g.live_sessions.remove(sid);
            g.pending_permissions
                .retain(|_, ev| ev.session_id.as_deref() != Some(sid.as_str()));
        }
    }

    if is_topic_removed(&kind) {
        if let Some(sid) = &session_id {
            g.live_sessions.remove(sid);
            g.buffers.remove(sid);
            g.pending_permissions
                .retain(|_, ev| ev.session_id.as_deref() != Some(sid.as_str()));
        } else if let Some(tid) = payload.get("thread_id").and_then(Value::as_i64) {
            let mut to_remove = Vec::new();
            for (sid, info) in &g.live_sessions {
                if info.get("thread_id").and_then(Value::as_i64) == Some(tid) {
                    to_remove.push(sid.clone());
                }
            }
            for sid in to_remove {
                g.live_sessions.remove(&sid);
                g.buffers.remove(&sid);
                g.pending_permissions
                    .retain(|_, ev| ev.session_id.as_deref() != Some(sid.as_str()));
            }
        }
    }

    // Append to live history for subscriber_snapshot replay
    if let Some(sid) = &session_id {
        if let Some(s) = g.live_sessions.get_mut(sid) {
            if let Value::Object(map) = s {
                let history = map.entry("history".to_string()).or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(arr) = history {
                    // Convert our envelope back to the daemon's internal event shape if possible
                    // but for now just push the payload. The daemon's history is a list of SessionEvent.
                    let mut item = payload.clone();
                    if let Value::Object(item_map) = &mut item {
                        item_map.insert("type".to_string(), Value::String(kind.clone()));
                    }
                    arr.push(item);
                    while arr.len() > cap {
                        arr.remove(0);
                    }
                }
            }
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

/// Helper kept for the existing MCP `acp_permission_respond` call site.
pub async fn mark_permission_resolved(handle: &AcpWsHandle, request_id: &str) {
    handle.mark_permission_resolved(request_id).await;
}

#[allow(dead_code)]
impl AcpWsHandle {
    pub fn cap_per_session(&self) -> usize {
        self.cap_per_session
    }
}
