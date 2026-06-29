use axum::extract::State;
use tracing::error;

use super::error::ApiError;
use super::state::AppState;

/// Proxy WebSocket connection to the Whisper transcription service.
/// Upstream address is configurable via RAG_WHISPER_WS_URL, defaulting to
/// the k8s service `whisper-slask-service.llm.svc.cluster.local`.
pub(crate) async fn whisper_proxy(
    ws: axum::extract::WebSocketUpgrade,
    State(state): State<AppState>,
) -> Result<axum::response::Response, ApiError> {
    let upstream_url = state.whisper.ws_url.clone();
    Ok(ws.on_upgrade(move |socket| whisper_proxy_task(socket, upstream_url)))
}

async fn whisper_proxy_task(client_socket: axum::extract::ws::WebSocket, upstream_url: String) {
    use axum::extract::ws::Message as AxumMessage;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    let (mut client_sink, mut client_stream) = client_socket.split();

    let (upstream_socket, _) = match connect_async(&upstream_url).await {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, url = %upstream_url, "whisper_proxy: failed to connect to upstream");
            return;
        }
    };

    let (mut upstream_sink, mut upstream_stream) = upstream_socket.split();

    let client_to_upstream = async {
        while let Some(Ok(msg)) = client_stream.next().await {
            let tungsten_msg = match msg {
                AxumMessage::Text(t) => TungsteniteMessage::Text(t.to_string()),
                AxumMessage::Binary(b) => TungsteniteMessage::Binary(b.to_vec()),
                AxumMessage::Ping(p) => TungsteniteMessage::Ping(p.to_vec().into()),
                AxumMessage::Pong(p) => TungsteniteMessage::Pong(p.to_vec().into()),
                AxumMessage::Close(c) => {
                    let close_frame =
                        c.map(|cf| tokio_tungstenite::tungstenite::protocol::CloseFrame {
                            code: cf.code.into(),
                            reason: cf.reason.to_string().into(),
                        });
                    TungsteniteMessage::Close(close_frame)
                }
            };
            if upstream_sink.send(tungsten_msg).await.is_err() {
                break;
            }
        }
    };

    let upstream_to_client = async {
        while let Some(Ok(msg)) = upstream_stream.next().await {
            let axum_msg = match msg {
                TungsteniteMessage::Text(t) => AxumMessage::Text(t.to_string().into()),
                TungsteniteMessage::Binary(b) => AxumMessage::Binary(b.into()),
                TungsteniteMessage::Ping(p) => AxumMessage::Ping(p.into()),
                TungsteniteMessage::Pong(p) => AxumMessage::Pong(p.into()),
                TungsteniteMessage::Close(c) => {
                    let close_frame = c.map(|cf| axum::extract::ws::CloseFrame {
                        code: cf.code.into(),
                        reason: cf.reason.to_string().into(),
                    });
                    AxumMessage::Close(close_frame)
                }
                TungsteniteMessage::Frame(_) => continue,
            };
            if client_sink.send(axum_msg).await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        _ = client_to_upstream => (),
        _ = upstream_to_client => (),
    }
}
