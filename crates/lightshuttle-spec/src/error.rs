//! Error type returned while resolving a manifest resource into a
//! container specification.

/// Shorthand alias for `std::result::Result<T, SpecError>`.
pub type Result<T> = std::result::Result<T, SpecError>;

/// Errors raised while building a [`crate::ContainerSpec`] from a
/// manifest resource declaration.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// The resolved specification is structurally invalid (bad port,
    /// volume, duration or healthcheck declaration).
    #[error("invalid container spec: {0}")]
    InvalidSpec(String),
}
