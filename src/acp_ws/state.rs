use std::collections::HashMap;
use serde_json::{Value, json};

use super::AcpEvent;
use super::session::SessionManager;
use super::terminal::TerminalManager;
use super::buffer::BufferManager;

#[derive(Debug, Default)]
pub struct AcpState {
    pub session_manager: SessionManager,
    pub terminal_manager: TerminalManager,
    pub buffer_manager: BufferManager,

    /// Last snapshot decoded into struct form (for MCP `acp_get_snapshot`).
    pub latest_snapshot: Option<AcpEvent>,
    /// Verbatim text of the most recent snapshot frame, replayed verbatim
    /// to new subscribers when present.
    pub latest_snapshot_text: Option<String>,

    /// Outstanding PermissionRequest events keyed by request_id.
    pub pending_permissions: HashMap<String, AcpEvent>,

    /// Connection status for diagnostics.
    pub connected: bool,
    pub last_error: Option<String>,
}

impl AcpState {
    pub fn new(cap_per_session: usize) -> Self {
        Self {
            buffer_manager: BufferManager::new(cap_per_session),
            ..Default::default()
        }
    }

    pub fn is_snapshot(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("Snapshot")
            || kind.eq_ignore_ascii_case("snapshot")
            || kind == "state_snapshot"
            || kind == "commands_snapshot"
    }

    pub fn is_permission_request(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("PermissionRequest") || kind == "permission_request"
    }

    pub fn handle_event(&mut self, kind: &str, session_id: Option<&String>, payload: &Value, text: &str, event_id: Option<u64>, now_ms: i64) -> AcpEvent {
        let seq = self.buffer_manager.next_seq();

        let event = AcpEvent {
            local_seq: seq,
            event_id,
            kind: kind.to_string(),
            session_id: session_id.cloned(),
            payload: payload.clone(),
            received_at: now_ms,
        };

        if Self::is_snapshot(kind) {
            self.latest_snapshot = Some(event.clone());
            self.latest_snapshot_text = Some(text.to_string());

            self.session_manager.apply_snapshot(payload);
            self.terminal_manager.apply_snapshot(payload);

            // Synthesize history events for the buffer
            if let Some(arr) = payload.get("sessions").and_then(Value::as_array) {
                for s in arr {
                    if let Some(sid) = s.get("acp_session_id").and_then(Value::as_str) {
                        if let Some(hist) = s.get("history").and_then(Value::as_array) {
                            let mut synthesized = Vec::with_capacity(hist.len());
                            for h in hist {
                                let h_seq = self.buffer_manager.next_seq();
                                let h_kind = h.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string();
                                synthesized.push(AcpEvent {
                                    local_seq: h_seq,
                                    event_id: h.get("event_id").and_then(Value::as_u64),
                                    kind: h_kind,
                                    session_id: Some(sid.to_owned()),
                                    payload: h.clone(),
                                    received_at: now_ms,
                                });
                            }

                            let buf = self.buffer_manager.buffers.entry(sid.to_owned()).or_default();
                            buf.events.clear();
                            for ev in synthesized {
                                buf.events.push_back(ev);
                            }
                            while buf.events.len() > self.buffer_manager.cap_per_session {
                                buf.events.pop_front();
                            }
                        }
                    }
                }
            }
        }

        let removed_session_ids = self.session_manager.handle_event(kind, session_id, payload);
        self.terminal_manager.handle_event(kind, payload);

        if Self::is_permission_request(kind)
            && let Some(req_id) = payload.get("request_id").and_then(Value::as_str)
        {
            self.pending_permissions.insert(req_id.to_owned(), event.clone());
        }

        for sid in removed_session_ids {
            if SessionManager::is_topic_removed(kind) {
                 self.buffer_manager.remove_session(&sid);
            }
            self.pending_permissions.retain(|_, ev| ev.session_id.as_deref() != Some(sid.as_str()));
        }

        // Append to live history for subscriber_snapshot replay
        if let Some(sid) = session_id {
            if let Some(s) = self.session_manager.live_sessions.get_mut(sid) {
                if let Value::Object(map) = s {
                    let history = map.entry("history".to_string()).or_insert_with(|| Value::Array(Vec::new()));
                    if let Value::Array(arr) = history {
                        let mut item = payload.clone();
                        if let Value::Object(item_map) = &mut item {
                            item_map.insert("type".to_string(), Value::String(kind.to_string()));
                        }
                        arr.push(item);
                        while arr.len() > self.buffer_manager.cap_per_session {
                            arr.remove(0);
                        }
                    }
                }
            }
        }

        self.buffer_manager.push_event(session_id, event.clone());

        event
    }

    pub fn subscriber_snapshot(&self) -> Option<String> {
        if !self.session_manager.live_sessions.is_empty() || !self.session_manager.live_projects.is_empty() {
            let sessions: Vec<Value> = self.session_manager.live_sessions.values().cloned().collect();
            let terminals: Vec<Value> = self.terminal_manager.live_terminals.values().cloned().collect();
            let payload = json!({
                "type": "state_snapshot",
                "sessions": sessions,
                "projects": self.session_manager.live_projects,
                "terminals": terminals,
            });
            return Some(payload.to_string());
        }
        self.latest_snapshot_text.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_snapshot_includes_terminals() {
        let mut state = AcpState::new(10);

        let payload = json!({
            "type": "state_snapshot",
            "sessions": [
                {
                    "acp_session_id": "sess1",
                    "status": "Working",
                    "history": [
                        { "type": "some_event", "event_id": 1 }
                    ]
                }
            ],
            "projects": [
                {
                    "name": "my_proj",
                    "path": "/some/path"
                }
            ],
            "terminals": [
                {
                    "terminal_id": "term1",
                    "session_id": "sess1",
                    "cwd": "/some/path"
                }
            ]
        });

        let event = state.handle_event("state_snapshot", None, &payload, &payload.to_string(), Some(1), 100);

        // Check it correctly processed sessions and terminals
        assert_eq!(state.session_manager.live_sessions.len(), 1);
        assert_eq!(state.terminal_manager.live_terminals.len(), 1);
        assert_eq!(state.buffer_manager.buffers.get("sess1").unwrap().events.len(), 1);

        // Verify synthesization includes terminals
        let snapshot = state.subscriber_snapshot().expect("Expected synthesized snapshot");
        let parsed: Value = serde_json::from_str(&snapshot).unwrap();

        assert_eq!(parsed["type"], "state_snapshot");

        let sessions = parsed["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["acp_session_id"], "sess1");

        let terminals = parsed["terminals"].as_array().unwrap();
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0]["terminal_id"], "term1");
    }

    #[test]
    fn test_terminal_created_closed() {
        let mut state = AcpState::new(10);

        let created_payload = json!({
            "type": "terminal_created",
            "terminal": {
                "terminal_id": "term2",
                "session_id": "sess2"
            }
        });

        state.handle_event("terminal_created", Some(&"sess2".to_string()), &created_payload, "", Some(2), 200);

        assert_eq!(state.terminal_manager.live_terminals.len(), 1);
        assert_eq!(state.terminal_manager.live_terminals.get("term2").unwrap()["session_id"], "sess2");

        let closed_payload = json!({
            "type": "terminal_closed",
            "terminal_id": "term2"
        });

        state.handle_event("terminal_closed", Some(&"sess2".to_string()), &closed_payload, "", Some(3), 300);

        assert_eq!(state.terminal_manager.live_terminals.len(), 0);
    }
}
