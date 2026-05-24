//! Error type returned by container runtime operations.

use std::time::Duration;

/// Shorthand alias for `std::result::Result<T, RuntimeError>`.
pub type Result<T> = std::result::Result<T, RuntimeError>;

/// Errors raised by a [`crate::ContainerRuntime`] implementation.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// The runtime could not establish a connection to the underlying
    /// container daemon (Docker socket, Podman API, ...).
    #[error("failed to connect to the container runtime")]
    Connect(#[source] bollard::errors::Error),

    /// Pulling an image from the registry failed.
    #[error("failed to pull image `{image}`")]
    ImagePull {
        /// The image reference that the runtime tried to pull.
        image: String,
        /// Underlying error from the container daemon.
        #[source]
        source: bollard::errors::Error,
    },

    /// The runtime refused to start the container.
    #[error("failed to start container")]
    Start(#[source] bollard::errors::Error),

    /// The runtime refused to stop a container.
    #[error("failed to stop container `{id}`")]
    Stop {
        /// Identifier of the container that could not be stopped.
        id: String,
        /// Underlying error from the container daemon.
        #[source]
        source: bollard::errors::Error,
    },

    /// The runtime could not inspect a container.
    #[error("failed to inspect container `{id}`")]
    Inspect {
        /// Identifier of the container that could not be inspected.
        id: String,
        /// Underlying error from the container daemon.
        #[source]
        source: bollard::errors::Error,
    },

    /// The container does not exist on the daemon.
    #[error("container `{0}` not found")]
    NotFound(String),

    /// A blocking operation exceeded its allotted time budget.
    #[error("operation `{operation}` timed out after {after:?}")]
    Timeout {
        /// Short name of the operation that timed out.
        operation: &'static str,
        /// Configured timeout.
        after: Duration,
    },

    /// Streaming logs from a container failed mid-flight.
    #[error("log stream error")]
    LogStream(#[source] bollard::errors::Error),

    /// Building an image from a Dockerfile failed.
    #[error("failed to build image from Dockerfile")]
    Build(#[source] bollard::errors::Error),

    /// The provided [`crate::ContainerSpec`] is structurally invalid.
    #[error("invalid container spec: {0}")]
    InvalidSpec(String),
}
