//! Error type returned by the export pipeline.

use lightshuttle_spec::SpecError;

/// Shorthand `Result` type that pins the error to [`ExportError`].
///
/// Used as the return type of [`crate::lower`] and [`crate::Emitter::emit`].
pub type Result<T> = std::result::Result<T, ExportError>;

/// One edge from an exported service to a dependency the same export
/// excludes, as reported by [`ExportError::DisabledDependencies`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct DisabledDependency {
    /// Exported service whose `depends_on` names the dependency.
    pub resource: String,
    /// Dependency disabled for the target.
    pub dependency: String,
}

impl DisabledDependency {
    /// Builds one dangling edge from the exported service and the
    /// dependency it names.
    #[must_use]
    pub fn new(resource: impl Into<String>, dependency: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            dependency: dependency.into(),
        }
    }
}

impl std::fmt::Display for DisabledDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` depends on `{}`", self.resource, self.dependency)
    }
}

/// Joins dangling edges for the [`ExportError::DisabledDependencies`]
/// message, in the order the refusal collected them.
fn join_dependencies(dependencies: &[DisabledDependency]) -> String {
    dependencies
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<String>>()
        .join(", ")
}

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

    /// One or more exported services depend on a resource disabled for the
    /// same target, so the output would reference a service it does not
    /// define.
    #[error(
        "{target} export would reference disabled resources: {}",
        join_dependencies(dependencies)
    )]
    DisabledDependencies {
        /// Target that refuses the export.
        target: &'static str,
        /// Every dangling edge, sorted by resource then dependency.
        dependencies: Vec<DisabledDependency>,
    },
}
