//! Tests for `LifecycleManager::restart_one` driven through the
//! in-memory `MockRuntime` testkit.

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::testkit::MockRuntime;
use lightshuttle_runtime::{LifecycleError, LifecycleEvent, LifecycleManager, LifecyclePlan};

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

/// Collect every event currently sitting on the receiver into a vector,
/// without blocking. The mock runtime emits events synchronously, so a
/// drain after each manager call is sufficient.
fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<LifecycleEvent>) -> Vec<LifecycleEvent> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(event);
    }
    out
}

#[tokio::test]
async fn restart_one_emits_stopped_started_healthy_in_order() {
    let plan = build_plan(STACK);
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, mut events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("initial start");
    let _ = drain(&mut events);

    manager
        .restart_one("cache")
        .await
        .expect("restart_one succeeds for a running resource");
    let after = drain(&mut events);

    let kinds: Vec<&'static str> = after
        .iter()
        .map(|e| match e {
            LifecycleEvent::ResourceStopped { name } if name == "cache" => "stopped",
            LifecycleEvent::ResourceStarted { name, .. } if name == "cache" => "started",
            LifecycleEvent::ResourceHealthy { name } if name == "cache" => "healthy",
            _ => "other",
        })
        .collect();

    let stopped_idx = kinds
        .iter()
        .position(|k| *k == "stopped")
        .expect("stopped event for cache");
    let started_idx = kinds
        .iter()
        .position(|k| *k == "started")
        .expect("started event for cache");
    let healthy_idx = kinds
        .iter()
        .position(|k| *k == "healthy")
        .expect("healthy event for cache");

    assert!(
        stopped_idx < started_idx && started_idx < healthy_idx,
        "events must arrive in order stopped < started < healthy, got {kinds:?}"
    );

    // The dependent `api` must not have been stopped during the
    // restart cycle (no extra `app_api` entry in stop_order).
    let stopped = observer.stopped_resources();
    assert!(
        !stopped.contains(&"app_api".to_owned()),
        "dependent `api` must keep running across restart_one, stop_order = {stopped:?}"
    );
}

#[tokio::test]
async fn restart_one_rejects_unknown_resource() {
    let plan = build_plan(STACK);
    let runtime = MockRuntime::new();
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("initial start");

    let err = manager
        .restart_one("nope")
        .await
        .expect_err("unknown resource is rejected");
    assert!(
        matches!(&err, LifecycleError::ResourceNotFound(name) if name == "nope"),
        "expected ResourceNotFound(\"nope\"), got {err:?}"
    );
}
