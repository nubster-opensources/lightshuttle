//! Tests of `LifecycleManager` against an in-memory mock runtime.
//! No Docker daemon required.

use std::time::Duration;

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::testkit::MockRuntime;
use lightshuttle_runtime::{LifecycleEvent, LifecycleManager, LifecyclePlan};

fn build_plan(yaml: &str) -> LifecyclePlan {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    LifecyclePlan::from_manifest(&manifest).expect("plan builds")
}

#[tokio::test]
async fn starts_and_stops_independent_resources() {
    let plan = build_plan(
        r"
project:
  name: app
resources:
  cache:
    redis:
      version: '7'
  api:
    container:
      image: alpine
",
    );
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    assert_eq!(observer.started_resources().len(), 2);

    manager
        .stop_all(Duration::from_millis(100))
        .await
        .expect("stop_all succeeds");
    assert_eq!(observer.stopped_resources().len(), 2);
}

#[tokio::test]
async fn respects_dependency_order_on_start() {
    let plan = build_plan(
        r"
project:
  name: app
resources:
  db:
    postgres:
      version: '16'
  api:
    container:
      image: alpine
      depends_on: [db]
",
    );
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    let order = observer.started_resources();
    let db_idx = order
        .iter()
        .position(|n| n == "app_db")
        .expect("db started");
    let api_idx = order
        .iter()
        .position(|n| n == "app_api")
        .expect("api started");
    assert!(db_idx < api_idx, "db must start before api, got {order:?}");
}

#[tokio::test]
async fn auto_cleanup_on_start_failure() {
    let plan = build_plan(
        r"
project:
  name: app
resources:
  db:
    postgres:
      version: '16'
  api:
    container:
      image: alpine
      depends_on: [db]
",
    );
    let runtime = MockRuntime::new();
    runtime.fail_on("app_api");
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    let err = manager
        .start_all()
        .await
        .expect_err("start_all should fail when api fails");
    let err_str = err.to_string();
    assert!(
        err_str.contains("api"),
        "error mentions failing resource, got: {err_str}"
    );

    let stopped = observer.stopped_resources();
    assert!(
        stopped.contains(&"app_db".to_owned()),
        "auto-cleanup should stop db, got: {stopped:?}"
    );
}

#[tokio::test]
async fn stop_all_in_reverse_topological_order() {
    let plan = build_plan(
        r"
project:
  name: app
resources:
  db:
    postgres:
      version: '16'
  api:
    container:
      image: alpine
      depends_on: [db]
",
    );
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.unwrap();
    manager.stop_all(Duration::from_millis(50)).await.unwrap();

    let stopped = observer.stopped_resources();
    let api_idx = stopped
        .iter()
        .position(|n| n == "app_api")
        .expect("api stopped");
    let db_idx = stopped
        .iter()
        .position(|n| n == "app_db")
        .expect("db stopped");
    assert!(
        api_idx < db_idx,
        "api must stop before db (reverse topo), got {stopped:?}"
    );
}

#[tokio::test]
async fn emits_lifecycle_events() {
    let plan = build_plan(
        r"
project:
  name: app
resources:
  cache:
    redis:
      version: '7'
",
    );
    let runtime = MockRuntime::new();
    let (manager, mut events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.unwrap();
    manager.stop_all(Duration::from_millis(50)).await.unwrap();

    let mut kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        kinds.push(match event {
            LifecycleEvent::ResourceStarted { .. } => "started",
            LifecycleEvent::ResourceHealthy { .. } => "healthy",
            LifecycleEvent::ResourceFailed { .. } => "failed",
            LifecycleEvent::ResourceStopped { .. } => "stopped",
            LifecycleEvent::StackStarted => "stack_started",
            LifecycleEvent::StackStopping => "stack_stopping",
            LifecycleEvent::StackStopped => "stack_stopped",
        });
    }
    assert!(kinds.contains(&"started"), "got: {kinds:?}");
    assert!(kinds.contains(&"healthy"), "got: {kinds:?}");
    assert!(kinds.contains(&"stack_started"), "got: {kinds:?}");
    assert!(kinds.contains(&"stopped"), "got: {kinds:?}");
    assert!(kinds.contains(&"stack_stopped"), "got: {kinds:?}");
}

#[tokio::test]
async fn resolves_dependency_outputs_in_env() {
    let plan = build_plan(
        r"
project:
  name: app
resources:
  api_db:
    postgres:
      version: '16'
  api:
    container:
      image: alpine
      env:
        DATABASE_URL: ${resources.api_db.url}
      depends_on: [api_db]
",
    );
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    let specs = observer.started_specs();
    let api_spec = specs
        .iter()
        .find(|s| s.name == "app_api")
        .expect("api spec captured");
    let url = api_spec
        .env
        .get("DATABASE_URL")
        .expect("DATABASE_URL env present");
    assert!(
        url.starts_with("postgres://"),
        "DATABASE_URL should be resolved to a real url, got `{url}`"
    );
    assert!(
        url.contains("@app_api_db:5432/api_db"),
        "DATABASE_URL should point at the db host/db, got `{url}`"
    );
}

#[tokio::test]
async fn auto_injects_lsh_env_vars() {
    let plan = build_plan(
        r"
project:
  name: app
resources:
  api_db:
    postgres:
      version: '16'
  api:
    container:
      image: alpine
      depends_on: [api_db]
",
    );
    let runtime = MockRuntime::new();
    let observer = runtime.clone();
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    let specs = observer.started_specs();
    let api_spec = specs
        .iter()
        .find(|s| s.name == "app_api")
        .expect("api spec captured");

    assert_eq!(
        api_spec.env.get("LSH_API_DB_HOST").map(String::as_str),
        Some("app_api_db")
    );
    assert_eq!(
        api_spec.env.get("LSH_API_DB_PORT").map(String::as_str),
        Some("5432")
    );
    assert_eq!(
        api_spec.env.get("LSH_API_DB_DATABASE").map(String::as_str),
        Some("api_db")
    );
    assert!(api_spec.env.contains_key("LSH_API_DB_URL"));
    assert!(api_spec.env.contains_key("LSH_API_DB_PASSWORD"));
}
