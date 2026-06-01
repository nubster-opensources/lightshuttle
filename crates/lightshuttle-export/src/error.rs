//! Error type returned by the export pipeline.

use lightshuttle_spec::SpecError;

/// Shorthand alias for `std::result::Result<T, ExportError>`.
pub type Result<T> = std::result::Result<T, ExportError>;

/// Errors raised while lowering a manifest or emitting artifacts.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// Resolving a manifest resource into a container specification
    /// failed during lowering.
    #[error("failed to resolve resource `{resource}`")]
    Spec {
        /// Resource whose resolution failed.
        resource: String,
        /// Underlying specification error.
        #[source]
        source: SpecError,
    },

    /// An emitter could not represent a resource for its target (for
    /// example, a locally built image has no registry reference to put
    /// in a Kubernetes manifest).
    #[error("`{resource}` cannot be exported to {target}: {reason}")]
    Unsupported {
        /// Resource that cannot be represented.
        resource: String,
        /// Target that rejected it.
        target: &'static str,
        /// Why the resource is unsupported for this target.
        reason: String,
    },
}
