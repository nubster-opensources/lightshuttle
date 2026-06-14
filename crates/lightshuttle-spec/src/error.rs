//! Error type returned while resolving a manifest resource into a
//! container specification.

/// Shorthand alias for `std::result::Result<T, SpecError>`.
///
/// All fallible operations in this crate return this type.
///
/// # Example
///
/// ```rust
/// use lightshuttle_spec::{Result, SpecError};
///
/// fn check(ok: bool) -> Result<u32> {
///     if ok {
///         Ok(42)
///     } else {
///         Err(SpecError::InvalidSpec("something went wrong".into()))
///     }
/// }
///
/// assert!(check(true).is_ok());
/// assert!(check(false).is_err());
/// ```
pub type Result<T> = std::result::Result<T, SpecError>;

/// Errors raised while building a [`crate::ContainerSpec`] from a
/// manifest resource declaration.
///
/// All variants carry a human-readable description of what is invalid
/// so callers can surface a clear diagnostic to the user.
///
/// # Example
///
/// ```rust
/// use lightshuttle_spec::SpecError;
///
/// let err = SpecError::InvalidSpec("port 99999 out of range".into());
/// assert!(err.to_string().contains("invalid container spec"));
/// ```
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// The resolved specification is structurally invalid (bad port,
    /// volume, duration, or healthcheck declaration).
    ///
    /// The inner `String` contains a description of the specific field
    /// that failed validation.
    #[error("invalid container spec: {0}")]
    InvalidSpec(String),
}
