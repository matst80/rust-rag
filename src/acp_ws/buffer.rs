use std::collections::{HashMap, VecDeque};
use super::AcpEvent;

#[derive(Debug, Default)]
pub struct SessionBuffer {
    pub events: VecDeque<AcpEvent>,
}

#[derive(Debug, Default)]
pub struct BufferManager {
    pub next_seq: u64,
    /// Ring buffer per session id. Events without a session id go in the empty-string bucket.
    pub buffers: HashMap<String, SessionBuffer>,
    pub cap_per_session: usize,
}

impl BufferManager {
    pub fn new(cap_per_session: usize) -> Self {
        Self {
            next_seq: 0,
            buffers: HashMap::new(),
            cap_per_session,
        }
    }

    pub fn next_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    pub fn push_event(&mut self, session_id: Option<&String>, event: AcpEvent) {
        let bucket = session_id.cloned().unwrap_or_default();
        let buf = self.buffers.entry(bucket).or_default();
        buf.events.push_back(event);
        while buf.events.len() > self.cap_per_session {
            buf.events.pop_front();
        }
    }

    pub fn remove_session(&mut self, session_id: &str) {
        self.buffers.remove(session_id);
    }
}
