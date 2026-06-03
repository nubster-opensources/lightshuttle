//! Secret sources and `.env` file loading for LightShuttle.
//!
//! Provides a [`SecretSource`] trait and a built-in [`EnvFileSource`] that
//! parses a `.env` file into a key-value map. The map can then be fed into
//! `lightshuttle-manifest`'s `InterpolationContext::with_env` to resolve
//! `${env.VAR}` references in manifests.

pub mod error;
pub mod source;

pub use error::SecretError;
pub use source::{EnvFileSource, SecretSource};
