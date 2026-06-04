//! Subcommand implementations.
//!
//! Each module owns a single command and returns an [`ExitOutcome`].
//! The top-level `main` translates the outcome into a POSIX exit code:
//!
//! - 0  success
//! - 1  user error (invalid manifest, missing file, validation failure)
//! - 2  runtime error (Docker unreachable, container start fail)
//! - 130 SIGINT (set by tokio when Ctrl+C arrives)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lightshuttle_secrets::{EnvFileSource, SecretSource as _};

pub(crate) mod alias;
pub(crate) mod down;
pub(crate) mod export;
pub(crate) mod logs;
pub(crate) mod manifest;
pub(crate) mod ps;
pub(crate) mod restart;
pub(crate) mod secrets;
pub(crate) mod up;
pub(crate) mod validate;

/// Translates command results into the right exit code.
///
/// User errors (manifest parse failures, missing files) are propagated
/// as `Err` from each command, then mapped to exit code 1 by `main`.
/// This enum carries only the outcomes that succeed at the runtime
/// layer.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ExitOutcome {
    Success,
    /// The lifecycle operation completed but the target resource ended
    /// up in a failed state. Mapped to exit code `1`, matching the
    /// `lightshuttle restart` contract.
    LifecycleFailed,
    RuntimeError,
}

impl ExitOutcome {
    /// POSIX exit code for this outcome.
    #[must_use]
    pub(crate) fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::LifecycleFailed => 1,
            Self::RuntimeError => 2,
        }
    }
}

/// Common helper: load and parse the manifest at `path`.
pub(crate) fn load_manifest(path: &Path) -> Result<lightshuttle_manifest::Manifest> {
    let yaml = std::fs::read_to_string(path)?;
    let mut manifest = lightshuttle_manifest::Manifest::parse(&yaml)?;
    manifest.resolve_host_volume_paths(&manifest_base_dir(path));
    Ok(manifest)
}

/// Absolute directory containing the manifest at `path`. Used to resolve
/// relative host volume paths against the manifest location.
fn manifest_base_dir(path: &Path) -> std::path::PathBuf {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| std::path::PathBuf::from("."), Path::to_path_buf);
    std::env::current_dir().map_or_else(|_| dir.clone(), |cwd| cwd.join(&dir))
}

/// Common helper: load environment variables from a `.env` file.
///
/// When `path` is `Some`, the file must exist (an explicit user path such as
/// `--env-file`). When `path` is `None`, the default `.env` in the working
/// directory is loaded if present and silently skipped when absent. Shared by
/// `up` and `secrets check` so both resolve secrets identically.
pub(crate) fn load_env(path: Option<PathBuf>) -> Result<HashMap<String, String>> {
    if let Some(explicit) = path {
        let source = EnvFileSource::load(&explicit)
            .with_context(|| format!("failed to load --env-file {}", explicit.display()))?;
        source
            .load()
            .with_context(|| format!("failed to read env file {}", explicit.display()))
    } else {
        match EnvFileSource::load_optional(".env").context("failed to parse .env")? {
            Some(source) => source.load().context("failed to read .env"),
            None => Ok(HashMap::new()),
        }
    }
}
