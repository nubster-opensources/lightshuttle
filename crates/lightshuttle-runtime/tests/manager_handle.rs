//! Happy-path tests for `ManagerHandle` driven through the in-memory
//! `MockRuntime` testkit.

use std::sync::Arc;

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::testkit::MockRuntime;
use lightshuttle_runtime::{
    LifecycleHandle, LifecycleHandleError, LifecycleManager, LifecyclePlan, ManagerHandle,
    ResourceStatus,
};

fn build_plan(yaml: &str) -> LifecyclePlan {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    LifecyclePlan::from_manifest(&manifest).expect("plan builds")
}

const STACK: &str = r"
project:
  name: app
resources:
  cache:
    redis:
      version: '7'
  api:
    container:
      image: alpine
      depends_on: [cache]
";

async fn started_handle() -> ManagerHandle<MockRuntime> {
    let plan = build_plan(STACK);
    let runtime = MockRuntime::new();
    let (manager, _events) = LifecycleManager::new(plan, runtime);
    manager.start_all().await.expect("start_all succeeds");
    ManagerHandle::new(Arc::new(manager))
}

#[tokio::test]
async fn list_returns_one_view_per_resource_in_topological_order() {
    let handle = started_handle().await;

    let views = handle.list().await.expect("list succeeds");
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].name, "cache");
    assert_eq!(views[1].name, "api");
    assert_eq!(views[0].kind, "redis");
    assert_eq!(views[1].kind, "container");
    assert!(views.iter().all(|v| v.status == ResourceStatus::Running));
    assert!(views.iter().all(|v| v.healthy));
    assert!(views.iter().all(|v| v.started_at.is_some()));
    assert!(views.iter().all(|v| v.last_error.is_none()));
}

#[tokio::test]
async fn get_returns_view_for_known_resource() {
    let handle = started_handle().await;

    let view = handle.get("cache").await.expect("get succeeds");
    assert_eq!(view.name, "cache");
    assert_eq!(view.kind, "redis");
    assert_eq!(view.status, ResourceStatus::Running);
    assert!(view.healthy);
    assert!(view.image.starts_with("redis:"));
}

#[tokio::test]
async fn get_rejects_unknown_resource() {
    let handle = started_handle().await;

    let err = handle.get("nope").await.expect_err("unknown rejected");
    assert!(matches!(err, LifecycleHandleError::UnknownResource(name) if name == "nope"));
}

#[tokio::test]
async fn restart_succeeds_for_known_resource() {
    let handle = started_handle().await;

    handle
        .restart("cache")
        .await
        .expect("restart succeeds for a running resource");

    // The resource is healthy again after the restart cycle.
    let view = handle.get("cache").await.expect("get after restart");
    assert_eq!(view.status, ResourceStatus::Running);
    assert!(view.healthy);
}

#[tokio::test]
async fn restart_rejects_unknown_resource() {
    let handle = started_handle().await;

    let err = handle.restart("nope").await.expect_err("unknown rejected");
    assert!(matches!(err, LifecycleHandleError::UnknownResource(name) if name == "nope"));
}

#[tokio::test]
async fn logs_returns_a_stream_for_known_running_resource() {
    let handle = started_handle().await;

    let _stream = handle
        .logs("cache", false)
        .await
        .expect("logs stream available");
}

#[tokio::test]
async fn logs_rejects_unknown_resource() {
    let handle = started_handle().await;

    // `LogChunkStream` is not `Debug`, so map the `Ok` side away before
    // unwrapping the expected error.
    let err = handle
        .logs("nope", false)
        .await
        .map(|_| ())
        .expect_err("unknown rejected");
    assert!(matches!(err, LifecycleHandleError::UnknownResource(name) if name == "nope"));
}
