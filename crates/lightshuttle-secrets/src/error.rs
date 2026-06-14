//! Error types for secret loading.

use std::path::PathBuf;

/// Errors that can occur while loading secrets from a source.
///
/// These errors are typically encountered when loading a `.env` file via [`crate::EnvFileSource`].
/// Use pattern matching to distinguish between missing files, I/O errors, and parse errors.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The `.env` file path was explicitly provided but does not exist.
    ///
    /// This error is returned by [`crate::EnvFileSource::load`] when the file is not found.
    /// Use [`crate::EnvFileSource::load_optional`] instead if the file is allowed to be absent.
    #[error("env file not found: {0}")]
    FileNotFound(PathBuf),

    /// An I/O error occurred while reading the file.
    ///
    /// This wraps the underlying filesystem error. Common causes include permission denied,
    /// invalid path, or disk read failures.
    #[error("failed to read env file {path}: {source}")]
    Io {
        /// Path to the file that caused the error.
        path: PathBuf,
        /// Underlying I/O error from the filesystem.
        #[source]
        source: std::io::Error,
    },

    /// A line in the file could not be parsed as `KEY=VALUE`.
    ///
    /// The syntax is described in [`crate::EnvFileSource`]. This error includes the exact
    /// line number (1-based) and a diagnostic message.
    #[error("invalid syntax in {path} at line {line}: {message}")]
    InvalidSyntax {
        /// Path to the file that caused the error.
        path: PathBuf,
        /// 1-based line number where the syntax error occurred.
        line: usize,
        /// Description of the syntax problem (e.g. "expected KEY=VALUE, got `...`").
        message: String,
    },
}
