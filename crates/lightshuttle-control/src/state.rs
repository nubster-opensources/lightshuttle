//! Shared application state injected into every axum handler.
//!
//! [`ControlState`] is generic over a [`LifecycleHandle`] implementation so
//! the control plane remains free of runtime-specific types. Axum requires
//! the state to implement `Clone`, so the handle must be cheaply cloneable
//! (typically via an inner `Arc`).

use std::sync::Arc;

use lightshuttle_runtime::LifecycleHandle;

use crate::metrics::Metrics;

/// Shared state injected into every route of the control plane.
///
/// Constructed once and cloned into the axum router via
/// [`axum::Router::with_state`]. All fields that route handlers need are
/// either `pub` or exposed through the constructors below.
///
/// Use [`ControlState::new`] for tests and embedders that do not need
/// Prometheus metrics, and [`ControlState::with_metrics`] for production
/// use where `GET /metrics` must return live data.
#[derive(Clone)]
pub struct ControlState<H>
where
    H: LifecycleHandle + Clone,
{
    /// Project name as declared in the manifest.
    ///
    /// Shown in the dashboard title and returned by `GET /healthz`.
    pub project: String,
    /// Lifecycle handle backing the resource endpoints.
    ///
    /// Handlers call [`lightshuttle_runtime::LifecycleHandle::list`],
    /// [`lightshuttle_runtime::LifecycleHandle::get`],
    /// [`lightshuttle_runtime::LifecycleHandle::restart`],
    /// [`lightshuttle_runtime::LifecycleHandle::logs`], and
    /// [`lightshuttle_runtime::LifecycleHandle::subscribe_events`] through
    /// this field.
    pub handle: H,
    /// Prometheus metrics renderer.
    pub(crate) metrics: Arc<Metrics>,
}

impl<H> ControlState<H>
where
    H: LifecycleHandle + Clone,
{
    /// Build state with a non-installing [`crate::Metrics`] handle.
    ///
    /// The attached [`crate::Metrics`] does not install a global recorder, so
    /// the `metrics!` macros write nowhere and `GET /metrics` renders an
    /// empty snapshot. This constructor is intended for tests and for embedders
    /// that do not need to serve metrics. In production, install the recorder
    /// once with [`crate::Metrics::install`] and use [`Self::with_metrics`]
    /// to pass the live handle.
    pub fn new(project: impl Into<String>, handle: H) -> Self {
        Self {
            project: project.into(),
            handle,
            metrics: Arc::new(Metrics::for_test()),
        }
    }

    /// Build state bound to an existing [`crate::Metrics`] renderer.
    ///
    /// Use this constructor in production when you have already called
    /// [`crate::Metrics::install`] and wrapped the result in an `Arc`.
    /// `GET /metrics` will then render live counters and gauges.
    ///
    /// The `Arc` is cheap to clone across axum route handlers.
    pub fn with_metrics(project: impl Into<String>, handle: H, metrics: Arc<Metrics>) -> Self {
        Self {
            project: project.into(),
            handle,
            metrics,
        }
    }
}
