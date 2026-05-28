//! `GET /ws/logs/:name` — stream container logs over a WebSocket.

use std::time::SystemTime;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{Path, State};
use axum::response::Response;
use futures::StreamExt;
use lightshuttle_runtime::{
    LifecycleHandle, LifecycleHandleError, LogChunk, LogChunkStream, LogStream,
};
use serde::Serialize;
use tracing::warn;

use crate::state::ControlState;

/// Wire shape of a single log frame.
#[derive(Debug, Serialize)]
struct LogFrame<'a> {
    /// Source stream of the chunk: `"stdout"` or `"stderr"`.
    stream: &'static str,
    /// Wall-clock timestamp seconds since UNIX epoch.
    ts_secs: u64,
    /// Sub-second component, nanoseconds in `0..1_000_000_000`.
    ts_nanos: u32,
    /// Best-effort UTF-8 decoding of the chunk bytes.
    data: &'a str,
}

/// Axum handler upgrading the request to a WebSocket.
pub(crate) async fn logs_ws<H>(
    ws: WebSocketUpgrade,
    State(state): State<ControlState<H>>,
    Path(name): Path<String>,
) -> Response
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    ws.on_upgrade(move |socket| handle_socket(socket, state.handle, name))
}

/// Drive the WebSocket lifecycle: open upstream stream, forward chunks
/// as text frames, react to client close, exit cleanly on upstream
/// EOF.
async fn handle_socket<H>(mut socket: WebSocket, handle: H, name: String)
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    let mut stream = match handle.logs(&name, true).await {
        Ok(s) => s,
        Err(LifecycleHandleError::UnknownResource(_)) => {
            send_close(
                &mut socket,
                close_code::UNSUPPORTED,
                format!("unknown resource: {name}"),
            )
            .await;
            return;
        }
        Err(err) => {
            send_close(&mut socket, close_code::ERROR, err.to_string()).await;
            return;
        }
    };

    forward(&mut socket, &mut stream).await;

    // Normal close once upstream is drained or client went away.
    let _ = socket.send(Message::Close(None)).await;
}

/// Pump one frame per upstream chunk until either side closes. Returns
/// when the upstream stream is exhausted, the client sends a close
/// message or any send/recv fails.
async fn forward(socket: &mut WebSocket, stream: &mut LogChunkStream) {
    loop {
        tokio::select! {
            chunk = stream.next() => match chunk {
                Some(Ok(chunk)) => {
                    let text = encode_frame(&chunk);
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return; // client disconnected mid-send
                    }
                }
                Some(Err(err)) => {
                    warn!(error = %err, "upstream log stream error");
                    return;
                }
                None => return, // upstream EOF
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {} // ignore pings/pongs/text/binary from client
            }
        }
    }
}

/// JSON-encode one chunk into the wire frame shape.
fn encode_frame(chunk: &LogChunk) -> String {
    let (secs, nanos) = chunk
        .timestamp
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| (d.as_secs(), d.subsec_nanos()))
        .unwrap_or((0, 0));
    let data = String::from_utf8_lossy(&chunk.bytes);
    let frame = LogFrame {
        stream: match chunk.stream {
            LogStream::Stdout => "stdout",
            LogStream::Stderr => "stderr",
        },
        ts_secs: secs,
        ts_nanos: nanos,
        data: data.as_ref(),
    };
    serde_json::to_string(&frame).unwrap_or_else(|_| String::from("{}"))
}

/// Send a close frame with a custom code and reason, swallowing send
/// errors so the caller can return cleanly.
async fn send_close(socket: &mut WebSocket, code: u16, reason: String) {
    let frame = Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }));
    let _ = socket.send(frame).await;
}
