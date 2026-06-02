//! `lightshuttle export`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lightshuttle_export::{ComposeEmitter, Emitter, HelmEmitter, KubernetesEmitter, Target, lower};
use tracing::info;

use super::{ExitOutcome, load_manifest};
use crate::cli::ExportTarget;

/// Generate deployment artifacts for `target` from the manifest at
/// `file`, writing them under `output` (or `./export/<target>`).
pub(crate) fn run(
    file: &Path,
    target: ExportTarget,
    output: Option<PathBuf>,
    force: bool,
) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let model = lower(&manifest).context("failed to lower the manifest for export")?;

    let emitter = emitter_for(target);
    let artifacts = emitter
        .emit(&model)
        .with_context(|| format!("failed to emit {} artifacts", emitter.target()))?;

    let dir = output.unwrap_or_else(|| default_output_dir(emitter.target()));
    if dir_is_non_empty(&dir) && !force {
        bail!(
            "output directory `{}` is not empty; pass --force to overwrite",
            dir.display()
        );
    }

    for artifact in &artifacts.files {
        let path = dir.join(&artifact.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create `{}`", parent.display()))?;
        }
        std::fs::write(&path, &artifact.contents)
            .with_context(|| format!("failed to write `{}`", path.display()))?;
    }

    info!(
        target = %emitter.target(),
        files = artifacts.files.len(),
        dir = %dir.display(),
        "export complete"
    );
    println!(
        "ok: wrote {count} file(s) for `{target}` to {dir}",
        count = artifacts.files.len(),
        target = emitter.target(),
        dir = dir.display(),
    );
    Ok(ExitOutcome::Success)
}

/// Select the emitter for `target`.
fn emitter_for(target: ExportTarget) -> Box<dyn Emitter> {
    match target {
        ExportTarget::Compose => Box::new(ComposeEmitter),
        ExportTarget::Kubernetes => Box::new(KubernetesEmitter),
        ExportTarget::Helm => Box::new(HelmEmitter),
    }
}

/// Default output directory: `./export/<target>`.
fn default_output_dir(target: Target) -> PathBuf {
    Path::new("export").join(target.label())
}

/// Whether `dir` exists and contains at least one entry.
fn dir_is_non_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some())
}
