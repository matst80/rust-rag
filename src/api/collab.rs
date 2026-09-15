use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;

#[derive(Clone, Default)]
pub struct CollabRegistry {
    /// room_id -> broadcast sender
    rooms: Arc<Mutex<HashMap<String, broadcast::Sender<Vec<u8>>>>>,
}

impl CollabRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_or_create_room(&self, room: &str) -> (broadcast::Sender<Vec<u8>>, broadcast::Receiver<Vec<u8>>) {
        let mut guard = self.rooms.lock().expect("collab rooms poisoned");
        if let Some(tx) = guard.get(room) {
            (tx.clone(), tx.subscribe())
        } else {
            let (tx, rx) = broadcast::channel(256);
            guard.insert(room.to_owned(), tx.clone());
            (tx, rx)
        }
    }

    fn cleanup_empty_rooms(&self) {
        let mut guard = self.rooms.lock().expect("collab rooms poisoned");
        guard.retain(|_, tx| tx.receiver_count() > 0);
    }
}

#[derive(serde::Deserialize)]
pub struct CollabQuery {
    pub room: Option<String>,
}

/// WebSocket endpoint for Yjs binary sync relay
pub async fn collab_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<super::state::AppState>,
    Query(query): Query<CollabQuery>,
) -> impl IntoResponse {
    let room = query.room.unwrap_or_else(|| "default".to_owned());
    ws.on_upgrade(move |socket| handle_collab_socket(socket, state, room))
}

async fn handle_collab_socket(socket: WebSocket, state: super::state::AppState, room: String) {
    let (mut sink, mut stream) = socket.split();
    let registry = &state.collab;
    let (tx, mut rx) = registry.get_or_create_room(&room);

    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Forward broadcasts from other peers in this room to client
            res = rx.recv() => {
                match res {
                    Ok(bytes) => {
                        if sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!(skipped = n, "collab_ws: subscriber lagged");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Read incoming Yjs messages from this client and broadcast to room
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Binary(bytes))) => {
                        let _ = tx.send(bytes.to_vec());
                    }
                    Some(Ok(Message::Ping(p))) => {
                        if sink.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::debug!(error = %e, "collab_ws: client read error");
                        break;
                    }
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                if sink.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
    }

    state.collab.cleanup_empty_rooms();
}
