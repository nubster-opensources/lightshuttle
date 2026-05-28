//! Shared application state injected into axum handlers.
//!
//! Generic over a [`LifecycleHandle`] so the control plane stays free
//! of runtime-specific types. axum requires the state to be `Clone`,
//! so the handle must be cheaply cloneable.

use lightshuttle_runtime::LifecycleHandle;

/// State shared by every route of the control plane.
#[derive(Clone, Debug)]
pub struct ControlState<H>
where
    H: LifecycleHandle + Clone,
{
    /// Project name as declared in the manifest.
    pub project: String,
    /// Lifecycle handle backing the resource endpoints.
    pub handle: H,
}

impl<H> ControlState<H>
where
    H: LifecycleHandle + Clone,
{
    /// Build a new state bound to `project` and `handle`.
    pub fn new(project: impl Into<String>, handle: H) -> Self {
        Self {
            project: project.into(),
            handle,
        }
    }
}
