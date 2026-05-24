//! Container runtime abstraction plus its supporting domain types.

use std::pin::Pin;
use std::time::{Duration, SystemTime};

use futures::stream::Stream;

use crate::error::Result;
use crate::spec::ContainerSpec;

/// Opaque identifier for a container managed by the runtime.
///
/// The internal representation is whatever string the underlying daemon
/// uses (Docker returns 64-character hexadecimal hashes); callers must
/// not depend on the format.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerId(String);

impl ContainerId {
    /// Build a [`ContainerId`] from a daemon-supplied string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the raw identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContainerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lifecycle status reported by the runtime when inspecting a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerStatus {
    /// The runtime has accepted the start request but the container is
    /// not yet running.
    Starting,

    /// The container is running and either has no healthcheck or has
    /// not produced a healthcheck result yet.
    Running,

    /// The container is running and reports a healthy healthcheck.
    Healthy,

    /// The container is running and reports an unhealthy healthcheck.
    Unhealthy,

    /// The container has exited.
    Stopped {
        /// Exit code reported by the container, when known.
        exit_code: Option<i32>,
    },
}

/// Which stream a log chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// One chunk of streamed log output.
#[derive(Debug, Clone)]
pub struct LogChunk {
    /// Source stream of the chunk.
    pub stream: LogStream,
    /// Wall-clock timestamp reported by the runtime.
    pub timestamp: SystemTime,
    /// Raw bytes of the chunk; may or may not end with a newline.
    pub bytes: Vec<u8>,
}

/// Boxed stream of log chunks for a single container.
pub type LogChunkStream = Pin<Box<dyn Stream<Item = Result<LogChunk>> + Send>>;

/// Container runtime abstraction.
///
/// The trait is intentionally narrow: it exposes only the operations
/// that the lifecycle manager needs. Daemon-specific capabilities
/// (network inspection, image management) stay private to each
/// implementation.
///
/// Implementations live in submodules such as [`crate::DockerRuntime`].
pub trait ContainerRuntime: Send + Sync {
    /// Start a container according to `spec`. Pulls the image if not
    /// already present locally.
    fn start(
        &self,
        spec: &ContainerSpec,
    ) -> impl std::future::Future<Output = Result<ContainerId>> + Send;

    /// Stop a container, sending `SIGTERM` and then `SIGKILL` after
    /// `grace`. Idempotent: stopping an already stopped container is a
    /// no-op.
    fn stop(
        &self,
        id: &ContainerId,
        grace: Duration,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Report the current status of a container.
    fn inspect(
        &self,
        id: &ContainerId,
    ) -> impl std::future::Future<Output = Result<ContainerStatus>> + Send;

    /// Block until the container reports a healthy status or `timeout`
    /// elapses. Returns [`crate::RuntimeError::Timeout`] in the latter
    /// case.
    fn wait_healthy(
        &self,
        id: &ContainerId,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Stream logs from a container. When `follow` is true the stream
    /// stays open and emits new chunks as they arrive; when false the
    /// stream completes after the existing logs are drained.
    fn logs(
        &self,
        id: &ContainerId,
        follow: bool,
    ) -> impl std::future::Future<Output = Result<LogChunkStream>> + Send;
}
