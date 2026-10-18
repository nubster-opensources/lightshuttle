//! `lightshuttle ps`.

use std::path::Path;

use anyhow::Result;
use lightshuttle_runtime::{ContainerStatus, HostRuntime, ManagedContainer, ResourceId};
use tracing::warn;

use super::{ExitOutcome, load_manifest};
use crate::output::format_ps;

/// Print the table of managed resources and their status.
///
/// Both natures are listed. A project that mixes containers and native
/// processes and showed only half of them would answer beside the question:
/// a developer reading a table without their worker concludes it is not
/// running, when it is.
pub(crate) async fn run(file: &Path) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let project = &manifest.project.name;

    let runtime = HostRuntime::connect(&super::manifest_base_dir(file))?;

    // A project made only of `process` resources runs with no daemon at all,
    // so an unreachable one is not necessarily a failure here. It is said
    // rather than hidden, and the processes are listed regardless.
    let mut rows = match runtime.docker() {
        Ok(docker) => docker.list_managed(project).await?,
        Err(error) => {
            warn!(%error, "no container daemon reachable; listing native processes only");
            println!("note: no container daemon reachable, listing native processes only");
            Vec::new()
        }
    };

    rows.extend(
        runtime
            .processes()
            .recorded_processes(project)?
            .into_iter()
            .map(|recorded| ManagedContainer {
                // The process number, which is the identity the operating
                // system answers to and the one a developer would use to look
                // the process up themselves.
                id: ResourceId::new(recorded.pid.to_string()),
                resource: recorded.resource,
                status: if recorded.is_live {
                    ContainerStatus::Running
                } else {
                    // The record outlived its process. Reported as stopped
                    // rather than dropped from the table: an entry that
                    // vanished silently would look like a resource that was
                    // never declared.
                    ContainerStatus::Stopped { exit_code: None }
                },
            }),
    );

    print!("{}", format_ps(&rows));
    Ok(ExitOutcome::Success)
}
