//! Secret source trait and built-in implementations.

mod env_file;

pub use env_file::EnvFileSource;

use std::collections::HashMap;

use crate::error::SecretError;

/// A source of key-value secret pairs.
///
/// Implementations may read from a file, the system environment, a remote vault,
/// or any other backing store. Each call to [`load`] returns a fresh snapshot.
/// Sources are expected to be cheap to call repeatedly; consumers may invoke
/// [`load`] multiple times without penalty.
///
/// This trait is used by higher layers (e.g. `lightshuttle-manifest`) to populate
/// interpolation contexts. Built-in implementations include [`EnvFileSource`].
///
/// # Implementing a custom source
///
/// ```
/// use lightshuttle_secrets::SecretSource;
/// use std::collections::HashMap;
/// use std::sync::Arc;
///
/// struct MyVaultSource {
///     url: String,
/// }
///
/// impl SecretSource for MyVaultSource {
///     fn load(&self) -> Result<HashMap<String, String>, lightshuttle_secrets::SecretError> {
///         // Fetch from a remote vault (hypothetical)
///         Ok([("API_TOKEN".to_string(), "vault-secret".to_string())].into())
///     }
///
///     fn source_name(&self) -> &str {
///         "MyVault"
///     }
/// }
/// ```
///
/// [`load`]: SecretSource::load
/// [`EnvFileSource`]: crate::EnvFileSource
pub trait SecretSource: Send + Sync {
    /// Load all secrets from this source.
    ///
    /// Returns a map of variable names to their string values. The map may be empty
    /// if the source contains no entries. Errors (via [`SecretError`]) indicate that
    /// the source exists but is invalid or inaccessible.
    ///
    /// Callers may invoke this method multiple times and expect idempotent results
    /// (assuming the source does not change between calls).
    fn load(&self) -> Result<HashMap<String, String>, SecretError>;

    /// Human-readable name used in error messages and diagnostics.
    ///
    /// For example: `.env`, `vault://prod`, `environment`, or a file path.
    /// This name should be short and suitable for logging.
    fn source_name(&self) -> &str;
}
