//! Error type returned by every fallible operation of this crate.
//!
//! All public functions that can fail return `Result<T>`, which is an alias
//! for `std::result::Result<T, ManifestError>`. The variants of
//! [`ManifestError`] are designed to carry enough context for a CLI to
//! produce a human-readable diagnostic without further inspection.

use thiserror::Error;

/// Shorthand for `std::result::Result<T, ManifestError>`.
///
/// Every fallible function in this crate returns this type. Import it as
/// `use lightshuttle_manifest::Result` to avoid the qualification.
pub type Result<T> = std::result::Result<T, ManifestError>;

/// Errors raised while parsing, validating or interpolating a manifest.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// The YAML payload could not be parsed at the syntactic level.
    #[error("failed to parse YAML")]
    Yaml(#[from] serde_norway::Error),

    /// A name (project, resource, database) does not match the expected
    /// pattern.
    #[error("invalid name `{name}`: must match `{pattern}`")]
    InvalidName {
        /// The offending name.
        name: String,
        /// The regular expression that the name failed to match.
        pattern: &'static str,
    },

    /// A cycle was detected in the dependency graph.
    #[error("dependency cycle detected: {0}")]
    Cycle(String),

    /// A reference (`depends_on` entry or interpolation target) names a
    /// resource that does not exist in the manifest.
    #[error("unknown resource reference: {0}")]
    UnknownResource(String),

    /// A `${{resources.x.y}}` reference uses a property that is unknown for
    /// the targeted kind.
    #[error("unknown property `{property}` on resource `{resource}` of kind `{kind}`")]
    UnknownProperty {
        /// The resource whose configuration is in error.
        resource: String,
        /// The unknown property name.
        property: String,
        /// The resource kind name as seen in the manifest.
        kind: &'static str,
    },

    /// An environment variable referenced in an interpolation has no value
    /// at lookup time and no default was supplied.
    #[error("environment variable `{0}` is not set and no default was provided")]
    EnvUnset(String),

    /// A `${{...}}` form is syntactically invalid (unterminated, malformed,
    /// or uses an unknown scheme).
    #[error("invalid interpolation: {0}")]
    InvalidInterpolation(String),

    /// A `${...}` interpolation nests deeper than the engine allows,
    /// guarding against pathological or unbounded manifests.
    #[error("interpolation nested deeper than the limit of {limit}: `{context}`")]
    InterpolationTooDeep {
        /// The maximum nesting depth the engine accepts.
        limit: usize,
        /// The offending interpolation string.
        context: String,
    },

    /// A duration string (healthcheck interval, timeout, start period) is
    /// malformed.
    #[error("invalid duration `{0}`: expected a value like `5s`, `200ms`, `2m`")]
    InvalidDuration(String),

    /// A field that is required in the current context is absent.
    #[error("missing required field `{field}` on resource `{resource}`")]
    MissingField {
        /// The resource whose configuration is in error.
        resource: String,
        /// The name of the missing field.
        field: &'static str,
    },

    /// The `dashboard.port` value is out of the allowed range.
    #[error("invalid dashboard port `{port}`: must be in the range 1..=65535")]
    InvalidDashboardPort {
        /// The offending port value.
        port: u16,
    },
}
