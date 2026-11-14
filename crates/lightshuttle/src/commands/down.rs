//! `lightshuttle down`.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use lightshuttle_runtime::{HostRuntime, SweepFailure, SweepPolicy, SweepReport, sweep_project};
use tracing::{info, warn};

use super::{ExitOutcome, load_manifest};

/// Stop and remove every container that carries the project's label, then
/// tear down the project network.
///
/// Does not depend on a running `up`; queries Docker directly by label so it
/// works after a hard kill of the manager. The teardown sweeps the project
/// (see [`sweep_project`]) rather than listing once: an `up` that is still
/// starting the stack can create a container right after the first listing,
/// and a single pass would leave it, and the network it is attached to,
/// behind. Containers are removed (not merely stopped) so they release their
/// network endpoints. Named volumes are preserved throughout.
pub(crate) async fn run(file: &Path, grace: Duration) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let project = &manifest.project.name;

    let runtime = HostRuntime::connect(&super::manifest_base_dir(file))?;
    let report = sweep_project(&runtime, project, SweepPolicy::with_grace(grace)).await;

    render_report(project, &report);

    // Containers are found by asking the daemon what it holds. Nothing plays
    // that role for a native process: under Unix a process group outlives the
    // death of its leader, so a supervisor that was killed leaves a server
    // still holding its port. The registry written beside the manifest is what
    // lets this `down`, run from any terminal, reclaim them.
    let skipped = runtime.processes().reclaim_project(project, grace).await?;
    render_skipped_records(project, &skipped);

    Ok(if report.is_clean() {
        ExitOutcome::Success
    } else {
        ExitOutcome::RuntimeError
    })
}

/// Reports the registry entries that were passed over rather than acted on.
///
/// An entry is skipped when its process number no longer names the process it
/// was recorded for: the process ended, or the operating system handed the
/// number to something else. Saying so matters more than it looks. The
/// alternative to skipping is killing a stranger's process, so a silent skip
/// would hide the one decision that keeps `down` from being dangerous.
fn render_skipped_records(project: &str, skipped: &[String]) {
    for resource in skipped {
        warn!(
            project,
            resource, "process record was stale and was left alone"
        );
        println!(
            "skipped: `{resource}` was recorded for project `{project}` but its process is gone"
        );
    }
}

/// Prints and logs the outcome of a sweep in the same spirit as the previous
/// single-pass `down`: a `stopped: <resource>` line per stopped container, a
/// dedicated message when nothing was left to do, and one line per failure.
fn render_report(project: &str, report: &SweepReport) {
    for resource in &report.stopped_resources {
        info!(resource, "stopped");
        println!("stopped: {resource}");
    }

    for resource in &report.removed_resources {
        info!(resource, "removed");
    }

    if report.stopped_resources.is_empty()
        && report.removed_resources.is_empty()
        && report.is_clean()
    {
        info!(project, "no managed containers to stop");
        println!("nothing to stop for project `{project}`");
    }

    for failure in &report.failures {
        render_failure(project, report.passes, failure);
    }
}

/// Prints and logs a single sweep failure, matching the wording the previous
/// single-pass `down` used for the failures it could already report.
fn render_failure(project: &str, passes: u32, failure: &SweepFailure) {
    match failure {
        SweepFailure::Stop { resource, source } => {
            warn!(resource, error = %source, "failed to stop");
            eprintln!("failed to stop `{resource}`: {source}");
        }
        SweepFailure::Remove { resource, source } => {
            warn!(resource, error = %source, "failed to remove");
            eprintln!("failed to remove `{resource}`: {source}");
        }
        SweepFailure::NetworkTeardown { source } => {
            warn!(project, error = %source, "failed to remove project network");
            eprintln!("failed to remove network for `{project}`: {source}");
        }
        SweepFailure::List { source } => {
            warn!(project, error = %source, "failed to list containers");
            eprintln!("failed to list containers for `{project}`: {source}");
        }
        SweepFailure::ContainersRemaining { resources } => {
            warn!(project, passes, remaining = ?resources, "containers still present");
            eprintln!(
                "containers still present for `{project}` after {passes} passes: {}",
                resources.join(", ")
            );
        }
        _ => {
            warn!(project, "sweep reported an unrecognised failure");
            eprintln!("sweep failed for `{project}`");
        }
    }
}
