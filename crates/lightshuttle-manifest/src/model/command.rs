//! Command override for container and dockerfile resources.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Entry point override.
///
/// The short string form is convenient for one-liners; the list form is
/// required when arguments need precise quoting.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum Command {
    /// Single command line, shell-style.
    Single(String),

    /// Explicit argument list.
    Args(Vec<String>),
}
