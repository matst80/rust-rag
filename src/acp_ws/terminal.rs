use std::collections::HashMap;
use serde_json::Value;

#[derive(Debug, Default)]
pub struct TerminalManager {
    /// Live terminal id -> most recently observed TerminalInfo
    pub live_terminals: HashMap<String, Value>,
}

impl TerminalManager {
    pub fn is_terminal_created(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("TerminalCreated") || kind == "terminal_created"
    }

    pub fn is_terminal_closed(kind: &str) -> bool {
        kind.eq_ignore_ascii_case("TerminalClosed") || kind == "terminal_closed"
    }

    pub fn handle_event(&mut self, kind: &str, payload: &Value) {
        if Self::is_terminal_created(kind) {
            let tinfo = payload.get("terminal").unwrap_or(payload);
            if let Some(tid) = tinfo.get("terminal_id").and_then(Value::as_str) {
                self.live_terminals.insert(tid.to_owned(), tinfo.clone());
            }
        }

        if Self::is_terminal_closed(kind) {
            if let Some(tid) = payload.get("terminal_id").and_then(Value::as_str) {
                self.live_terminals.remove(tid);
            }
        }
    }

    pub fn apply_snapshot(&mut self, payload: &Value) {
        if let Some(arr) = payload.get("terminals").and_then(Value::as_array) {
            self.live_terminals.clear();
            for t in arr {
                if let Some(tid) = t.get("terminal_id").and_then(Value::as_str) {
                    self.live_terminals.insert(tid.to_owned(), t.clone());
                }
            }
        }
    }
}
