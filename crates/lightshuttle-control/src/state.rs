//! Shared application state injected into axum handlers.

/// State shared by every route of the control plane.
///
/// At v0.2.0 it only carries the project name (surfaced through
/// `/healthz`); the lifecycle handle is added in a follow-up PR.
#[derive(Clone, Debug)]
pub struct ControlState {
    /// Project name as declared in the manifest.
    pub project: String,
}

impl ControlState {
    /// Build a new state bound to `project`.
    #[must_use]
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
        }
    }
}
