use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// How long since last poll a user is still considered "active".
pub const PRESENCE_WINDOW_MS: i64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceEntry {
    pub user: String,
    pub kind: String,
    pub last_seen: i64,
}

#[derive(Default)]
pub struct PresenceTracker {
    /// channel -> user -> (kind, last_seen_ms)
    inner: Mutex<HashMap<String, HashMap<String, (String, i64)>>>,
}

impl PresenceTracker {
    pub fn touch(&self, channel: &str, user: &str, kind: &str) {
        let now = current_ms();
        let mut guard = self.inner.lock().expect("presence mutex poisoned");
        let bucket = guard.entry(channel.to_owned()).or_default();
        bucket.insert(user.to_owned(), (kind.to_owned(), now));
    }

    pub fn list(&self, channel: &str) -> Vec<PresenceEntry> {
        let cutoff = current_ms() - PRESENCE_WINDOW_MS;
        let mut guard = self.inner.lock().expect("presence mutex poisoned");
        let bucket = match guard.get_mut(channel) {
            Some(b) => b,
            None => return Vec::new(),
        };
        bucket.retain(|_, (_, last_seen)| *last_seen >= cutoff);
        let mut out: Vec<PresenceEntry> = bucket
            .iter()
            .map(|(user, (kind, last_seen))| PresenceEntry {
                user: user.clone(),
                kind: kind.clone(),
                last_seen: *last_seen,
            })
            .collect();
        out.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        out
    }

    pub fn list_all(&self) -> HashMap<String, Vec<PresenceEntry>> {
        let cutoff = current_ms() - PRESENCE_WINDOW_MS;
        let mut guard = self.inner.lock().expect("presence mutex poisoned");
        let mut out = HashMap::new();
        for (channel, bucket) in guard.iter_mut() {
            bucket.retain(|_, (_, last_seen)| *last_seen >= cutoff);
            if bucket.is_empty() {
                continue;
            }
            let mut entries: Vec<PresenceEntry> = bucket
                .iter()
                .map(|(user, (kind, last_seen))| PresenceEntry {
                    user: user.clone(),
                    kind: kind.clone(),
                    last_seen: *last_seen,
                })
                .collect();
            entries.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
            out.insert(channel.clone(), entries);
        }
        out
    }
}

fn current_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
