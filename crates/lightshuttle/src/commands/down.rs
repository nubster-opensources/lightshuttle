//! `lightshuttle down`.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use lightshuttle_runtime::{ContainerRuntime, DockerRuntime};
use tracing::{info, warn};

use super::{ExitOutcome, load_manifest};

/// Stop every container that carries the project's label.
///
/// Does not depend on a running `up`; queries Docker directly by
/// label so it works after a hard kill of the manager.
pub(crate) async fn run(file: &Path, grace: Duration) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let project = &manifest.project.name;

    let runtime = DockerRuntime::connect()?;
    let containers = runtime.list_managed(project).await?;

    if containers.is_empty() {
        info!(project = %project, "no managed containers to stop");
        println!("nothing to stop for project `{project}`");
        return Ok(ExitOutcome::Success);
    }

    let mut had_error = false;
    for managed in containers {
        match runtime.stop(&managed.id, grace).await {
            Ok(()) => {
                info!(resource = %managed.resource, "stopped");
                println!("stopped: {}", managed.resource);
            }
            Err(e) => {
                warn!(resource = %managed.resource, error = %e, "failed to stop");
                eprintln!("failed to stop `{}`: {e}", managed.resource);
                had_error = true;
            }
        }
    }

    Ok(if had_error {
        ExitOutcome::RuntimeError
    } else {
        ExitOutcome::Success
    })
}
