//! Command override for container and dockerfile resources.
//!
//! [`Command`] is used in the `command` field of [`crate::ContainerConfig`] and
//! [`crate::DockerfileConfig`] to override the image default `CMD`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Override for the image default `CMD`.
///
/// Two forms are accepted in the manifest YAML:
///
/// - A single string is interpreted shell-style, equivalent to wrapping
///   the value in `sh -c "..."`. Convenient for one-liners.
/// - An array of strings is passed directly to the container runtime as
///   an argument vector, giving precise control over quoting and
///   whitespace.
///
/// Either form becomes the container `Cmd`. The image `ENTRYPOINT` is
/// preserved: it is never overridden, so an image declaring
/// `ENTRYPOINT ["/usr/local/bin/app"]` runs that binary with this value
/// appended as its arguments. Against such an image, a startup shim
/// written as `sh -c "..."` is not executed as a command of its own; it
/// reaches the entrypoint binary as positional arguments, which most
/// argument parsers reject. Only images whose entrypoint is a shell, or
/// which declare no entrypoint at all, run this value directly.
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
