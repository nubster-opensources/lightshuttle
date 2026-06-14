//! Port mapping representation.
//!
//! [`PortMapping`] appears in the `ports` field of [`crate::ContainerConfig`] and
//! [`crate::DockerfileConfig`]. Two forms are accepted: a bare integer for the
//! short form, or a full `"host:container"` string.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Port mapping for a container resource.
///
/// Two forms are accepted in the manifest:
///
/// - Integer short form: the runtime mirrors the container port on an
///   identical host port. Example: `8080` maps `0.0.0.0:8080 -> 8080`.
/// - String full form: supports the `"host:container"` and
///   `"bind_addr:host:container"` syntaxes understood by the container
///   runtime. Example: `"127.0.0.1:9090:9090"`.
///
/// Used in the `ports` field of [`crate::ContainerConfig`] and
/// [`crate::DockerfileConfig`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum PortMapping {
    /// Short form: just the container port number. The runtime mirrors it
    /// on the same host port.
    Container(u16),

    /// Full form: `"host:container"` or `"bind_addr:host:container"`.
    Mapping(String),
}
