//! Tests of [`sweep_project`] against a scripted in-memory double.
//!
//! `sweep_project` exists because `lightshuttle down` never talks to a
//! running `up` supervisor: it discovers a project's containers purely by
//! label. If `up` is still booting the stack, it can create a container
//! right after `down` took its first listing, leaving a container attached
//! to the project network that a single-pass teardown would never see. Every
//! test below drives that race, and the two failure paths (a container that
//! never stops cooperating, a network that refuses to go away), through
//! [`ScriptedRuntime`], never against a real Docker daemon.

#![allow(clippy::must_use_candidate)]

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use lightshuttle_runtime::{
    ContainerId, ContainerRuntime, ContainerSpec, ContainerStatus, LogChunkStream,
    ManagedContainer, ProjectInventory, Result, RuntimeError, SweepFailure, SweepPolicy,
    sweep_project,
};
use tokio::time::Instant;

/// One container tracked by [`ScriptedRuntime`]: its identifier and the
/// resource it belongs to.
#[derive(Clone)]
struct Container {
    id: ContainerId,
    resource: String,
}

/// A scripted supervisor reaction: removing a container for `trigger`
/// resource makes a fresh container for `spawns` resource appear. `repeats`
/// keeps the rule alive after it fires once, modelling a supervisor that
/// keeps recreating the same resource forever instead of reacting only to
/// its predecessor's removal.
#[derive(Clone)]
struct SpawnRule {
    spawns: String,
    repeats: bool,
}

/// Mutable state behind [`ScriptedRuntime`], behind a single mutex so the
/// double stays consistent across the sequential calls `sweep_project`
/// makes.
struct State {
    containers: Vec<Container>,
    list_calls: u32,
    network_teardown_calls: u32,
    always_fail_network_teardown: bool,
    transient_network_teardown_failures: u32,
    transient_list_failures: u32,
    failing_stops: HashSet<String>,
    spawn_rules: HashMap<String, SpawnRule>,
    next_spawned_id: u64,
}

/// In-memory double standing in for a Docker daemon plus a still-booting
/// `lightshuttle up` supervisor. `sweep_project` only ever reaches this type
/// through the narrow [`ContainerRuntime`] and [`ProjectInventory`] traits,
/// exactly like the real `DockerRuntime`.
///
/// Faithfulness: a single-pass implementation of `sweep_project` (list once,
/// stop and remove what it saw, tear the network down immediately) fails
/// `sweep_removes_a_container_created_after_the_first_listing` against this
/// double, because the container spawned by removing `lightshuttle_otel` is
/// still attached to the network when that immediate teardown runs.
struct ScriptedRuntime {
    state: Mutex<State>,
}

/// Builds a [`RuntimeError`] the double can raise without a Docker client:
/// [`RuntimeError::Timeout`] needs no `bollard` error to construct, unlike
/// every other variant tied to a daemon response.
fn scripted_error(operation: &'static str) -> RuntimeError {
    RuntimeError::Timeout {
        operation,
        after: Duration::ZERO,
    }
}

impl ScriptedRuntime {
    /// An empty daemon: no container, no scripted reaction, network
    /// teardown succeeds whenever nothing is attached to it.
    fn new() -> Self {
        Self {
            state: Mutex::new(State {
                containers: Vec::new(),
                list_calls: 0,
                network_teardown_calls: 0,
                always_fail_network_teardown: false,
                transient_network_teardown_failures: 0,
                transient_list_failures: 0,
                failing_stops: HashSet::new(),
                spawn_rules: HashMap::new(),
                next_spawned_id: 0,
            }),
        }
    }

    /// Locks the scripted state, panicking loudly on poisoning rather than
    /// silently producing a wrong report.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("scripted runtime mutex poisoned")
    }

    /// Seeds the daemon with a container for `resource`, present before the
    /// sweep's first listing.
    fn with_initial_container(self, resource: &str) -> Self {
        let id = ContainerId::new(format!("seed-{resource}"));
        self.lock().containers.push(Container {
            id,
            resource: resource.to_owned(),
        });
        self
    }

    /// Configures `stop` to fail every time it is called for `resource`; the
    /// container is still removed afterwards, matching the sweep's contract
    /// that a failed stop does not block removal.
    fn fail_stop_for(self, resource: &str) -> Self {
        self.lock().failing_stops.insert(resource.to_owned());
        self
    }

    /// Configures `teardown_project_network` to fail unconditionally,
    /// modelling a network the daemon refuses to remove for a reason
    /// unrelated to dangling containers.
    fn always_fail_network_teardown(self) -> Self {
        self.lock().always_fail_network_teardown = true;
        self
    }

    /// Configures `teardown_project_network` to fail its first `times` calls
    /// then behave normally, modelling a daemon that is briefly busy
    /// releasing the last endpoint.
    fn fail_network_teardown_times(self, times: u32) -> Self {
        self.lock().transient_network_teardown_failures = times;
        self
    }

    /// Configures `list_managed` to fail its first `times` calls then behave
    /// normally, modelling a daemon that briefly stops answering.
    fn fail_list_times(self, times: u32) -> Self {
        self.lock().transient_list_failures = times;
        self
    }

    /// Scripts a one-shot supervisor reaction: the first time a container
    /// for `trigger` resource is removed, a fresh container for `spawns`
    /// resource appears, modelling `up` starting a dependent once its
    /// predecessor is gone.
    fn spawn_once_after_removal(self, trigger: &str, spawns: &str) -> Self {
        self.lock().spawn_rules.insert(
            trigger.to_owned(),
            SpawnRule {
                spawns: spawns.to_owned(),
                repeats: false,
            },
        );
        self
    }

    /// Scripts an endlessly reappearing container: every time a container
    /// for `resource` is removed, a fresh one for the same resource appears
    /// immediately, modelling a supervisor stuck recreating it (a
    /// crash-loop, or a healthcheck that keeps restarting it).
    fn respawn_forever(self, resource: &str) -> Self {
        self.lock().spawn_rules.insert(
            resource.to_owned(),
            SpawnRule {
                spawns: resource.to_owned(),
                repeats: true,
            },
        );
        self
    }

    /// Number of times `list_managed` was called.
    fn list_calls(&self) -> u32 {
        self.lock().list_calls
    }

    /// Number of times `teardown_project_network` was called.
    fn network_teardown_calls(&self) -> u32 {
        self.lock().network_teardown_calls
    }
}

impl ProjectInventory for ScriptedRuntime {
    async fn list_managed(&self, _project: &str) -> Result<Vec<ManagedContainer>> {
        let mut state = self.lock();
        state.list_calls += 1;
        if state.transient_list_failures > 0 {
            state.transient_list_failures -= 1;
            return Err(scripted_error("scripted transient listing failure"));
        }
        let mut containers: Vec<ManagedContainer> = state
            .containers
            .iter()
            .map(|container| ManagedContainer {
                id: container.id.clone(),
                resource: container.resource.clone(),
                status: ContainerStatus::Running,
            })
            .collect();
        containers.sort_by(|a, b| a.resource.cmp(&b.resource));
        Ok(containers)
    }
}

impl ContainerRuntime for ScriptedRuntime {
    async fn start(&self, _spec: &ContainerSpec) -> Result<ContainerId> {
        Err(scripted_error("start is not used by sweep_project"))
    }

    async fn stop(&self, id: &ContainerId, _grace: Duration) -> Result<()> {
        let state = self.lock();
        let Some(resource) = state
            .containers
            .iter()
            .find(|container| &container.id == id)
            .map(|container| container.resource.clone())
        else {
            return Ok(());
        };
        if state.failing_stops.contains(&resource) {
            return Err(scripted_error("scripted stop failure"));
        }
        Ok(())
    }

    async fn remove(&self, name: &str) -> Result<()> {
        let mut state = self.lock();
        let Some(position) = state
            .containers
            .iter()
            .position(|container| container.id.as_str() == name)
        else {
            return Ok(());
        };
        let removed = state.containers.remove(position);

        if let Some(rule) = state.spawn_rules.get(&removed.resource).cloned() {
            state.next_spawned_id += 1;
            let spawned_id =
                ContainerId::new(format!("spawned-{}-{}", rule.spawns, state.next_spawned_id));
            state.containers.push(Container {
                id: spawned_id,
                resource: rule.spawns.clone(),
            });
            if !rule.repeats {
                state.spawn_rules.remove(&removed.resource);
            }
        }
        Ok(())
    }

    async fn inspect(&self, _id: &ContainerId) -> Result<ContainerStatus> {
        Err(scripted_error("inspect is not used by sweep_project"))
    }

    async fn wait_healthy(&self, _id: &ContainerId, _timeout: Duration) -> Result<()> {
        Err(scripted_error("wait_healthy is not used by sweep_project"))
    }

    async fn logs(&self, _id: &ContainerId, _follow: bool) -> Result<LogChunkStream> {
        Err(scripted_error("logs is not used by sweep_project"))
    }

    async fn ensure_project_network(&self, _project: &str) -> Result<()> {
        Err(scripted_error(
            "ensure_project_network is not used by sweep_project",
        ))
    }

    async fn teardown_project_network(&self, _project: &str) -> Result<()> {
        let mut state = self.lock();
        state.network_teardown_calls += 1;
        if state.always_fail_network_teardown {
            return Err(scripted_error("scripted network teardown failure"));
        }
        if state.transient_network_teardown_failures > 0 {
            state.transient_network_teardown_failures -= 1;
            return Err(scripted_error(
                "scripted transient network teardown failure",
            ));
        }
        if state.containers.is_empty() {
            Ok(())
        } else {
            Err(scripted_error(
                "network teardown fails while a container is still attached",
            ))
        }
    }
}

/// A container created after the sweep's first listing (the still-booting
/// `up` supervisor replaces the collector with `app` once the collector is
/// gone) must not be left behind: the sweep has to relist after a removal
/// and catch it before tearing the network down. A naive single-pass
/// implementation fails this test, see [`ScriptedRuntime`]'s doc comment.
#[tokio::test(start_paused = true)]
async fn sweep_removes_a_container_created_after_the_first_listing() {
    let runtime = ScriptedRuntime::new()
        .with_initial_container("lightshuttle_otel")
        .spawn_once_after_removal("lightshuttle_otel", "app");
    let policy = SweepPolicy::with_grace(Duration::from_secs(1));
    let settle_delay = policy.settle_delay;

    let started_at = Instant::now();
    let report = sweep_project(&runtime, "demo", policy).await;
    let elapsed = started_at.elapsed();
    let sweep_list_calls = runtime.list_calls();

    assert_eq!(
        report.removed_resources,
        vec!["lightshuttle_otel".to_owned(), "app".to_owned()],
        "otel must be removed before the spawned app is caught: {report:?}"
    );
    assert!(
        report.failures.is_empty(),
        "unexpected failures: {report:?}"
    );
    assert!(report.is_clean(), "report should be clean: {report:?}");
    assert_eq!(
        runtime.network_teardown_calls(),
        1,
        "the network teardown must only be attempted once, after app is gone too"
    );
    assert!(
        runtime
            .list_managed("demo")
            .await
            .expect("listing succeeds")
            .is_empty(),
        "no container should remain on the daemon"
    );
    assert!(
        report.passes >= 2,
        "a second pass is needed to catch app: {report:?}"
    );
    assert_eq!(
        sweep_list_calls, report.passes,
        "the sweep must list exactly once per pass"
    );
    assert!(
        elapsed >= settle_delay,
        "the settle delay must be honoured before relisting, elapsed {elapsed:?}"
    );
}

/// No container exists at all, e.g. a previous `down` already cleaned them
/// up but crashed before tearing the network down: the sweep must reclaim
/// the orphaned network on the very first pass, without paying a settle
/// delay that has nothing to wait for.
#[tokio::test(start_paused = true)]
async fn sweep_reclaims_an_orphaned_network_without_settling() {
    let runtime = ScriptedRuntime::new();
    let policy = SweepPolicy::with_grace(Duration::from_secs(1));

    let started_at = Instant::now();
    let report = sweep_project(&runtime, "demo", policy).await;
    let elapsed = started_at.elapsed();

    assert_eq!(
        runtime.network_teardown_calls(),
        1,
        "the network teardown must be attempted exactly once"
    );
    assert_eq!(report.passes, 1, "a single pass suffices: {report:?}");
    assert_eq!(
        elapsed,
        Duration::ZERO,
        "nothing was removed, so no settle delay should be paid"
    );
    assert!(report.is_clean(), "report should be clean: {report:?}");
}

/// A supervisor endlessly recreating `app` (a crash-loop, or a healthcheck
/// that keeps restarting it) must not turn the sweep into an infinite loop:
/// the pass budget bounds the run, and whatever is still there when the
/// budget runs out is reported instead of chased forever.
#[tokio::test(start_paused = true)]
async fn sweep_stops_after_max_passes_when_containers_keep_reappearing() {
    let runtime = ScriptedRuntime::new()
        .with_initial_container("app")
        .respawn_forever("app");
    let mut policy = SweepPolicy::with_grace(Duration::from_secs(1));
    policy.max_passes = NonZeroU32::new(3).expect("3 is nonzero");

    let report = sweep_project(&runtime, "demo", policy).await;

    assert_eq!(
        report.passes, 3,
        "the sweep must stop exactly at the pass budget: {report:?}"
    );
    assert!(
        report.failures.iter().any(|failure| matches!(
            failure,
            SweepFailure::ContainersRemaining { resources }
                if resources.iter().any(|resource| resource == "app")
        )),
        "the report must name app as still remaining: {report:?}"
    );
    assert!(
        !report.is_clean(),
        "a sweep that gives up on a remaining container is not clean: {report:?}"
    );
}

/// A container whose stop fails (it ignores `SIGTERM` and the daemon reports
/// the grace-period kill as an error) must still be removed: one failing
/// step does not block the rest of the teardown.
#[tokio::test(start_paused = true)]
async fn sweep_removes_a_container_whose_stop_failed() {
    let runtime = ScriptedRuntime::new()
        .with_initial_container("app")
        .fail_stop_for("app");
    let policy = SweepPolicy::with_grace(Duration::from_secs(1));

    let report = sweep_project(&runtime, "demo", policy).await;

    assert!(
        report.failures.iter().any(|failure| matches!(
            failure,
            SweepFailure::Stop { resource, .. } if resource == "app"
        )),
        "a Stop failure for app must be recorded: {report:?}"
    );
    assert!(
        report.removed_resources.contains(&"app".to_owned()),
        "app must still be removed despite the failed stop: {report:?}"
    );
    assert!(
        runtime
            .list_managed("demo")
            .await
            .expect("listing succeeds")
            .is_empty(),
        "no container should remain on the daemon"
    );
    assert!(
        !report.is_clean(),
        "a sweep with a recorded failure is not clean: {report:?}"
    );
}

/// The project network refuses to go away for a reason unrelated to
/// dangling containers (a permission issue, a stale endpoint left by another
/// tool, ...): the sweep retries a bounded number of times, then gives up
/// with exactly one failure in the report instead of one per attempt.
#[tokio::test(start_paused = true)]
async fn sweep_reports_a_persistent_network_teardown_failure() {
    let runtime = ScriptedRuntime::new().always_fail_network_teardown();
    let mut policy = SweepPolicy::with_grace(Duration::from_secs(1));
    policy.max_passes = NonZeroU32::new(3).expect("3 is nonzero");

    let report = sweep_project(&runtime, "demo", policy).await;

    let network_teardown_failures = report
        .failures
        .iter()
        .filter(|failure| matches!(failure, SweepFailure::NetworkTeardown { .. }))
        .count();
    assert_eq!(
        network_teardown_failures, 1,
        "exactly one failure must be recorded, not one per retry: {report:?}"
    );
    assert!(
        runtime.network_teardown_calls() <= 3,
        "the teardown must not be retried past max_passes, got {} calls",
        runtime.network_teardown_calls()
    );
    assert!(
        !report.is_clean(),
        "a persistent teardown failure is not clean: {report:?}"
    );
}

/// The daemon rejects the network teardown twice, then accepts it: the sweep
/// must retry on later passes and end clean, not report the transient
/// failures it recovered from.
#[tokio::test(start_paused = true)]
async fn sweep_recovers_from_a_transient_network_teardown_failure() {
    let runtime = ScriptedRuntime::new().fail_network_teardown_times(2);
    let policy = SweepPolicy::with_grace(Duration::from_secs(1));

    let report = sweep_project(&runtime, "demo", policy).await;

    assert_eq!(
        runtime.network_teardown_calls(),
        3,
        "two rejected attempts, then the one that succeeds"
    );
    assert_eq!(report.passes, 3, "one pass per attempt: {report:?}");
    assert!(
        report.is_clean(),
        "recovered failures must not be reported: {report:?}"
    );
}

/// The daemon fails to list containers once, then answers: the sweep must
/// retry the listing, still remove the container and reclaim the network,
/// and end clean.
#[tokio::test(start_paused = true)]
async fn sweep_recovers_from_a_transient_listing_failure() {
    let runtime = ScriptedRuntime::new()
        .with_initial_container("app")
        .fail_list_times(1);
    let policy = SweepPolicy::with_grace(Duration::from_secs(1));

    let report = sweep_project(&runtime, "demo", policy).await;

    assert_eq!(
        report.removed_resources,
        vec!["app".to_owned()],
        "the container must be removed once the listing answers: {report:?}"
    );
    assert_eq!(
        runtime.network_teardown_calls(),
        1,
        "the network is reclaimed once app is gone"
    );
    assert!(
        report.is_clean(),
        "a recovered listing failure must not be reported: {report:?}"
    );
}
