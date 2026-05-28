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

    let client = reqwest::Client::new();
    let restart_url = join_url(&base, &format!("api/resources/{resource}/restart"));
    let response = client
        .post(&restart_url)
        .send()
        .await
        .context("failed to reach the control plane")?;

    match response.status() {
        reqwest::StatusCode::ACCEPTED => {
            println!(
                "{} {}",
                "restart accepted for".green().bold(),
                resource.cyan().bold()
            );
        }
        reqwest::StatusCode::NOT_FOUND => {
            bail!("unknown resource `{resource}`");
        }
        other => {
            bail!("unexpected control plane response: {other}");
        }
    }

    if detach {
        return Ok(ExitOutcome::Success);
    }

    follow_events(&base, resource).await
}

/// Connect to `/ws/events` and block until either a terminal event for
/// `resource` arrives or the global timeout fires.
async fn follow_events(base: &str, resource: &str) -> Result<ExitOutcome> {
    let ws_url = to_ws_url(base) + "ws/events";
    let (mut socket, _resp) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(&ws_url),
    )
    .await
    .context("timed out connecting to the events websocket")?
    .context("failed to connect to the events websocket")?;

    let deadline = tokio::time::Instant::now() + FOLLOW_TIMEOUT;

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
