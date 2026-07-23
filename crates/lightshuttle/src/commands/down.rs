//! `lightshuttle down`.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use lightshuttle_runtime::{ContainerRuntime, DockerRuntime};
use tracing::{info, warn};

use super::{ExitOutcome, load_manifest};

/// Stop and remove every container that carries the project's label, then
/// tear down the project network.
///
/// Does not depend on a running `up`; queries Docker directly by
/// label so it works after a hard kill of the manager. Containers are removed
/// (not merely stopped) so they release their network endpoints, and the
/// project network teardown runs even when no container is discovered, which
/// reclaims a network orphaned by a hard-killed manager. Named volumes are
/// preserved throughout.
pub(crate) async fn run(file: &Path, grace: Duration) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let project = &manifest.project.name;

    let runtime = DockerRuntime::connect()?;
    let containers = runtime.list_managed(project).await?;

    let mut had_error = false;
    if containers.is_empty() {
        info!(project = %project, "no managed containers to stop");
        println!("nothing to stop for project `{project}`");
    } else {
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

            // Remove the container (force) even if the stop failed, so it can
            // no longer hold an endpoint on the project network. Docker accepts
            // the container id in place of its name.
            if let Err(e) = runtime.remove(managed.id.as_str()).await {
                warn!(resource = %managed.resource, error = %e, "failed to remove");
                eprintln!("failed to remove `{}`: {e}", managed.resource);
                had_error = true;
            } else {
                info!(resource = %managed.resource, "removed");
            }
        }
    }

    // Always tear down the project network, even with no containers: a manager
    // killed hard can leave the network behind with every container already
    // gone. The teardown is idempotent (a missing network is not an error) and
    // only removes a network this project owns.
    if let Err(e) = runtime.teardown_project_network(project).await {
        warn!(project = %project, error = %e, "failed to remove project network");
        eprintln!("failed to remove network for `{project}`: {e}");
        had_error = true;
    }

    Ok(if had_error {
        ExitOutcome::RuntimeError
    } else {
        ExitOutcome::Success
    })
}
