//! mDNS discovery for `_acp-ws._tcp` instances.
//!
//! Browses the LAN for ACP WS endpoints and exposes the live list plus the
//! currently selected instance. The selected URL is what `acp_ws` reconnects
//! to and what the frontend hands to the browser.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use tokio::sync::RwLock;
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_acp-ws._tcp.local.";

#[derive(Debug, Clone, serde::Serialize)]
pub struct AcpInstance {
    /// Friendly mDNS instance name (e.g. `acp-ws-9001`).
    pub name: String,
    /// First IPv4/IPv6 address resolved.
    pub host: String,
    pub port: u16,
    /// Computed `ws://host:port/`.
    pub url: String,
    /// Subset of TXT key/value pairs (`version`, `protocol`, `auth`, ...).
    pub txt: HashMap<String, String>,
}

#[derive(Default)]
struct Inner {
    instances: HashMap<String, AcpInstance>,
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
}

/// Spawn the discovery daemon. `on_select` fires whenever the active
/// instance is set or its address changes.
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
                        g.instances.remove(&short);
                        if matches!(&g.active, Some(n) if n == &short) {
                            g.active = g.instances.keys().next().cloned();
                        }
                        info!("acp_discovery: removed {short}");
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
