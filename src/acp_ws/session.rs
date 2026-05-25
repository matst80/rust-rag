use std::collections::HashMap;
use serde_json::Value;

#[derive(Debug, Default)]
pub struct SessionManager {
    /// Live session id → most recently observed SessionInfo
    pub live_sessions: HashMap<String, Value>,
    /// Projects list from the last Snapshot.
    pub live_projects: Vec<Value>,
}

impl SessionManager {
    pub fn is_session_started(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("SessionStarted") || kind == "session_started"
    }

    pub fn is_session_ended(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("SessionEnded") || kind == "session_ended"
    }

    pub fn is_session_renamed(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("SessionRenamed") || kind == "session_renamed"
    }

    pub fn is_topic_removed(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("TopicRemoved") || kind == "topic_removed"
    }

    pub fn handle_event(&mut self, kind: &str, session_id: Option<&String>, payload: &Value) -> Vec<String> {
        let mut removed_session_ids = Vec::new();
        if (Self::is_session_started(kind) || Self::is_session_renamed(kind))
            && let Some(sid) = session_id
        {
            self.live_sessions
                .entry(sid.clone())
                .and_modify(|v| {
                    if let Value::Object(existing) = v
                        && let Value::Object(incoming) = payload
                    {
                        for (k, val) in incoming {
                            existing.insert(k.clone(), val.clone());
                        }
                    }
                })
                .or_insert_with(|| payload.clone());
        }

        if Self::is_session_ended(kind) {
            if let Some(sid) = session_id {
                self.live_sessions.remove(sid);
                removed_session_ids.push(sid.clone());
            }
        }

        if Self::is_topic_removed(kind) {
            if let Some(sid) = session_id {
                self.live_sessions.remove(sid);
                removed_session_ids.push(sid.clone());
            } else if let Some(tid) = payload.get("thread_id").and_then(Value::as_i64) {
                let mut to_remove = Vec::new();
                for (sid, info) in &self.live_sessions {
                    if info.get("thread_id").and_then(Value::as_i64) == Some(tid) {
                        to_remove.push(sid.clone());
                    }
                }
                for sid in to_remove {
                    self.live_sessions.remove(&sid);
                    removed_session_ids.push(sid);
                }
            }
        }

        removed_session_ids
    }

    pub fn apply_snapshot(&mut self, payload: &Value) {
        if let Some(arr) = payload.get("sessions").and_then(Value::as_array) {
            self.live_sessions.clear();
            for s in arr {
                let sid = s
                    .get("acp_session_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(sid) = sid {
                    self.live_sessions.insert(sid.clone(), s.clone());
                }
            }
        }
        if let Some(arr) = payload.get("projects").and_then(Value::as_array) {
            self.live_projects = arr.clone();
        }
    }
}
