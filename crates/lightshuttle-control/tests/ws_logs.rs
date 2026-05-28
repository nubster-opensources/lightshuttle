//! Integration tests for `GET /ws/logs/:name`.

use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use futures::stream::{self, Stream, StreamExt};
use lightshuttle_control::{ControlServer, ControlState, bind};
use lightshuttle_runtime::{
    LifecycleHandle, LifecycleHandleError, LogChunk, LogChunkStream, LogStream, ResourceView,
    RuntimeError,
};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

/// In-memory handle whose `logs()` returns a finite stream of three
/// canned chunks for known resources.
#[derive(Clone, Default)]
struct StubHandle;

impl LifecycleHandle for StubHandle {
    async fn list(&self) -> Result<Vec<ResourceView>, LifecycleHandleError> {
        Ok(Vec::new())
    }

    async fn get(&self, name: &str) -> Result<ResourceView, LifecycleHandleError> {
        Err(LifecycleHandleError::UnknownResource(name.to_owned()))
    }

    async fn restart(&self, _name: &str) -> Result<(), LifecycleHandleError> {
        Err(LifecycleHandleError::NotSupported("restart"))
    }

    async fn logs(
        &self,
        name: &str,
        _follow: bool,
    ) -> Result<LogChunkStream, LifecycleHandleError> {
        if name != "cache" {
            return Err(LifecycleHandleError::UnknownResource(name.to_owned()));
        }
        let chunks = vec![
            make_chunk(LogStream::Stdout, b"hello\n"),
            make_chunk(LogStream::Stderr, b"warning\n"),
            make_chunk(LogStream::Stdout, b"goodbye\n"),
        ];
        let stream: Pin<Box<dyn Stream<Item = Result<LogChunk, RuntimeError>> + Send>> =
            Box::pin(stream::iter(chunks).map(Ok));
        Ok(stream)
    }
}

fn make_chunk(stream: LogStream, bytes: &[u8]) -> LogChunk {
    LogChunk {
        stream,
        timestamp: SystemTime::UNIX_EPOCH,
        bytes: bytes.to_vec(),
    }
}

/// Spawn the server on a random localhost port and return the bound
/// URL plus a shutdown trigger.
async fn spawn_server() -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let state = ControlState::new("demo", StubHandle);
    let server = ControlServer::new(state);
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _ = server
            .serve(listener, async move {
                let _ = rx.await;
            })
            .await;
    });
    let url = format!("ws://{addr}/ws/logs/{{name}}");
    (url, tx, task)
}

#[tokio::test]
async fn streams_three_text_frames_for_known_resource() {
    let (template, shutdown_tx, task) = spawn_server().await;
    let url = template.replace("{name}", "cache");

    let (mut socket, _resp) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(&url),
    )
    .await
    .expect("connect timeout")
    .expect("connect");

    let mut texts: Vec<String> = Vec::new();
    while let Some(msg) = socket.next().await {
        match msg.expect("recv") {
            Message::Text(t) => texts.push(t.to_string()),
            Message::Close(_) => break,
            _ => {}
        }
        if texts.len() >= 3 {
            // Read the trailing close frame.
            let _ = socket.next().await;
            break;
        }
    }

    assert!(
        texts.len() >= 3,
        "expected at least 3 text frames, got {}",
        texts.len()
    );
    let first: serde_json::Value = serde_json::from_str(&texts[0]).expect("frame 0 is valid JSON");
    assert_eq!(first["stream"], "stdout");
    assert_eq!(first["data"], "hello\n");
    let second: serde_json::Value = serde_json::from_str(&texts[1]).expect("frame 1 is valid JSON");
    assert_eq!(second["stream"], "stderr");
    assert_eq!(second["data"], "warning\n");

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn unknown_resource_closes_with_code_1003() {
    let (template, shutdown_tx, task) = spawn_server().await;
    let url = template.replace("{name}", "nope");

    let (mut socket, _resp) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(&url),
    )
    .await
    .expect("connect timeout")
    .expect("connect");

    let mut close_seen: Option<(u16, String)> = None;
    while let Some(msg) = socket.next().await {
        if let Message::Close(Some(frame)) = msg.expect("recv") {
            close_seen = Some((frame.code.into(), frame.reason.to_string()));
            break;
        }
    }

    let (code, reason) = close_seen.expect("close frame received");
    assert_eq!(code, 1003);
    assert!(
        reason.contains("nope"),
        "reason should mention resource name, got: {reason}"
    );

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

// Arc usage check to silence dead_code lints if any field above goes
// unused as the surface evolves.
#[allow(dead_code)]
fn _kept_for_future_arc_handles(_a: Arc<u8>) {}
