//! Secret source trait and built-in implementations.

mod env_file;

pub use env_file::EnvFileSource;

use std::collections::HashMap;

use crate::error::SecretError;

/// A source of key-value secret pairs.
///
/// Implementations may read from a file, the system environment, a remote
/// vault, or any other backing store. Each call to [`load`] returns a fresh
/// snapshot; sources are expected to be cheap to call repeatedly.
///
/// [`load`]: SecretSource::load
pub trait SecretSource: Send + Sync {
    /// Load all secrets from this source.
    ///
    /// Returns a map of variable names to their string values.
    fn load(&self) -> Result<HashMap<String, String>, SecretError>;

    /// Human-readable name used in error messages and diagnostics.
    fn source_name(&self) -> &str;
}
