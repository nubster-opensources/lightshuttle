//! Integration test for the lifecycle manager against a real Docker
//! daemon. Requires `docker info` to succeed; run with
//! `cargo test --test lifecycle_integration -- --ignored`.

use std::time::Duration;

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{DockerRuntime, LifecycleManager, LifecyclePlan};

const STACK: &str = r#"
project:
  name: lightshuttle_it
resources:
  alpine:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
"#;

#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn start_and_stop_a_real_stack() {
    let manifest = Manifest::parse(STACK).expect("manifest parses");
    let plan = LifecyclePlan::from_manifest(&manifest).expect("plan builds");
    let runtime = DockerRuntime::connect().expect("Docker daemon reachable");

    let (manager, _events) = LifecycleManager::new(plan, runtime);

    manager
        .start_all()
        .await
        .expect("alpine container should boot");

    manager
        .stop_all(Duration::from_secs(3))
        .await
        .expect("alpine container should stop cleanly");
}
