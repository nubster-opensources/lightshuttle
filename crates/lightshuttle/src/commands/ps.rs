//! `lightshuttle ps`.

use std::path::Path;

use anyhow::Result;
use lightshuttle_runtime::DockerRuntime;

use super::{ExitOutcome, load_manifest};
use crate::output::format_ps;

/// Print the table of managed containers and their status.
pub(crate) async fn run(file: &Path) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let project = &manifest.project.name;

    let runtime = DockerRuntime::connect()?;
    let containers = runtime.list_managed(project).await?;
    print!("{}", format_ps(&containers));
    Ok(ExitOutcome::Success)
}
