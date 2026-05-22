//! Volume configuration for resources that may persist state.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Volume specification.
///
/// `true` requests an auto-named persistent volume, `false` opts out (the
/// data is lost on `down`), and a string supplies an explicit named
/// volume.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum Volume {
    /// Boolean form: enable or disable an auto-named persistent volume.
    Boolean(bool),

    /// String form: explicit named volume.
    Named(String),
}

impl Default for Volume {
    fn default() -> Self {
        Self::Boolean(true)
    }
}
