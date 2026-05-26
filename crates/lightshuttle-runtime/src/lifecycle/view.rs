//! Lightweight value types exposed by the control plane.
//!
//! Built on demand by [`crate::ManagerHandle`] and surfaced through the
//! [`crate::LifecycleHandle`] trait. The shape is intentionally stable
//! across runtime backends so callers (REST API, dashboard, CLI) never
//! see backend-specific types.

use std::time::SystemTime;

use crate::lifecycle::status::NodeStatus;
use crate::spec::ImageSource;

/// Dashboard-friendly view of a single managed resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceView {
    /// Manifest-declared resource name.
    pub name: String,
    /// Resource kind discriminant (`postgres`, `redis`, `container`, `dockerfile`).
    pub kind: String,
    /// Coarse-grained status suitable for UI rendering.
    pub status: ResourceStatus,
    /// Whether the resource passed its healthcheck.
    pub healthy: bool,
    /// Container image reference, as resolved at start time.
    pub image: String,
    /// Wall-clock time at which the runtime accepted the start request.
    pub started_at: Option<SystemTime>,
    /// Last terminal failure reason, when applicable.
    pub last_error: Option<String>,
}

/// Coarse-grained resource status, derived from [`NodeStatus`] and
/// flattened for UI consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStatus {
    /// Not started yet.
    Pending,
    /// Container is booting up.
    Starting,
    /// Container is up, with or without a green healthcheck.
    Running,
    /// Resource has entered a terminal failure state.
    Failed,
    /// Resource has been stopped on request.
    Stopped,
}

impl From<&NodeStatus> for ResourceStatus {
    fn from(status: &NodeStatus) -> Self {
        match status {
            NodeStatus::Pending => Self::Pending,
            NodeStatus::Starting => Self::Starting,
            NodeStatus::Running | NodeStatus::Healthy => Self::Running,
            NodeStatus::Failed { .. } => Self::Failed,
            NodeStatus::Stopped => Self::Stopped,
        }
    }
}

/// Render an [`ImageSource`] as the user-facing image reference: the
/// pulled image string for `Pull`, the produced tag for `Build`.
pub(crate) fn image_label(src: &ImageSource) -> String {
    match src {
        ImageSource::Pull(s) => s.clone(),
        ImageSource::Build { tag, .. } => tag.clone(),
    }
}

/// Extract the last error message from a [`NodeStatus`], when terminal.
pub(crate) fn last_error_from(status: &NodeStatus) -> Option<String> {
    match status {
        NodeStatus::Failed { reason } => Some(reason.clone()),
        _ => None,
    }
}
