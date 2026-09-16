//! Error type returned by the export pipeline.

use lightshuttle_spec::SpecError;

/// Shorthand `Result` type that pins the error to [`ExportError`].
///
/// Used as the return type of [`crate::lower`] and [`crate::Emitter::emit`].
pub type Result<T> = std::result::Result<T, ExportError>;

/// Errors raised while lowering a manifest or emitting artifacts.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

    /// One or more `${env.NAME}` references have no default and the target
    /// refuses to export without them (Kubernetes; see the design's
    /// rendering table).
    #[error(
        "`{resource}` export to {target} requires environment variable(s) with no default: {}",
        variables.join(", ")
    )]
    UnresolvedVariables {
        /// Resource whose deployment text references the unresolved variables.
        resource: String,
        /// Target that refuses to export without the variables.
        target: &'static str,
        /// Sorted, deduplicated list of variable names with no default.
        variables: Vec<String>,
    },

    /// A `${resources.<name>.<property>}` reference resolves to a sensitive
    /// property (see [`lightshuttle_spec::SENSITIVE_OUTPUTS`]) outside of an
    /// environment variable, where it would otherwise land in clear text.
    #[error(
        "`{resource}` field `{field}` references sensitive property `{reference}` outside of an environment variable"
    )]
    SensitiveReferenceOutsideEnv {
        /// Resource whose deployment text made the reference.
        resource: String,
        /// Field the sensitive reference was found in.
        field: String,
        /// The offending `${resources.<name>.<property>}` reference, rendered verbatim.
        reference: String,
    },

    /// A variable name does not match the shell-safe identifier grammar
    /// `[A-Za-z_][A-Za-z0-9_]*` required by every export target.
    #[error(
        "`{resource}` references invalid variable name `{name}`: must match `[A-Za-z_][A-Za-z0-9_]*`"
    )]
    InvalidVariableName {
        /// Resource whose deployment text references the invalid name.
        resource: String,
        /// The offending variable name.
        name: String,
    },
}
