//! Locate the `lightshuttle.yml` manifest associated with the current
//! working directory.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

/// File name searched for during upward discovery.
pub const MANIFEST_FILENAME: &str = "lightshuttle.yml";

/// Resolve the manifest path.
///
/// When `override_path` is `Some`, the call returns it verbatim after a
/// quick existence check. Otherwise, the discovery walks from the
/// current working directory up to the filesystem root looking for a
/// `lightshuttle.yml` file. The first match wins, in the spirit of
/// `cargo` looking for `Cargo.toml`.
pub fn resolve_manifest(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        return Err(anyhow!("manifest not found at `{}`", path.display()));
    }

    let cwd = std::env::current_dir().context("failed to read the current directory")?;
    discover_from(&cwd)
}

fn discover_from(start: &Path) -> Result<PathBuf> {
    let mut current: Option<&Path> = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(MANIFEST_FILENAME);
        if candidate.is_file() {
            return Ok(candidate);
        }
        current = dir.parent();
    }
    Err(anyhow!(
        "no `{MANIFEST_FILENAME}` found between `{}` and the filesystem root",
        start.display()
    ))
}
