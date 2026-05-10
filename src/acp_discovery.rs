//! mDNS discovery for `_acp-ws._tcp` instances.
//!
//! Browses the LAN for ACP WS endpoints and exposes the live list plus the
//! currently selected instance. The selected URL is what `acp_ws` reconnects
//! to and what the frontend hands to the browser.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};
use tokio::sync::RwLock;
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_acp-ws._tcp.local.";

/// How long an HTTP-registered instance is kept without a heartbeat before
/// being pruned. Long enough to forgive transient client outages, short
/// enough that stale URLs vanish quickly. Tunable via `RAG_ACP_REGISTER_TTL_SECS`.
const DEFAULT_REGISTER_TTL: Duration = Duration::from_secs(120);
/// How often the janitor wakes to prune expired registrations.
const REGISTER_PRUNE_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AcpInstanceSource {
    /// Discovered via mDNS browse on the LAN.
    Mdns,
    /// Registered explicitly over HTTP (typically used when the rust-rag
    /// service runs in k8s and clients live outside the cluster subnet).
    Registered,
}

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct AcpInstance {
    /// Friendly instance name (e.g. `acp-ws-9001`). Unique per source map.
    pub name: String,
    /// Resolved IP / hostname.
    pub host: String,
    pub port: u16,
    /// Connection URL. Defaults to `ws://host:port/` when omitted by the
    /// caller; HTTP registration may pass a fully-qualified `wss://…` URL.
    pub url: String,
    /// Subset of TXT key/value pairs (`version`, `protocol`, `auth`, ...).
    pub txt: HashMap<String, String>,
    /// Where this entry came from. Lets the UI distinguish auto-discovered
    /// instances from ones that registered via the HTTP API.
    pub source: AcpInstanceSource,
}

#[derive(Default)]
struct Inner {
    instances: HashMap<String, AcpInstance>,
    /// `last_seen` for HTTP-registered instances only. mDNS entries are
    /// pruned by `ServiceRemoved` events instead. Absent → not registered
    /// (i.e. mDNS).
    last_seen: HashMap<String, Instant>,
    active: Option<String>,
}

#[derive(Clone)]
pub struct AcpDiscoveryHandle {
    inner: Arc<RwLock<Inner>>,
    on_select: Arc<dyn Fn(&AcpInstance) + Send + Sync>,
}

impl AcpDiscoveryHandle {
    pub async fn list(&self) -> Vec<AcpInstance> {
        let g = self.inner.read().await;
        let mut v: Vec<_> = g.instances.values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub async fn active(&self) -> Option<AcpInstance> {
        let g = self.inner.read().await;
        g.active.as_ref().and_then(|n| g.instances.get(n).cloned())
    }

    /// Mark `name` active and notify subscribers (acp_ws reconnect).
    /// Returns the resolved instance, or `None` if unknown.
    pub async fn select(&self, name: &str) -> Option<AcpInstance> {
        let resolved = {
            let mut g = self.inner.write().await;
            let inst = g.instances.get(name).cloned()?;
            g.active = Some(inst.name.clone());
            inst
        };
        (self.on_select)(&resolved);
        Some(resolved)
    }

    /// Re-select the same instance (used to nudge acp_ws after a fresh
    /// resolution updates the host).
    async fn refresh_active_if_changed(&self, updated: &AcpInstance) {
        let should_notify = {
            let g = self.inner.read().await;
            matches!(&g.active, Some(n) if n == &updated.name)
        };
        if should_notify {
            (self.on_select)(updated);
        }
    }

    /// Register an instance via HTTP. If `name` already exists from mDNS,
    /// it's overwritten (registered entries take precedence — they were
    /// asserted by an explicit caller). Returns the stored instance after
    /// normalization (URL filled in if absent).
    pub async fn register(&self, mut instance: AcpInstance) -> AcpInstance {
        instance.source = AcpInstanceSource::Registered;
        if instance.url.is_empty() {
            instance.url = format!("ws://{}:{}/", instance.host, instance.port);
        }
        let key = instance.name.clone();
        let stored = {
            let mut g = self.inner.write().await;
            g.last_seen.insert(key.clone(), Instant::now());
            // Auto-select the first instance the server learns about so the
            // UI has something to point at; keeps parity with mDNS path.
            if g.active.is_none() {
                g.active = Some(key.clone());
            }
            g.instances.insert(key.clone(), instance.clone());
            instance
        };
        self.refresh_active_if_changed(&stored).await;
        stored
    }

    /// Bump `last_seen` for an existing registered instance. Returns `true`
    /// when the heartbeat hit a known registered entry.
    pub async fn heartbeat(&self, name: &str) -> bool {
        let mut g = self.inner.write().await;
        if !g.last_seen.contains_key(name) {
            return false;
        }
        g.last_seen.insert(name.to_owned(), Instant::now());
        true
    }

    /// Remove an HTTP-registered instance. Returns `true` if removed.
    /// mDNS-discovered instances are not affected — they only leave when
    /// the service goes away on the network.
    pub async fn unregister(&self, name: &str) -> bool {
        let mut g = self.inner.write().await;
        if g.last_seen.remove(name).is_none() {
            return false;
        }
        g.instances.remove(name);
        if matches!(&g.active, Some(n) if n == name) {
            g.active = g.instances.keys().next().cloned();
        }
        true
    }
}

fn register_ttl() -> Duration {
    std::env::var("RAG_ACP_REGISTER_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_REGISTER_TTL)
}

/// Build a discovery handle without mDNS. Useful when the service runs in
/// an environment where multicast can't reach clients (e.g. inside a k8s
/// pod whose subnet is isolated from the user's LAN). Clients register
/// themselves over HTTP via `POST /api/acp/register`.
pub fn spawn_http_only<F>(on_select: F) -> AcpDiscoveryHandle
where
    F: Fn(&AcpInstance) + Send + Sync + 'static,
{
    let inner: Arc<RwLock<Inner>> = Arc::new(RwLock::new(Inner::default()));
    let handle = AcpDiscoveryHandle {
        inner: inner.clone(),
        on_select: Arc::new(on_select),
    };
    spawn_register_janitor(inner, handle.clone());
    info!("acp_discovery: HTTP-only mode (no mDNS)");
    handle
}

/// Background task that prunes HTTP-registered instances whose heartbeat
/// has gone silent. mDNS entries are untouched — they have their own
/// `ServiceRemoved` lifecycle.
fn spawn_register_janitor(inner: Arc<RwLock<Inner>>, handle: AcpDiscoveryHandle) {
    tokio::spawn(async move {
        let ttl = register_ttl();
        loop {
            tokio::time::sleep(REGISTER_PRUNE_INTERVAL).await;
            let now = Instant::now();
            let mut expired = Vec::new();
            {
                let g = inner.read().await;
                for (name, seen) in g.last_seen.iter() {
                    if now.duration_since(*seen) > ttl {
                        expired.push(name.clone());
                    }
                }
            }
            for name in expired {
                if handle.unregister(&name).await {
                    info!("acp_discovery: pruned stale registration {name}");
                }
            }
        }
    });
}

/// Spawn the discovery daemon. `on_select` fires whenever the active
/// instance is set or its address changes. Also starts the HTTP-register
/// janitor so manually-registered instances coexist with mDNS ones.
pub fn spawn<F>(on_select: F) -> Option<AcpDiscoveryHandle>
where
    F: Fn(&AcpInstance) + Send + Sync + 'static,
{
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(err) => {
            warn!("acp_discovery: failed to start mDNS daemon: {err}");
            return None;
        }
    };
    let receiver = match daemon.browse(SERVICE_TYPE) {
        Ok(r) => r,
        Err(err) => {
            warn!("acp_discovery: browse failed: {err}");
            return None;
        }
    };

    let inner: Arc<RwLock<Inner>> = Arc::new(RwLock::new(Inner::default()));
    let handle = AcpDiscoveryHandle {
        inner: inner.clone(),
        on_select: Arc::new(on_select),
    };
    spawn_register_janitor(inner.clone(), handle.clone());

    let bg_handle = handle.clone();
    tokio::spawn(async move {
        info!("acp_discovery: browsing {SERVICE_TYPE}");
        loop {
            // mdns-sd uses a sync channel; bridge to async by polling with
            // try_recv inside a sleep-wait loop.
            match receiver.recv_async().await {
                Ok(event) => match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let name = info.get_fullname().trim_end_matches(".").to_string();
                        let short = info
                            .get_fullname()
                            .strip_suffix(SERVICE_TYPE)
                            .map(|s| s.trim_end_matches('.').to_string())
                            .unwrap_or_else(|| name.clone());
                        let port = info.get_port();
                        let host = info
                            .get_addresses()
                            .iter()
                            .next()
                            .map(|ip| ip.to_string())
                            .unwrap_or_default();
                        let url = format!("ws://{host}:{port}/");
                        let mut txt = HashMap::new();
                        for prop in info.get_properties().iter() {
                            txt.insert(
                                prop.key().to_string(),
                                prop.val_str().to_string(),
                            );
                        }
                        let instance = AcpInstance {
                            name: short,
                            host,
                            port,
                            url,
                            txt,
                            source: AcpInstanceSource::Mdns,
                        };
                        info!(
                            "acp_discovery: resolved {} → {}",
                            instance.name, instance.url
                        );
                        let key = instance.name.clone();
                        {
                            let mut g = inner.write().await;
                            // Auto-select the first instance we see.
                            if g.active.is_none() {
                                g.active = Some(key.clone());
                            }
                            g.instances.insert(key, instance.clone());
                        }
                        bg_handle.refresh_active_if_changed(&instance).await;
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        let short = fullname
                            .strip_suffix(SERVICE_TYPE)
                            .map(|s| s.trim_end_matches('.').to_string())
                            .unwrap_or(fullname);
                        let mut g = inner.write().await;
                        // Don't yank an HTTP-registered entry just because a
                        // (different) mDNS service with the same name went
                        // away. Registered ones outlive mDNS volatility.
                        if !g.last_seen.contains_key(&short) {
                            g.instances.remove(&short);
                            if matches!(&g.active, Some(n) if n == &short) {
                                g.active = g.instances.keys().next().cloned();
                            }
                            info!("acp_discovery: removed {short}");
                        }
                    }
                    _ => {}
                },
                Err(err) => {
                    warn!("acp_discovery: recv error: {err}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    Some(handle)
}
