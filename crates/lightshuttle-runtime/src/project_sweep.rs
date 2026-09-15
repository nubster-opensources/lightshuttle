//! Teardown of every container a project owns, independent of a running
//! supervisor.
//!
//! `lightshuttle down` does not talk to the `up` process: it discovers the
//! project's containers by label. A supervisor that is still booting the
//! stack can create a container after a single listing, which then keeps an
//! endpoint on the project network and makes the network teardown fail.
//! [`sweep_project`] therefore repeats list, stop and remove until a listing
//! comes back empty, within a bounded number of passes, and only then tears
//! the network down.

use std::future::Future;
use std::num::NonZeroU32;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::docker::ManagedContainer;
use crate::error::{Result, RuntimeError};
use crate::runtime::ContainerRuntime;

/// Default upper bound on the number of sweep passes.
const DEFAULT_MAX_PASSES: NonZeroU32 = NonZeroU32::MIN.saturating_add(4);

/// Default pause after a pass that removed at least one container, leaving a
/// still-booting supervisor time to reveal its next container.
const DEFAULT_SETTLE_DELAY: Duration = Duration::from_millis(500);

/// Lists every container a project owns, running or stopped.
///
/// Kept apart from [`ContainerRuntime`], which only exposes what the
/// lifecycle manager needs: listing by project is a teardown concern.
pub trait ProjectInventory: Send + Sync {
    /// Every container labelled as belonging to `project`.
    fn list_managed(
        &self,
        project: &str,
    ) -> impl Future<Output = Result<Vec<ManagedContainer>>> + Send;
}

/// Bounds and pacing of a [`sweep_project`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepPolicy {
    /// `SIGTERM`-to-`SIGKILL` grace window given to each container.
    pub grace: Duration,
    /// Maximum number of list, stop and remove passes before giving up.
    pub max_passes: NonZeroU32,
    /// Pause after a pass that removed at least one container.
    pub settle_delay: Duration,
}

impl SweepPolicy {
    /// Policy with the given grace window and the default bounds.
    #[must_use]
    pub fn with_grace(grace: Duration) -> Self {
        Self {
            grace,
            max_passes: DEFAULT_MAX_PASSES,
            settle_delay: DEFAULT_SETTLE_DELAY,
        }
    }
}

/// Outcome of a [`sweep_project`] run.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct SweepReport {
    /// Resources whose container was stopped, in the order they were stopped.
    pub stopped_resources: Vec<String>,
    /// Resources whose container was removed, in the order they were removed.
    pub removed_resources: Vec<String>,
    /// Every failure met during the sweep, in the order it happened.
    pub failures: Vec<SweepFailure>,
    /// Number of passes performed.
    pub passes: u32,
}

impl SweepReport {
    /// `true` when the sweep met no failure: no container and no project
    /// network are left behind.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}

/// One failure met during a [`sweep_project`] run.
#[derive(Debug)]
#[non_exhaustive]
pub enum SweepFailure {
    /// A container did not stop; its removal is still attempted.
    Stop {
        /// Resource the container belongs to.
        resource: String,
        /// Underlying runtime error.
        source: RuntimeError,
    },
    /// A container could not be removed.
    Remove {
        /// Resource the container belongs to.
        resource: String,
        /// Underlying runtime error.
        source: RuntimeError,
    },
    /// The project's containers could not be listed.
    List {
        /// Underlying runtime error.
        source: RuntimeError,
    },
    /// The project network could not be removed after the last pass.
    NetworkTeardown {
        /// Underlying runtime error.
        source: RuntimeError,
    },
    /// Containers were still present when the pass budget ran out.
    ContainersRemaining {
        /// Resources whose container was still listed.
        resources: Vec<String>,
    },
}

/// Stop and remove every container of `project` until a listing comes back
/// empty, then tear down the project network.
///
/// A pass lists the project's containers, stops each one and removes it,
/// attempting the removal even when the stop failed. After a pass that
/// removed at least one container the sweep pauses for
/// [`SweepPolicy::settle_delay`] and lists again, so a container created by a
/// supervisor that is still starting resources is caught. An empty listing
/// triggers the network teardown without any pause. The sweep never runs more
/// than [`SweepPolicy::max_passes`] passes; whatever remains is reported rather
/// than retried forever. Named volumes are preserved.
///
/// Failures never abort the sweep: they are collected in the returned
/// [`SweepReport`].
pub async fn sweep_project<R>(runtime: &R, project: &str, policy: SweepPolicy) -> SweepReport
where
    R: ContainerRuntime + ProjectInventory,
{
    let max_passes = policy.max_passes.get();
    let mut report = SweepReport::default();
    let mut passes = 0_u32;

    loop {
        passes += 1;
        debug!(project, pass = passes, "listing project containers");
        let listing = match runtime.list_managed(project).await {
            Ok(listing) => listing,
            Err(source) => {
                if passes < max_passes {
                    tokio::time::sleep(policy.settle_delay).await;
                    continue;
                }
                warn!(project, "giving up listing containers after last pass");
                report.failures.push(SweepFailure::List { source });
                break;
            }
        };

        if listing.is_empty() {
            match runtime.teardown_project_network(project).await {
                Ok(()) => {
                    info!(project, passes, "project network reclaimed");
                    break;
                }
                Err(source) => {
                    if passes < max_passes {
                        tokio::time::sleep(policy.settle_delay).await;
                        continue;
                    }
                    warn!(project, "network teardown still failing at last pass");
                    report
                        .failures
                        .push(SweepFailure::NetworkTeardown { source });
                    break;
                }
            }
        }

        stop_and_remove_pass(runtime, &listing, policy.grace, &mut report).await;

        if passes == max_passes {
            report_remaining_state(runtime, project, &mut report).await;
            break;
        }

        tokio::time::sleep(policy.settle_delay).await;
    }

    report.passes = passes;
    report
}

/// Stops and force-removes every container of one listing, recording a
/// [`SweepFailure`] per failing step without aborting the rest of the pass.
async fn stop_and_remove_pass<R>(
    runtime: &R,
    listing: &[ManagedContainer],
    grace: Duration,
    report: &mut SweepReport,
) where
    R: ContainerRuntime,
{
    for container in listing {
        match runtime.stop(&container.id, grace).await {
            Ok(()) => report.stopped_resources.push(container.resource.clone()),
            Err(source) => report.failures.push(SweepFailure::Stop {
                resource: container.resource.clone(),
                source,
            }),
        }

        match runtime.remove(container.id.as_str()).await {
            Ok(()) => report.removed_resources.push(container.resource.clone()),
            Err(source) => report.failures.push(SweepFailure::Remove {
                resource: container.resource.clone(),
                source,
            }),
        }
    }
}

/// Reports what is still left once the pass budget is exhausted: one extra
/// listing (not counted as a pass) tells whether containers remain or the
/// network can still be reclaimed.
async fn report_remaining_state<R>(runtime: &R, project: &str, report: &mut SweepReport)
where
    R: ContainerRuntime + ProjectInventory,
{
    match runtime.list_managed(project).await {
        Ok(remaining) if remaining.is_empty() => {
            if let Err(source) = runtime.teardown_project_network(project).await {
                report
                    .failures
                    .push(SweepFailure::NetworkTeardown { source });
            }
        }
        Ok(remaining) => {
            let resources = remaining.into_iter().map(|c| c.resource).collect();
            report
                .failures
                .push(SweepFailure::ContainersRemaining { resources });
        }
        Err(source) => report.failures.push(SweepFailure::List { source }),
    }
}
