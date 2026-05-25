//! `lightshuttle manifest`.

use std::path::Path;

use anyhow::Result;

use super::{ExitOutcome, load_manifest};

/// Dump the resolved manifest as YAML on stdout.
pub(crate) fn run(file: &Path) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let yaml = manifest.to_yaml()?;
    print!("{yaml}");
    Ok(ExitOutcome::Success)
}
