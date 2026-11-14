//! `lightshuttle logs`.

use std::path::Path;

use anyhow::{Result, anyhow};
use futures::StreamExt;
use lightshuttle_manifest::ResourceKind;
use lightshuttle_runtime::{HostRuntime, ResourceRuntime};

use super::{ExitOutcome, load_manifest};
use crate::output::write_log_chunk;

/// Stream logs of a single resource.
pub(crate) async fn run(file: &Path, resource: &str, follow: bool) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let project = &manifest.project.name;

    if matches!(
        manifest.resources.get(resource),
        Some(ResourceKind::Process(_))
    ) {
        return Err(process_logs_unavailable(project, resource));
    }

    let runtime = HostRuntime::connect(&super::manifest_base_dir(file))?;
    let containers = runtime.docker()?.list_managed(project).await?;
    let target = containers
        .into_iter()
        .find(|c| c.resource == resource)
        .ok_or_else(|| anyhow!("resource `{resource}` is not running for project `{project}`"))?;

    let mut stream = runtime.logs(&target.id, follow).await?;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => write_log_chunk(&chunk),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(ExitOutcome::Success)
}

/// Explains why a `process` resource has no logs to stream from here.
///
/// A container's output is held by the daemon, which hands it back to whoever
/// asks. Nothing plays that role for a native process, so the supervisor keeps
/// it itself, in memory and within a bound, and never on disk: what a program
/// prints is whatever it decides to print, credentials included, and a file
/// nobody asked for is a file nobody thinks to delete.
///
/// The consequence is accepted rather than worked around: the output lives in
/// the memory of the `up` that started the process, so it is reachable from
/// that terminal and from the dashboard it serves, and from nowhere else. The
/// refusal names both, because a command that failed without saying where to
/// look would be the same limitation with none of the help.
fn process_logs_unavailable(project: &str, resource: &str) -> anyhow::Error {
    anyhow!(
        "`{resource}` is a `process` resource, and its output is held in memory by the \
         `lightshuttle up` that started it, never written to disk. Read it in that terminal, \
         or on the dashboard it serves for project `{project}`."
    )
}
