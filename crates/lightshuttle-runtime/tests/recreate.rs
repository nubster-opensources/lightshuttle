//! Contract tests for the recreate path: the runtime must reject a
//! duplicate container name and accept a fresh start once the previous
//! container has been removed.

use std::collections::HashMap;
use std::time::Duration;

use lightshuttle_runtime::testkit::MockRuntime;
use lightshuttle_runtime::{ContainerRuntime, ContainerSpec, ImageSource};

fn spec(name: &str) -> ContainerSpec {
    ContainerSpec {
        name: name.to_owned(),
        project: "app".to_owned(),
        resource: name.to_owned(),
        image: ImageSource::Pull("alpine".to_owned()),
        env: HashMap::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        command: None,
        healthcheck: None,
    }
}

#[tokio::test]
async fn starting_a_duplicate_name_is_rejected() {
    let runtime = MockRuntime::new();
    let s = spec("app_cache");

    runtime.start(&s).await.expect("first start succeeds");
    let err = runtime
        .start(&s)
        .await
        .expect_err("second start with the same name is rejected");

    assert!(
        err.to_string().contains("already in use"),
        "expected a name-collision error, got: {err}"
    );
}

#[tokio::test]
async fn remove_then_start_recreates_the_container() {
    let runtime = MockRuntime::new();
    let s = spec("app_cache");

    runtime.start(&s).await.expect("first start succeeds");
    runtime
        .remove(&s.name)
        .await
        .expect("remove of an existing container succeeds");
    runtime
        .start(&s)
        .await
        .expect("start after remove succeeds");
}

#[tokio::test]
async fn remove_is_idempotent_for_an_unknown_name() {
    let runtime = MockRuntime::new();
    runtime
        .remove("app_ghost")
        .await
        .expect("removing a container that never existed is a no-op");
}

#[tokio::test]
async fn stop_alone_does_not_free_the_name() {
    // Stopping leaves the container in state, so a re-up that skipped
    // `remove` would still collide. This pins the daemon-faithful
    // behaviour the manager relies on.
    let runtime = MockRuntime::new();
    let s = spec("app_cache");

    let id = runtime.start(&s).await.expect("first start succeeds");
    runtime
        .stop(&id, Duration::from_secs(1))
        .await
        .expect("stop succeeds");

    let err = runtime
        .start(&s)
        .await
        .expect_err("re-start without remove still collides after stop");
    assert!(err.to_string().contains("already in use"), "got: {err}");
}
