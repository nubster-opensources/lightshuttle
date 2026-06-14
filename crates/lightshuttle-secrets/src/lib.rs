#![deny(missing_docs)]

//! Secret sources and `.env` file loading for LightShuttle.
//!
//! This crate provides a pluggable system for loading environment variables and secrets.
//! It forms a leaf layer of LightShuttle: it has no internal dependencies and is used by
//! higher layers to interpolate `${env.VAR}` references in manifests.
//!
//! The core abstraction is the [`SecretSource`] trait, which any backing store can implement
//! to expose key-value pairs. A built-in [`EnvFileSource`] implementation parses `.env` files
//! using POSIX-like syntax (supporting quoted values, comments, and optional `export` prefix).
//!
//! # Example: Load a `.env` file
//!
//! ```
//! use lightshuttle_secrets::EnvFileSource;
//! use std::path::Path;
//!
//! # use tempfile::NamedTempFile;
//! # use std::io::Write as _;
//! # let mut f = NamedTempFile::new().unwrap();
//! # f.write_all(b"API_KEY=secret123\nDB_URL=postgres://localhost/db\n").unwrap();
//! # let path = f.path();
//! let source = EnvFileSource::load(path)?;
//! println!("Loaded {} entries from .env", source.len());
//! # Ok::<(), lightshuttle_secrets::SecretError>(())
//! ```
//!
//! # Example: Load `.env` optionally (default file)
//!
//! ```
//! use lightshuttle_secrets::EnvFileSource;
//!
//! # use tempfile::NamedTempFile;
//! # use std::io::Write as _;
//! # let mut f = NamedTempFile::new().unwrap();
//! # f.write_all(b"KEY=value\n").unwrap();
//! # let path = f.path();
//! // Returns None if the file does not exist (no error)
//! if let Some(source) = EnvFileSource::load_optional(path)? {
//!     println!("Using {} secrets from .env", source.len());
//! } else {
//!     println!("No .env file found; using defaults");
//! }
//! # Ok::<(), lightshuttle_secrets::SecretError>(())
//! ```

pub mod error;
pub mod source;

pub use error::SecretError;
pub use source::{EnvFileSource, SecretSource};
