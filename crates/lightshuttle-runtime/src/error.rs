//! Error types returned by container runtime operations.
//!
//! All fallible runtime methods return [`Result<T>`], which is a type alias
//! for `std::result::Result<T, RuntimeError>`.

use std::time::Duration;

/// Shorthand alias for `std::result::Result<T, `[`RuntimeError`]`>`.
///
/// Used throughout this crate so callers never have to spell out the full
/// error type on every return position.
pub type Result<T> = std::result::Result<T, RuntimeError>;

/// Errors raised by a [`crate::ResourceRuntime`] implementation.
///
/// The enum is `#[non_exhaustive]`: it now covers native processes as well as
/// containers, so new variants will keep arriving, and callers must carry a
/// wildcard arm rather than be broken by each one.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

    /// The runtime refused to remove a container.
    #[error("failed to remove container `{name}`")]
    Remove {
        /// Name of the container that could not be removed.
        name: String,
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

    /// Creating the per-project Docker bridge network failed.
    #[error("failed to create network `{name}`")]
    NetworkCreate {
        /// Name of the network that could not be created.
        name: String,
        /// Underlying error from the container daemon.
        #[source]
        source: bollard::errors::Error,
    },

    /// Removing the per-project Docker bridge network failed.
    #[error("failed to remove network `{name}`")]
    NetworkRemove {
        /// Name of the network that could not be removed.
        name: String,
        /// Underlying error from the container daemon.
        #[source]
        source: bollard::errors::Error,
    },

    /// Building an image from a Dockerfile failed.
    #[error("failed to build image from Dockerfile")]
    Build(#[source] bollard::errors::Error),

    /// The `BuildKit` builder reported a failure in its progress stream.
    #[error("image build failed: {0}")]
    BuildFailed(String),

    /// The provided [`crate::ContainerSpec`] is structurally invalid.
    #[error("invalid container spec: {0}")]
    InvalidSpec(String),

    /// The program named by a `process` resource could not be found.
    ///
    /// Reported with the program as written in the manifest, never the
    /// expanded search path: the manifest is what the developer can fix.
    #[error("no executable named `{program}` was found on PATH")]
    ExecutableNotFound {
        /// Program as the manifest names it.
        program: String,
    },

    /// A `process` resource could not be started.
    #[error("failed to start process `{resource}` running `{program}`")]
    ProcessStart {
        /// Resource name as declared in the manifest.
        resource: String,
        /// Program that could not be started.
        program: String,
        /// Underlying operating system error.
        #[source]
        source: std::io::Error,
    },

    /// A process group could not be signalled, or did not end.
    #[error("failed to stop the process group led by {pid}")]
    ProcessStop {
        /// Process number of the group leader.
        pid: u32,
        /// What went wrong, as the platform reported it.
        reason: String,
    },

    /// An operation named a `process` this runtime does not supervise.
    ///
    /// Distinct from [`Self::NotFound`], which names a container the daemon
    /// does not hold: this one means the identifier does not belong to this
    /// supervisor at all, which is what a `logs` or `inspect` on a process
    /// started by a different `up` looks like.
    #[error("process `{name}` is not supervised by this runtime")]
    ProcessNotSupervised {
        /// Identifier the caller passed.
        name: String,
    },

    /// The on-disk process registry of a project could not be read or written.
    #[error("failed to access the process registry at `{path}`")]
    ProcessRegistry {
        /// Path of the registry file.
        path: String,
        /// Underlying error.
        reason: String,
    },
}
