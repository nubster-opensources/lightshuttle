//! `GET /ws/events` — broadcast lifecycle events as JSON text frames.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade, close_code};
use axum::response::Response;
use lightshuttle_runtime::{LifecycleEvent, LifecycleHandle};
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;

use crate::state::ControlState;

/// Axum handler upgrading the request to a WebSocket on the lifecycle
/// event broadcast.
pub(crate) async fn events_ws<H>(
    ws: WebSocketUpgrade,
    State(state): State<ControlState<H>>,
) -> Response
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let receiver = state.handle.subscribe_events();
    ws.on_upgrade(move |socket| handle_socket(socket, receiver))
}

/// Pump every event received from the broadcast as a JSON text frame.
async fn handle_socket(
    mut socket: WebSocket,
    mut receiver: tokio::sync::broadcast::Receiver<LifecycleEvent>,
) {
    loop {
        tokio::select! {
            recv = receiver.recv() => match recv {
                Ok(event) => {
                    let text = match serde_json::to_string(&event) {
                        Ok(s) => s,
                        Err(err) => {
                            warn!(error = %err, "failed to serialise lifecycle event");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(skipped)) => {
                    warn!(skipped, "events websocket subscriber lagged");
                }
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            }
        }
    }

    let _ = socket
        .send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code: close_code::NORMAL,
            reason: "stream ended".into(),
        })))
        .await;
}
