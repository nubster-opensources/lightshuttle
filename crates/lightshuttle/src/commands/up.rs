//! `lightshuttle up`.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use lightshuttle_runtime::{DockerRuntime, LifecycleManager, LifecyclePlan};
use tracing::info;

use super::{ExitOutcome, load_manifest};

/// Boot the stack, supervise it, and stop it cleanly on signal.
pub(crate) async fn run(file: &Path, grace: Duration) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let plan = LifecyclePlan::from_manifest(&manifest)?;
    let runtime = DockerRuntime::connect()?;
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    info!(project = %manifest.project.name, "stack starting");
    manager.run_until_signal(grace).await?;
    info!(project = %manifest.project.name, "stack stopped");
    Ok(ExitOutcome::Success)
}
