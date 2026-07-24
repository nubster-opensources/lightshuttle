//! `lightshuttle restart`.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use owo_colors::OwoColorize;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;
use tracing::warn;

use super::ExitOutcome;
use crate::control_url;

/// Reasonable upper bound for a restart cycle on a developer machine.
/// Past this point, we surface a runtime error rather than block the
/// CLI forever waiting on an event that may never come.
const FOLLOW_TIMEOUT: Duration = Duration::from_secs(90);

/// Upper bound for the events websocket handshake itself, kept separate
/// from the terminal-event wait so a slow connect fails fast.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Connected `/ws/events` websocket stream over a (possibly TLS-wrapped)
/// loopback TCP connection.
type EventSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Minimum subset of a `LifecycleEvent` JSON frame the CLI needs.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LifecycleEvent {
    ResourceHealthy {
        name: String,
    },
    ResourceFailed {
        name: String,
        #[serde(default)]
        error: String,
    },
    #[serde(other)]
    Other,
}

/// Entry point for `lightshuttle restart`.
pub(crate) async fn run(resource: &str, detach: bool) -> Result<ExitOutcome> {
    let cwd = std::env::current_dir().context("failed to read current working directory")?;
    let base = control_url::read(&cwd).context(
        "could not read .lightshuttle/control.url; is `lightshuttle up` running in this folder?",
    )?;
    let client = build_client()?;
    restart_and_follow(&client, &base, resource, detach, FOLLOW_TIMEOUT).await
}

/// Build the HTTP client used to reach the control plane.
///
/// Redirects are disabled on purpose: the control plane is loopback only,
/// and a redirect would be the only way to leave it.
fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build the HTTP client")
}

/// Drive a restart to its observable outcome.
///
/// In foreground mode the events websocket is opened *before* the restart
/// POST is sent. The control plane registers the broadcast receiver while
/// upgrading the socket, before it returns `101`, so a socket connected here
/// is guaranteed to observe every event the restart emits afterwards: the
/// terminal event can no longer land in a window with no subscriber (#277).
///
/// In `--detach` mode no socket is opened and the call returns as soon as the
/// restart is admitted.
async fn restart_and_follow(
    client: &reqwest::Client,
    base: &str,
    resource: &str,
    detach: bool,
    timeout: Duration,
) -> Result<ExitOutcome> {
    if detach {
        let status = post_restart(client, base, resource).await?;
        admit_or_bail(status, resource)?;
        return Ok(ExitOutcome::Success);
    }

    // Subscribe before triggering.
    let mut socket = connect_events(base).await?;
    let status = post_restart(client, base, resource).await?;
    admit_or_bail(status, resource)?;
    wait_terminal(&mut socket, resource, timeout).await
}

/// Send the restart POST and return the raw HTTP status.
async fn post_restart(
    client: &reqwest::Client,
    base: &str,
    resource: &str,
) -> Result<reqwest::StatusCode> {
    let url = join_url(base, &format!("api/resources/{resource}/restart"));
    let response = client
        .post(&url)
        .send()
        .await
        .context("failed to reach the control plane")?;
    Ok(response.status())
}

/// Interpret the restart admission status: announce a scheduled restart on
/// `202`, or map every rejection to a user-facing error.
fn admit_or_bail(status: reqwest::StatusCode, resource: &str) -> Result<()> {
    match status {
        reqwest::StatusCode::ACCEPTED => {
            println!(
                "{} {}",
                "restart accepted for".green().bold(),
                resource.cyan().bold()
            );
            Ok(())
        }
        reqwest::StatusCode::NOT_FOUND => bail!("unknown resource `{resource}`"),
        reqwest::StatusCode::CONFLICT => {
            bail!("a restart of `{resource}` is already in progress")
        }
        other => bail!("unexpected control plane response: {other}"),
    }
}

/// Open the `/ws/events` websocket, bounded by [`CONNECT_TIMEOUT`].
async fn connect_events(base: &str) -> Result<EventSocket> {
    let ws_url = to_ws_url(base) + "ws/events";
    let (socket, _resp) =
        tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(&ws_url))
            .await
            .context("timed out connecting to the events websocket")?
            .context("failed to connect to the events websocket")?;
    Ok(socket)
}

/// Block until a terminal event for `resource` arrives or `timeout` fires.
async fn wait_terminal(
    socket: &mut EventSocket,
    resource: &str,
    timeout: Duration,
) -> Result<ExitOutcome> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("timed out waiting for `{resource}` to become healthy after restart");
        }

        let Some(msg) = tokio::time::timeout(remaining, socket.next())
            .await
            .context("timed out reading from the events websocket")?
        else {
            bail!("events websocket closed before `{resource}` reported a terminal status");
        };

        let frame = msg.context("events websocket frame error")?;
        let text = match frame {
            Message::Text(t) => t,
            Message::Close(_) => {
                bail!("events websocket closed before `{resource}` reported a terminal status");
            }
            _ => continue,
        };

        let raw: &str = text.as_str();
        let event: LifecycleEvent = match serde_json::from_str(raw) {
            Ok(e) => e,
            Err(err) => {
                warn!(error = %err, raw = %raw, "ignoring malformed event frame");
                continue;
            }
        };

        match event {
            LifecycleEvent::ResourceHealthy { name } if name == resource => {
                println!(
                    "{} {}",
                    "resource healthy:".green().bold(),
                    resource.cyan().bold()
                );
                return Ok(ExitOutcome::Success);
            }
            LifecycleEvent::ResourceFailed { name, error } if name == resource => {
                eprintln!(
                    "{} {}: {}",
                    "resource failed:".red().bold(),
                    resource.cyan().bold(),
                    error
                );
                return Ok(ExitOutcome::LifecycleFailed);
            }
            _ => {}
        }
    }
}

/// Join `base` (which always ends with `/`) and a relative `path`
/// without doubling up the separator.
fn join_url(base: &str, path: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

/// Convert an `http(s)://...` URL to its `ws(s)://...` equivalent.
fn to_ws_url(base: &str) -> String {
    if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_owned()
    }
}

#[cfg(test)]
mod follow_tests {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use lightshuttle_control::{ControlServer, ControlState, bind};
    use lightshuttle_runtime::{
        LifecycleEvent, LifecycleHandle, LifecycleHandleError, LogChunkStream, ResourceStatus,
        ResourceView, RestartPermit,
    };
    use tokio::sync::broadcast;

    use super::{ExitOutcome, build_client, restart_and_follow};

    /// A control-plane handle whose restart broadcasts the terminal event
    /// the instant the operation runs. A CLI that subscribed only after the
    /// POST returned would already have missed it: this is the fast restart
    /// that reproduces issue #277.
    #[derive(Clone)]
    struct EagerRestartHandle {
        events: Arc<broadcast::Sender<LifecycleEvent>>,
        conflict: bool,
        emit: bool,
    }

    impl EagerRestartHandle {
        fn new() -> Self {
            let (events, _rx) = broadcast::channel(16);
            Self {
                events: Arc::new(events),
                conflict: false,
                emit: true,
            }
        }

        /// Admission reports a restart already in flight, so the endpoint
        /// answers `409 Conflict`.
        fn conflicting() -> Self {
            Self {
                conflict: true,
                ..Self::new()
            }
        }

        /// The restart never broadcasts a terminal event: used to prove a
        /// detached restart returns without waiting on the event stream.
        fn silent() -> Self {
            Self {
                emit: false,
                ..Self::new()
            }
        }
    }

    impl LifecycleHandle for EagerRestartHandle {
        async fn list(&self) -> Result<Vec<ResourceView>, LifecycleHandleError> {
            Ok(vec![view("cache")])
        }

        async fn get(&self, name: &str) -> Result<ResourceView, LifecycleHandleError> {
            if name == "cache" {
                Ok(view("cache"))
            } else {
                Err(LifecycleHandleError::UnknownResource(name.to_owned()))
            }
        }

        async fn restart(&self, name: &str) -> Result<(), LifecycleHandleError> {
            if self.emit {
                let _ = self.events.send(LifecycleEvent::ResourceHealthy {
                    name: name.to_owned(),
                });
            }
            Ok(())
        }

        fn try_admit_restart(&self, name: &str) -> Result<RestartPermit, LifecycleHandleError> {
            if self.conflict {
                Err(LifecycleHandleError::RestartInProgress(name.to_owned()))
            } else {
                Ok(RestartPermit::unguarded(name))
            }
        }

        async fn logs(
            &self,
            name: &str,
            _follow: bool,
        ) -> Result<LogChunkStream, LifecycleHandleError> {
            Err(LifecycleHandleError::UnknownResource(name.to_owned()))
        }

        fn subscribe_events(&self) -> broadcast::Receiver<LifecycleEvent> {
            self.events.subscribe()
        }
    }

    fn view(name: &str) -> ResourceView {
        ResourceView {
            name: name.to_owned(),
            kind: "redis".to_owned(),
            status: ResourceStatus::Running,
            healthy: true,
            image: "redis:latest".to_owned(),
            started_at: Some(SystemTime::UNIX_EPOCH),
            last_error: None,
        }
    }

    async fn spawn_server(handle: EagerRestartHandle) -> String {
        let addr = "127.0.0.1:0"
            .parse::<std::net::SocketAddr>()
            .expect("loopback address parses");
        let listener = bind(addr).await.expect("bind loopback listener");
        let bound = listener.local_addr().expect("read bound address");
        let server = ControlServer::new(ControlState::new("demo", handle));
        tokio::spawn(async move {
            server
                .serve(listener, std::future::pending::<()>())
                .await
                .ok();
        });
        format!("http://{bound}/")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn foreground_restart_observes_a_very_fast_terminal_event() {
        let base = spawn_server(EagerRestartHandle::new()).await;
        let client = build_client().expect("http client builds");

        let outcome = restart_and_follow(&client, &base, "cache", false, Duration::from_secs(5))
            .await
            .expect("a foreground restart must observe its terminal event");

        assert!(
            matches!(outcome, ExitOutcome::Success),
            "expected Success, got {outcome:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn foreground_restart_surfaces_409_as_restart_in_progress() {
        let base = spawn_server(EagerRestartHandle::conflicting()).await;
        let client = build_client().expect("http client builds");

        let error = restart_and_follow(&client, &base, "cache", false, Duration::from_secs(5))
            .await
            .expect_err("a conflicting restart must be surfaced as an error");

        assert!(
            error.to_string().contains("already in progress"),
            "expected a restart-in-progress message, got: {error}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn detach_returns_immediately_without_a_terminal_event() {
        // The handle never broadcasts. A detached restart must still return
        // at once: if it waited on the event stream it would hang until the
        // (deliberately short) timeout and fail.
        let base = spawn_server(EagerRestartHandle::silent()).await;
        let client = build_client().expect("http client builds");

        let outcome = restart_and_follow(&client, &base, "cache", true, Duration::from_secs(2))
            .await
            .expect("a detached restart returns without following events");

        assert!(
            matches!(outcome, ExitOutcome::Success),
            "expected Success, got {outcome:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{join_url, to_ws_url};

    #[test]
    fn join_url_preserves_trailing_slash() {
        assert_eq!(
            join_url("http://127.0.0.1:1234/", "api/x"),
            "http://127.0.0.1:1234/api/x"
        );
        assert_eq!(
            join_url("http://127.0.0.1:1234", "api/x"),
            "http://127.0.0.1:1234/api/x"
        );
    }

    #[test]
    fn to_ws_url_swaps_scheme() {
        assert_eq!(to_ws_url("http://x/"), "ws://x/");
        assert_eq!(to_ws_url("https://x/"), "wss://x/");
    }
}
