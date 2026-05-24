//! `lightshuttle validate`.

use std::path::Path;

use anyhow::Result;
use tracing::info;

use super::{ExitOutcome, load_manifest};

/// Run the manifest validation pass.
///
/// `strict` upgrades warnings to errors. The current parser already
/// rejects every condition we would warn about, so `strict` has no
/// observable effect yet but is kept on the API surface so it stays
/// available the day a soft warning is introduced.
pub fn run(file: &Path, _strict: bool) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    info!(
        resources = manifest.resources.len(),
        project = %manifest.project.name,
        "manifest is valid"
    );
    println!(
        "ok: project `{project}` with {count} resource(s)",
        project = manifest.project.name,
        count = manifest.resources.len(),
    );
    Ok(ExitOutcome::Success)
}
