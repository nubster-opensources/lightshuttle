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
/// The enum is `#[non_exhaustive]`: new variants can be added without another
/// breaking change, and callers must carry a wildcard arm. It is marked so at
/// the same time as the first refusal that is not a structural one, which is
/// the moment the choice stops being free: the alternative was to fold every
/// future refusal into [`SpecError::InvalidSpec`] and leave callers matching
/// on message text.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SpecError {
    /// The resolved specification is structurally invalid (bad port,
    /// volume, duration, or healthcheck declaration).
    ///
    /// The inner `String` contains a description of the specific field
    /// that failed validation.
    #[error("invalid container spec: {0}")]
    InvalidSpec(String),

    /// A `process` resource was asked for an address while declaring no
    /// `port`.
    ///
    /// Not a structural error: a companion process that listens on nothing is
    /// a legitimate resource, and it starts perfectly well. What cannot be
    /// done is render an address for something that has none.
    #[error("process `{resource}` declares no `port`, so it exposes no address")]
    ProcessWithoutPort {
        /// The `process` resource that was asked for an address.
        resource: String,
    },

    /// A resource publishing no port on the host was asked for an address by
    /// a native process.
    ///
    /// A process is not attached to the project network: it reaches a
    /// container through a port published on the host loopback. A container
    /// that publishes nothing is therefore reachable by its container
    /// siblings and by nothing else.
    #[error(
        "`{resource}` publishes no port on the host, so a native process has no address to reach it"
    )]
    NoPublishedPort {
        /// The resource that publishes nothing.
        resource: String,
    },
}
