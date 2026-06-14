//! Command override for container and dockerfile resources.
//!
//! [`Command`] is used in the `command` field of [`crate::ContainerConfig`] and
//! [`crate::DockerfileConfig`] to override the image entrypoint.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Container entrypoint override.
///
/// Two forms are accepted in the manifest YAML:
///
/// - A single string is interpreted shell-style, equivalent to wrapping
///   the value in `sh -c "..."`. Convenient for one-liners.
/// - An array of strings is passed directly to the container runtime as
///   an argument vector, giving precise control over quoting and
///   whitespace.
///
/// Used in the `command` field of [`crate::ContainerConfig`] and
/// [`crate::DockerfileConfig`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum Command {
    /// Shell-style one-liner, e.g. `"./start.sh --port 8080"`.
    Single(String),

    /// Explicit argument vector, e.g. `["./start.sh", "--port", "8080"]`.
    Args(Vec<String>),
}
