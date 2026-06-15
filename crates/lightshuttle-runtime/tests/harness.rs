//! Tests for the shared integration harness in `common`.
//!
//! The pure helpers (`unique_project`, `wait_for_healthy`) run as ordinary
//! tests with no Docker. The cleanup guard is exercised against a real
//! daemon behind `#[ignore]`.

mod common;

use std::collections::HashMap;
use std::time::Duration;

use lightshuttle_runtime::{
    ContainerRuntime, ContainerSpec, DockerRuntime, ImageSource, LifecycleEvent,
};
use tokio::sync::broadcast;

#[test]
fn unique_project_names_are_distinct() {
    let first = common::unique_project("alpha");
    let second = common::unique_project("alpha");
    assert_ne!(first, second, "each call must produce a fresh project name");
}

#[test]
fn unique_project_names_are_dns_safe() {
    let name = common::unique_project("alpha");
    assert!(
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "project name must be a valid DNS label, got `{name}`"
    );
}

#[tokio::test]
async fn wait_for_healthy_returns_when_resource_becomes_healthy() {
    let (tx, mut rx) = broadcast::channel(16);
    tx.send(LifecycleEvent::StackStarted)
        .expect("send stack started");
    tx.send(LifecycleEvent::ResourceHealthy {
        name: "api".to_owned(),
    })
    .expect("send resource healthy");

    let result = common::wait_for_healthy(&mut rx, "api", Duration::from_secs(1)).await;

    assert!(
        result.is_ok(),
        "should resolve once the resource is healthy, got {result:?}"
    );
}

#[tokio::test]
async fn wait_for_healthy_fails_fast_when_resource_fails() {
    let (tx, mut rx) = broadcast::channel(16);
    tx.send(LifecycleEvent::ResourceFailed {
        name: "api".to_owned(),
        error: "boom".to_owned(),
    })
    .expect("send resource failed");

    let result = common::wait_for_healthy(&mut rx, "api", Duration::from_secs(1)).await;

    let error = result.expect_err("should error when the resource fails");
    assert!(
        error.contains("boom"),
        "error should carry the underlying cause, got `{error}`"
    );
}

#[tokio::test]
async fn wait_for_healthy_times_out_without_events() {
    // Keep the sender alive so the channel stays open and the timeout path,
    // not the closed-channel path, is what ends the wait.
    let (_tx, mut rx) = broadcast::channel::<LifecycleEvent>(16);

    let result = common::wait_for_healthy(&mut rx, "api", Duration::from_millis(50)).await;

    assert!(
        result.is_err(),
        "should time out when no healthy event ever arrives"
    );
}

fn probe_spec(project: &str) -> ContainerSpec {
    ContainerSpec {
        name: format!("{project}-probe"),
        project: project.to_owned(),
        resource: "probe".to_owned(),
        image: ImageSource::Pull("alpine:3.20".to_owned()),
        env: HashMap::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        command: Some(vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "sleep 30".to_owned(),
        ]),
        healthcheck: None,
        working_dir: None,
    }
}

#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn project_cleanup_removes_managed_containers() {
    if !common::docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = common::unique_project("cleanup");
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");

    {
        let _guard = common::ProjectCleanup::new(project.clone());
        let _id = runtime
            .start(&probe_spec(&project))
            .await
            .expect("probe container starts");

        let managed = runtime
            .list_managed(&project)
            .await
            .expect("list managed containers");
        assert_eq!(
            managed.len(),
            1,
            "exactly one container should be managed, got {managed:?}"
        );
        // The guard drops here, tearing the project down synchronously.
    }

    let remaining = runtime
        .list_managed(&project)
        .await
        .expect("list managed containers after cleanup");
    assert!(
        remaining.is_empty(),
        "the cleanup guard must remove every container, got {remaining:?}"
    );
}
