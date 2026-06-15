//! Core lifecycle scenarios driven against a live Docker daemon.
//!
//! These exercise the manager surface end to end: bringing a stack up,
//! tearing it down, restarting a single resource without disturbing its
//! dependents, streaming real container logs, and gating on a real Docker
//! healthcheck. Each test is `#[ignore]`, skips gracefully when Docker is
//! absent, runs under a collision-free project name, tears itself down
//! through the RAII guard, and waits on lifecycle events instead of sleeping.
//!
//! Run with:
//! `cargo test -p lightshuttle-runtime --test lifecycle_scenarios -- --ignored`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{
    ContainerStatus, DockerRuntime, LifecycleHandle, LifecycleManager, LifecyclePlan, ManagerHandle,
};

/// Generous health deadline: pulling `alpine:3.20` on a cold CI runner can
/// take several seconds before the first healthcheck even runs.
const HEALTH_DEADLINE: Duration = Duration::from_secs(60);

/// Grace window passed to `stop_all`; alpine `sleep` stops well within it.
const STOP_GRACE: Duration = Duration::from_secs(3);

/// Parse `yaml` into a `LifecyclePlan`, panicking with context on failure.
fn plan_from(yaml: &str) -> LifecyclePlan {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    LifecyclePlan::from_manifest(&manifest).expect("plan builds")
}

/// Open a fresh Docker connection used only to observe managed containers
/// (the manager owns the runtime it drives, so observation needs its own).
fn observer() -> DockerRuntime {
    DockerRuntime::connect().expect("Docker daemon reachable")
}

/// `true` when the container is in a live state rather than stopped/exited.
fn is_live(status: &ContainerStatus) -> bool {
    matches!(
        status,
        ContainerStatus::Running | ContainerStatus::Starting | ContainerStatus::Healthy
    )
}

// --- Scenario A: up then down ------------------------------------------------

/// `start_all` brings a two-resource stack live, then `stop_all` stops every
/// container and removes the project network.
///
/// `stop_all` stops containers but does not remove them, and `list_managed`
/// lists with `all(true)`, so "down" is asserted by every managed container
/// having left a live state, not by the list being empty. The RAII guard
/// performs the actual `docker rm -f`.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn up_brings_the_stack_live_then_down_stops_it() {
    if !common::docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = common::unique_project("up");
    let _guard = common::ProjectCleanup::new(project.clone());

    let plan = plan_from(&two_container_stack(&project));
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("stack boots");

    let live = observer()
        .list_managed(&project)
        .await
        .expect("list after start");
    assert_eq!(
        live.len(),
        2,
        "both resources should be managed, got {live:?}"
    );
    assert!(
        live.iter().all(|c| is_live(&c.status)),
        "every resource should be live after start, got {live:?}"
    );

    manager.stop_all(STOP_GRACE).await.expect("stack stops");

    let stopped = observer()
        .list_managed(&project)
        .await
        .expect("list after stop");
    assert!(
        stopped.iter().all(|c| !is_live(&c.status)),
        "no resource should remain live after stop, got {stopped:?}"
    );
}

// --- Scenario B: health gating ----------------------------------------------

/// A resource carrying an explicit Docker healthcheck drives the manager to
/// emit `ResourceHealthy`, which `wait_for_healthy` observes off the broadcast
/// channel with no fixed sleep.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn health_gating_emits_resource_healthy() {
    if !common::docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = common::unique_project("hc");
    let _guard = common::ProjectCleanup::new(project.clone());

    let plan = plan_from(&healthchecked_stack(&project));
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let (manager, mut events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("stack boots");

    common::wait_for_healthy(&mut events, "web", HEALTH_DEADLINE)
        .await
        .expect("web should report healthy from its Docker healthcheck");

    manager.stop_all(STOP_GRACE).await.expect("stack stops");
}

// --- Scenario C: restart one, dependents untouched --------------------------

/// `restart_one` cycles a single resource (new container) while a dependent
/// resource keeps its original container untouched.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn restart_one_recreates_target_and_leaves_dependent_running() {
    if !common::docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = common::unique_project("rst");
    let _guard = common::ProjectCleanup::new(project.clone());

    let plan = plan_from(&dependent_stack(&project));
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let (manager, mut events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("stack boots");
    common::wait_for_healthy(&mut events, "cache", HEALTH_DEADLINE)
        .await
        .expect("cache healthy before restart");

    let api_before = managed_id(&project, "api").await;

    manager.restart_one("cache").await.expect("cache restarts");
    common::wait_for_healthy(&mut events, "cache", HEALTH_DEADLINE)
        .await
        .expect("cache healthy again after restart");

    let api_after = managed_id(&project, "api").await;
    assert_eq!(
        api_before, api_after,
        "the dependent `api` container must survive a restart of `cache`"
    );

    manager.stop_all(STOP_GRACE).await.expect("stack stops");
}

/// Return the container id of `resource` within `project`, panicking when it
/// is not (yet) managed.
async fn managed_id(project: &str, resource: &str) -> String {
    let managed = observer()
        .list_managed(project)
        .await
        .expect("list managed");
    match managed.into_iter().find(|c| c.resource == resource) {
        Some(container) => container.id.to_string(),
        None => panic!("resource `{resource}` should be managed in `{project}`"),
    }
}

// --- Scenario D: real logs ---------------------------------------------------

/// A container that prints a known marker exposes it through
/// `ManagerHandle::logs`, proving log streaming reaches real stdout.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn logs_stream_real_container_output() {
    if !common::docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = common::unique_project("log");
    let _guard = common::ProjectCleanup::new(project.clone());

    let plan = plan_from(&log_marker_stack(&project));
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("printer boots");

    let handle = ManagerHandle::new(Arc::new(manager));
    let mut stream = handle
        .logs("printer", false)
        .await
        .expect("logs stream opens");

    let mut collected = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("log chunk reads");
        collected.extend_from_slice(&chunk.bytes);
    }

    let text = String::from_utf8_lossy(&collected);
    assert!(
        text.contains(LOG_MARKER),
        "log stream should carry the printed marker, got `{text}`"
    );

    handle
        .manager()
        .stop_all(STOP_GRACE)
        .await
        .expect("stack stops");
}

// --- Manifest builders -------------------------------------------------------

/// Marker line emitted by the log scenario's container.
const LOG_MARKER: &str = "LIGHTSHUTTLE_LOG_MARKER";

/// Two plain containers, no dependency, both long-lived.
fn two_container_stack(project: &str) -> String {
    format!(
        r#"
project:
  name: {project}
resources:
  alpha:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
  beta:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
"#
    )
}

/// One container with an always-succeeding healthcheck so it reaches the
/// `Healthy` state quickly.
fn healthchecked_stack(project: &str) -> String {
    format!(
        r#"
project:
  name: {project}
resources:
  web:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
      healthcheck:
        test: ["CMD-SHELL", "true"]
        interval: "1s"
        timeout: "2s"
        retries: 3
        start_period: "1s"
"#
    )
}

/// A healthchecked `cache` and an `api` that depends on it.
fn dependent_stack(project: &str) -> String {
    format!(
        r#"
project:
  name: {project}
resources:
  cache:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
      healthcheck:
        test: ["CMD-SHELL", "true"]
        interval: "1s"
        timeout: "2s"
        retries: 3
        start_period: "1s"
  api:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
      depends_on: [cache]
"#
    )
}

/// A single container that prints a known marker then idles, so the marker is
/// drainable from a non-following log stream.
fn log_marker_stack(project: &str) -> String {
    format!(
        r#"
project:
  name: {project}
resources:
  printer:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo {LOG_MARKER}; sleep 60"]
"#
    )
}
