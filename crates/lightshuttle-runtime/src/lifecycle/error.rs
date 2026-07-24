//! Error types returned by [`crate::LifecycleManager`] and [`crate::LifecyclePlan`].
//!
//! [`LifecycleError`] is the top-level error type for the lifecycle layer. It
//! wraps lower-level [`crate::RuntimeError`] values (from the container
//! runtime) and [`lightshuttle_spec::SpecError`] values (from manifest
//! conversion), and adds lifecycle-specific variants such as dependency cycles,
//! healthcheck timeouts, and missing environment variables.

use std::time::Duration;

use lightshuttle_spec::SpecError;

use crate::error::RuntimeError;

/// Errors raised by the lifecycle layer.
#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    /// The dependency graph contains a cycle.
    #[error("cycle detected in dependency graph: {0}")]
    Cycle(String),

    /// Converting a manifest resource into a [`crate::ContainerSpec`] failed.
    #[error("manifest conversion failed for `{resource}`")]
    SpecBuild {
        /// Resource whose conversion failed.
        resource: String,
        /// Underlying specification error.
        #[source]
        source: SpecError,
    },

    /// A resource failed to start.
    #[error("failed to start resource `{resource}`")]
    Start {
        /// Resource that failed.
        resource: String,
        /// Underlying runtime error.
        #[source]
        source: RuntimeError,
    },

    /// A resource failed to stop cleanly.
    #[error("failed to stop resource `{resource}`")]
    Stop {
        /// Resource that failed.
        resource: String,
        /// Underlying runtime error.
        #[source]
        source: RuntimeError,
    },

    /// A resource never became healthy within the configured timeout.
    #[error("resource `{resource}` healthcheck timed out after {timeout:?}")]
    HealthcheckTimeout {
        /// Resource that did not become healthy.
        resource: String,
        /// Configured timeout.
        timeout: Duration,
    },

    /// A dependency of the resource failed.
    #[error("dependency `{dependency}` for `{resource}` failed: {reason}")]
    DependencyFailed {
        /// Resource whose start was blocked.
        resource: String,
        /// Dependency that failed.
        dependency: String,
        /// Reason reported by the failed dependency.
        reason: String,
    },

    /// A reference targets a resource that does not exist in the plan.
    #[error("resource `{0}` not found in the current plan")]
    ResourceNotFound(String),

    /// A restart was requested while another restart of the same resource
    /// is still running. Restarts are serialized per resource so a fresh
    /// container is never removed by a competing restart.
    #[error("a restart of resource `{resource}` is already in progress")]
    RestartInProgress {
        /// Resource whose restart is already in flight.
        resource: String,
    },

    /// One or more `${env.VAR}` references in the manifest cannot be
    /// resolved because the variables are unset and have no default.
    #[error(
        "missing required environment variable(s): {}",
        names.join(", ")
    )]
    MissingEnvVars {
        /// Sorted, deduplicated list of missing variable names.
        names: Vec<String>,
    },
}
