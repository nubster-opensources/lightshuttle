//! Error types for secret loading.

use std::path::PathBuf;

/// Errors that can occur while loading secrets.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The `.env` file path was explicitly provided but does not exist.
    #[error("env file not found: {0}")]
    FileNotFound(PathBuf),

    /// An I/O error occurred while reading the file.
    #[error("failed to read env file {path}: {source}")]
    Io {
        /// Path to the file that caused the error.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A line in the file could not be parsed as `KEY=VALUE`.
    #[error("invalid syntax in {path} at line {line}: {message}")]
    InvalidSyntax {
        /// Path to the file that caused the error.
        path: PathBuf,
        /// 1-based line number.
        line: usize,
        /// Description of the syntax problem.
        message: String,
    },
}
