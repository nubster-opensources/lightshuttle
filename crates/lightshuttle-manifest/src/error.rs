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
///
/// The enum is `#[non_exhaustive]`: new variants can be added in a patch
/// release without breaking downstream `match` statements, so consumers must
/// keep a wildcard arm.
#[derive(Debug, Error)]
#[non_exhaustive]
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

    /// An `entrypoint` was declared as an empty list. Clearing an image
    /// entrypoint is not supported: the Engine API and Compose disagree on
    /// how to express it, and no use case requires it yet.
    #[error(
        "`entrypoint` on resource `{resource}` is an empty list: remove the field to keep the image entrypoint, or give it an argument vector"
    )]
    EmptyEntrypoint {
        /// The resource whose configuration is in error.
        resource: String,
    },

    /// The same variable was declared as both plain and sensitive.
    #[error(
        "environment variable `{key}` on resource `{resource}` is declared in both `env` and `secrets`"
    )]
    DuplicateEnvironmentKey {
        /// Resource containing the duplicate declaration.
        resource: String,
        /// Environment variable declared twice.
        key: String,
    },

    /// A relative volume `src` escapes the manifest base directory through a
    /// `..` component. Such a mapping is rejected rather than resolved: it
    /// would mount an arbitrary host path into the container.
    #[error(
        "invalid volume path `{mapping}`: a relative source must not escape the manifest directory with `..`"
    )]
    InvalidVolumePath {
        /// The offending `src:target` mapping, verbatim.
        mapping: String,
    },

    /// A relative `working_dir` on a `process` resource escapes the manifest
    /// base directory through a `..` component.
    ///
    /// Refused for the same reason as [`Self::InvalidVolumePath`], and named
    /// separately because it is not a volume: a process's `working_dir` is a
    /// path on the developer's machine, whereas a container's names a
    /// directory inside the image.
    #[error(
        "invalid working directory `{path}` on resource `{resource}`: a relative path must not escape the manifest directory with `..`"
    )]
    InvalidWorkingDirectory {
        /// Resource whose `working_dir` is in error.
        resource: String,
        /// The offending path, verbatim.
        path: String,
    },

    /// A `process` resource declared an empty `command`.
    #[error("`command` on process resource `{resource}` is empty: name the program to run")]
    EmptyProcessCommand {
        /// The resource whose configuration is in error.
        resource: String,
    },

    /// A resource references the address of a `process` that declares no
    /// `port`.
    ///
    /// Refused rather than guessed at runtime. A companion process that
    /// listens on nothing is a legitimate resource, so the port is optional;
    /// what is not possible is rendering an address for something that has
    /// none.
    #[error(
        "resource `{consumer}` references `{property}` of process `{target}`, which declares no `port`: add `port` to `{target}` or drop the reference"
    )]
    ProcessReferenceWithoutPort {
        /// Resource making the reference.
        consumer: String,
        /// Referenced `process` resource.
        target: String,
        /// Property the reference asked for (`host`, `url` or `port`).
        property: String,
    },

    /// A `process` resource depends on a container that publishes no port.
    ///
    /// A process reaches a container through a port published on the host
    /// loopback, so a container publishing nothing offers no address for the
    /// process to use, and waiting for it could only ever be a wait for
    /// nothing reachable.
    #[error(
        "process `{process}` depends on `{container}`, which publishes no port: a process reaches a container through a published host port, so there is no address to hand it"
    )]
    ProcessDependencyWithoutPublishedPort {
        /// The `process` resource declaring the dependency.
        process: String,
        /// The depended-on resource that publishes nothing.
        container: String,
    },
}
