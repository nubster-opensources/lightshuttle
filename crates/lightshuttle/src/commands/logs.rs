//! `lightshuttle logs`.

use std::path::Path;

use anyhow::{Result, anyhow};
use futures::StreamExt;
use lightshuttle_runtime::{ContainerRuntime, DockerRuntime};

use super::{ExitOutcome, load_manifest};
use crate::output::write_log_chunk;

/// Stream logs of a single resource.
pub(crate) async fn run(file: &Path, resource: &str, follow: bool) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let project = &manifest.project.name;

    let runtime = DockerRuntime::connect()?;
    let containers = runtime.list_managed(project).await?;
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
