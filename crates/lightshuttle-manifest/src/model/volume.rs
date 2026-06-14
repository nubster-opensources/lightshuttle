//! Volume configuration for resources that may persist state.
//!
//! [`Volume`] appears in the `volume` field of [`crate::PostgresConfig`] and
//! [`crate::RedisConfig`]. Three values are accepted: `true`, `false`, or an
//! explicit named volume string.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Volume persistence specification for a managed resource.
///
/// Three forms are accepted in the manifest:
///
/// - `true` (default): the runtime provisions an auto-named volume.
/// - `false`: no volume; the container data directory is ephemeral and lost
///   when `lightshuttle down` removes the container.
/// - A string such as `"my-db-data"`: an explicitly named volume,
///   shared across projects or preserved with a predictable name.
///
/// Used in the `volume` field of [`crate::PostgresConfig`] and [`crate::RedisConfig`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum Volume {
    /// Boolean form: `true` enables an auto-named volume; `false` disables
    /// persistence.
    Boolean(bool),

    /// String form: explicit named volume identifier.
    Named(String),
}

impl Default for Volume {
    fn default() -> Self {
        Self::Boolean(true)
    }
}
