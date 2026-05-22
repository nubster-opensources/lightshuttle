//! Port mapping representation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Port mapping for a container.
///
/// The short integer form exposes a single container port that the
/// runtime mirrors on the host. The string form supports the full
/// Compose syntax (`"host:container"`, `"bind:host:container"`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum PortMapping {
    /// Short form: just the container port. Host port mirrors it.
    Container(u16),

    /// Full form: `"9090:9090"` or `"127.0.0.1:9090:9090"`.
    Mapping(String),
}
