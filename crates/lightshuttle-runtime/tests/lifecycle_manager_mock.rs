//! Tests of `LifecycleManager` against an in-memory mock runtime.
//! No Docker daemon required.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use futures::stream::{Stream, StreamExt};
use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{
    ContainerId, ContainerRuntime, ContainerSpec, ContainerStatus, LifecycleEvent,
    LifecycleManager, LifecyclePlan, LogChunk, LogChunkStream, RuntimeError,
};

/// In-memory runtime: every container becomes healthy 30 ms after start
/// unless its name is configured as a failure target.
struct MockRuntime {
    state: Arc<Mutex<HashMap<String, MockContainer>>>,
    fail_on: Arc<Mutex<Option<String>>>,
    start_order: Arc<Mutex<Vec<String>>>,
    stop_order: Arc<Mutex<Vec<String>>>,
    started_specs: Arc<Mutex<Vec<ContainerSpec>>>,
}

struct MockContainer {
    name: String,
    status: ContainerStatus,
    started_at: Instant,
    healthy_after: Duration,
}

impl MockRuntime {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            fail_on: Arc::new(Mutex::new(None)),
            start_order: Arc::new(Mutex::new(Vec::new())),
            stop_order: Arc::new(Mutex::new(Vec::new())),
            started_specs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn fail_on_resource(&self, name: &str) {
        *self.fail_on.lock().unwrap() = Some(name.to_owned());
    }
}

impl ContainerRuntime for MockRuntime {
    async fn start(&self, spec: &ContainerSpec) -> Result<ContainerId, RuntimeError> {
        if self.fail_on.lock().unwrap().as_deref() == Some(spec.name.as_str()) {
            return Err(RuntimeError::InvalidSpec(format!(
                "mock failure for `{}`",
                spec.name
            )));
        }
        self.start_order.lock().unwrap().push(spec.name.clone());
        self.started_specs.lock().unwrap().push(spec.clone());
        let id = ContainerId::new(format!("mock-{}", spec.name));
        self.state.lock().unwrap().insert(
            id.as_str().to_owned(),
            MockContainer {
                name: spec.name.clone(),
                status: ContainerStatus::Starting,
                started_at: Instant::now(),
                healthy_after: Duration::from_millis(30),
            },
        );
        Ok(id)
    }

    async fn stop(&self, id: &ContainerId, _grace: Duration) -> Result<(), RuntimeError> {
        let mut state = self.state.lock().unwrap();
        if let Some(c) = state.get_mut(id.as_str()) {
            c.status = ContainerStatus::Stopped { exit_code: Some(0) };
            self.stop_order.lock().unwrap().push(c.name.clone());
        }
        Ok(())
    }

    async fn inspect(&self, id: &ContainerId) -> Result<ContainerStatus, RuntimeError> {
        let state = self.state.lock().unwrap();
        let c = state
            .get(id.as_str())
            .ok_or_else(|| RuntimeError::NotFound(id.as_str().to_owned()))?;
        Ok(c.status.clone())
    }

    async fn wait_healthy(&self, id: &ContainerId, timeout: Duration) -> Result<(), RuntimeError> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            {
                let mut state = self.state.lock().unwrap();
                if let Some(c) = state.get_mut(id.as_str())
                    && c.started_at.elapsed() >= c.healthy_after
                {
                    c.status = ContainerStatus::Healthy;
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(RuntimeError::Timeout {
            operation: "wait_healthy",
            after: timeout,
        })
    }

    async fn logs(&self, _id: &ContainerId, _follow: bool) -> Result<LogChunkStream, RuntimeError> {
        let empty: Pin<Box<dyn Stream<Item = Result<LogChunk, RuntimeError>> + Send>> =
            Box::pin(futures::stream::empty::<Result<LogChunk, RuntimeError>>().map(|x| x));
        let _ = SystemTime::now(); // silence unused import on this branch
        Ok(empty)
    }
}

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
    let started_capture = Arc::clone(&runtime.start_order);
    let stopped_capture = Arc::clone(&runtime.stop_order);
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    assert_eq!(started_capture.lock().unwrap().len(), 2);

    manager
        .stop_all(Duration::from_millis(100))
        .await
        .expect("stop_all succeeds");
    assert_eq!(stopped_capture.lock().unwrap().len(), 2);
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
    let started_capture = Arc::clone(&runtime.start_order);
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    let order = started_capture.lock().unwrap().clone();
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
    runtime.fail_on_resource("app_api");
    let stopped_capture = Arc::clone(&runtime.stop_order);
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    let err = manager
        .start_all()
        .await
        .expect_err("start_all should fail when api fails");
    let err_str = err.to_string();
    // The error wraps the manifest resource name (`api`), not the
    // runtime-side container name (`app_api`).
    assert!(
        err_str.contains("api"),
        "error mentions failing resource, got: {err_str}"
    );

    // db started before api failed; auto-cleanup must have stopped it.
    // stop_order records the container name (`app_db`).
    let stopped = stopped_capture.lock().unwrap().clone();
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
    let stopped_capture = Arc::clone(&runtime.stop_order);
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.unwrap();
    manager.stop_all(Duration::from_millis(50)).await.unwrap();

    let stopped = stopped_capture.lock().unwrap().clone();
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
    let specs_capture = Arc::clone(&runtime.started_specs);
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    let specs = specs_capture.lock().unwrap().clone();
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
    let specs_capture = Arc::clone(&runtime.started_specs);
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("start_all succeeds");
    let specs = specs_capture.lock().unwrap().clone();
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
