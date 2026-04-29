use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// Keep tombstones around long enough that any active long-poll picks them up.
/// Past this window the deleted-id falls off; new clients reconcile via
/// initial fetch.
pub const TOMBSTONE_WINDOW_MS: i64 = 5 * 60 * 1000;
const PER_CHANNEL_CAP: usize = 1024;

#[derive(Debug, Clone)]
pub struct Tombstone {
    pub id: String,
    pub deleted_at: i64,
}

#[derive(Default)]
pub struct TombstoneTracker {
    inner: Mutex<HashMap<String, VecDeque<Tombstone>>>,
}

impl TombstoneTracker {
    pub fn record(&self, channel: &str, id: &str) {
        let now = current_ms();
        let mut guard = self.inner.lock().expect("tombstone mutex poisoned");
        let bucket = guard.entry(channel.to_owned()).or_default();
        bucket.push_back(Tombstone {
            id: id.to_owned(),
            deleted_at: now,
        });
        // Drop oldest entries when the bucket exceeds the cap or falls outside
        // the retention window.
        let cutoff = now - TOMBSTONE_WINDOW_MS;
        while let Some(front) = bucket.front() {
            if bucket.len() > PER_CHANNEL_CAP || front.deleted_at < cutoff {
                bucket.pop_front();
            } else {
                break;
            }
        }
    }

    /// Tombstones for `channel` with `deleted_at >= since`. Caller filters
    /// further if it has already-known ids.
    pub fn since(&self, channel: &str, since: i64) -> Vec<Tombstone> {
        let mut guard = self.inner.lock().expect("tombstone mutex poisoned");
        let Some(bucket) = guard.get_mut(channel) else {
            return Vec::new();
        };
        let cutoff = current_ms() - TOMBSTONE_WINDOW_MS;
        bucket.retain(|t| t.deleted_at >= cutoff);
        bucket
            .iter()
            .filter(|t| t.deleted_at >= since)
            .cloned()
            .collect()
    }
}

fn current_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
