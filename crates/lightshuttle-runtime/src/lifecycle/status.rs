//! Per-node status and lifecycle event types.
//!
//! [`NodeStatus`] is the fine-grained internal status carried by each
//! `tokio::sync::watch` channel inside the manager. [`LifecycleEvent`] is the
//! externally broadcast event emitted on the `tokio::sync::broadcast` channel
//! returned by [`crate::LifecycleManager::subscribe_events`].
//!
//! The two types serve different consumers: `NodeStatus` is used internally by
//! `start_one` to gate dependency ordering; `LifecycleEvent` is consumed by
//! CLI progress bars, dashboard WebSocket connections, and test assertions.

use serde::Serialize;

/// Lifecycle status of a single managed resource.
///
/// Broadcast through a `tokio::sync::watch` channel so dependents can
/// wait for their dependencies to become ready without polling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    /// The resource has not been started yet.
    Pending,
    /// The runtime accepted the start request; the container is booting.
    Starting,
    /// The container is up but does not declare a healthcheck or has
    /// not produced a healthcheck result yet.
    Running,
    /// The container is up and reports a successful healthcheck.
    Healthy,
    /// The resource entered a terminal failure state with the recorded
    /// reason.
    Failed {
        /// Free-form failure reason for diagnostics.
        reason: String,
    },
    /// The resource has been stopped on request.
    Stopped,
}

impl NodeStatus {
    /// Whether the resource is considered ready for dependents to
    /// start on top of it.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Healthy | Self::Running)
    }

    /// Whether the resource is in a terminal state (failed or stopped).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::Stopped)
    }
}

/// Event emitted by [`crate::LifecycleManager`] for consumption by a
/// CLI, dashboard or test harness.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LifecycleEvent {
    /// A resource has been created and started by the runtime.
    ResourceStarted {
        /// Resource name as declared in the manifest.
        name: String,
        /// Container identifier returned by the runtime.
        container_id: String,
    },
    /// A resource passed its healthcheck.
    ResourceHealthy {
        /// Resource name.
        name: String,
    },
    /// A resource failed and will not run.
    ResourceFailed {
        /// Resource name.
        name: String,
        /// Human-readable failure description.
        error: String,
    },
    /// A resource has been stopped cleanly.
    ResourceStopped {
        /// Resource name.
        name: String,
    },
    /// Every resource has reached a ready state.
    StackStarted,
    /// The manager has started rolling the stack down.
    StackStopping,
    /// Every resource has been stopped.
    StackStopped,
}
