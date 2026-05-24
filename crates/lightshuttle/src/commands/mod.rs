//! Subcommand implementations.
//!
//! Each module owns a single command and returns an [`ExitOutcome`].
//! The top-level `main` translates the outcome into a POSIX exit code:
//!
//! - 0  success
//! - 1  user error (invalid manifest, missing file, validation failure)
//! - 2  runtime error (Docker unreachable, container start fail)
//! - 130 SIGINT (set by tokio when Ctrl+C arrives)

use std::path::Path;

use anyhow::Result;

pub mod down;
pub mod logs;
pub mod manifest;
pub mod ps;
pub mod up;
pub mod validate;

/// Translates command results into the right exit code.
///
/// User errors (manifest parse failures, missing files) are propagated
/// as `Err` from each command, then mapped to exit code 1 by `main`.
/// This enum carries only the outcomes that succeed at the runtime
/// layer.
#[derive(Debug, Clone, Copy)]
pub enum ExitOutcome {
    Success,
    RuntimeError,
}

impl ExitOutcome {
    /// POSIX exit code for this outcome.
    #[must_use]
    pub fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::RuntimeError => 2,
        }
    }
}

/// Common helper: load and parse the manifest at `path`.
pub fn load_manifest(path: &Path) -> Result<lightshuttle_manifest::Manifest> {
    let yaml = std::fs::read_to_string(path)?;
    Ok(lightshuttle_manifest::Manifest::parse(&yaml)?)
}
