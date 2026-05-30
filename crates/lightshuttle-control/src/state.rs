//! Shared application state injected into axum handlers.
//!
//! Generic over a [`LifecycleHandle`] so the control plane stays free
//! of runtime-specific types. axum requires the state to be `Clone`,
//! so the handle must be cheaply cloneable.

use std::sync::Arc;

use lightshuttle_runtime::LifecycleHandle;

use crate::metrics::Metrics;

/// State shared by every route of the control plane.
#[derive(Clone)]
pub struct ControlState<H>
where
    H: LifecycleHandle + Clone,
{
    /// Project name as declared in the manifest.
    pub project: String,
    /// Lifecycle handle backing the resource endpoints.
    pub handle: H,
    /// Prometheus metrics renderer.
    pub(crate) metrics: Arc<Metrics>,
}

impl<H> ControlState<H>
where
    H: LifecycleHandle + Clone,
{
    /// Build a new state bound to `project` and `handle`, with a
    /// non-installing test metrics handle.
    ///
    /// The attached [`Metrics`] does not install a global recorder, so
    /// the `metrics!` macros write nowhere and `GET /metrics` renders an
    /// empty snapshot. This constructor is meant for tests and embedders
    /// that do not serve metrics. In production, install the recorder
    /// once and use [`Self::with_metrics`] instead.
    pub fn new(project: impl Into<String>, handle: H) -> Self {
        Self {
            project: project.into(),
            handle,
            metrics: Arc::new(Metrics::for_test()),
        }
    }

    /// Build a new state bound to `project`, `handle` and a shared
    /// [`Metrics`] renderer (typically the globally installed one).
    pub fn with_metrics(project: impl Into<String>, handle: H, metrics: Arc<Metrics>) -> Self {
        Self {
            project: project.into(),
            handle,
            metrics,
        }
    }
}
