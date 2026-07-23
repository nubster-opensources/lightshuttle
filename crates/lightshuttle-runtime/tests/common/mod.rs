//! Shared helpers for the Docker integration tests.
//!
//! These utilities make the integration suite deterministic and self
//! cleaning: every test runs under a collision-free project name, waits on
//! lifecycle events instead of sleeping, and tears down its containers and
//! network through an RAII guard even when an assertion panics.

#![allow(dead_code)]

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use lightshuttle_runtime::LifecycleEvent;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

/// Returns `true` when a Docker daemon answers `docker info`.
///
/// The ignored integration tests call this to skip gracefully on a machine
/// without Docker instead of failing. Presence is detected by running
/// `docker info` and checking only the exit status.
#[must_use]
pub(crate) fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Returns a collision-free project name built from `prefix`, the current
/// process id and a per-process atomic counter.
///
/// The result is restricted to `[a-z0-9-]` so it is a valid DNS label and a
/// valid Docker network suffix, letting two tests in the same run (or two
/// runs on the same machine) never share containers or a network.
#[must_use]
pub(crate) fn unique_project(prefix: &str) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ls-it-{prefix}-{}-{sequence}", std::process::id())
}

/// Awaits the `ResourceHealthy` event for `resource` on `events`, returning
/// early with an error when the resource fails or the deadline elapses.
///
/// Waiting on the lifecycle broadcast rather than a fixed sleep keeps the
/// integration tests fast and free of timing races. Lagged receivers are
/// tolerated; unrelated events are ignored.
///
/// # Errors
///
/// Returns an error when the resource emits `ResourceFailed`, when the event
/// channel closes before the resource becomes healthy, or when `timeout`
/// elapses first.
pub(crate) async fn wait_for_healthy(
    events: &mut Receiver<LifecycleEvent>,
    resource: &str,
    timeout: Duration,
) -> Result<(), String> {
    let wait = async {
        loop {
            match events.recv().await {
                Ok(LifecycleEvent::ResourceHealthy { name }) if name == resource => {
                    return Ok(());
                }
                Ok(LifecycleEvent::ResourceFailed { name, error }) if name == resource => {
                    return Err(format!("resource `{resource}` failed: {error}"));
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => {
                    return Err(format!(
                        "event channel closed before `{resource}` became healthy"
                    ));
                }
            }
        }
    };

    tokio::time::timeout(timeout, wait)
        .await
        .map_err(|_| format!("timed out waiting for `{resource}` to become healthy"))?
}

/// Returns `true` when the `lightshuttle-<project>` bridge network exists.
///
/// Shells out to `docker network inspect` and reports presence by exit status
/// alone, so the integration tests can assert a shutdown left no network
/// behind without depending on any private runtime API.
#[must_use]
pub(crate) fn network_exists(project: &str) -> bool {
    Command::new("docker")
        .args(["network", "inspect", &format!("lightshuttle-{project}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// RAII teardown for a LightShuttle project's Docker resources.
///
/// On drop it force-removes every container labelled
/// `lightshuttle.project=<project>` and removes the `lightshuttle-<project>`
/// network, shelling out to the Docker CLI. The teardown is synchronous and
/// best effort, so it runs correctly even after the async runtime is gone or
/// a test has panicked.
pub(crate) struct ProjectCleanup {
    project: String,
}

impl ProjectCleanup {
    /// Registers cleanup for `project`; resources are removed when the guard
    /// is dropped.
    #[must_use]
    pub(crate) fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
        }
    }

    /// The project name this guard cleans up.
    #[must_use]
    pub(crate) fn project(&self) -> &str {
        &self.project
    }
}

impl Drop for ProjectCleanup {
    fn drop(&mut self) {
        let label = format!("label=lightshuttle.project={}", self.project);
        if let Ok(listed) = Command::new("docker")
            .args(["ps", "-aq", "--filter", &label])
            .output()
        {
            for id in String::from_utf8_lossy(&listed.stdout).split_whitespace() {
                let _ = Command::new("docker").args(["rm", "-f", id]).output();
            }
        }
        let _ = Command::new("docker")
            .args(["network", "rm", &format!("lightshuttle-{}", self.project)])
            .output();
    }
}
