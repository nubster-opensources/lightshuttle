//! Inter-container networking scenario against a live Docker daemon.
//!
//! LightShuttle attaches each container to the per-project bridge network
//! under an alias equal to its manifest resource name (see
//! `docker.rs`: `aliases: Some(vec![spec.resource.clone()])`). A sibling can
//! therefore reach it by that short name. The proof is event-driven: the
//! client's healthcheck resolves the server by alias, so the client only
//! reaches `Healthy` when intra-project DNS works. No fixed sleep is used.
//!
//! Run with:
//! `cargo test -p lightshuttle-runtime --test networking -- --ignored`.

mod common;

use std::time::Duration;

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::{LifecycleManager, LifecyclePlan};

/// Generous deadline covering a cold image pull plus the first healthcheck.
const HEALTH_DEADLINE: Duration = Duration::from_secs(60);

/// Grace window for teardown.
const STOP_GRACE: Duration = Duration::from_secs(3);

/// Parse `yaml` into a `LifecyclePlan`, panicking with context on failure.
fn plan_from(yaml: &str) -> LifecyclePlan {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    LifecyclePlan::from_manifest(&manifest).expect("plan builds")
}

/// A `client` resolves a sibling `server` by its resource-name alias on the
/// shared project network; reaching `Healthy` proves intra-project DNS.
#[tokio::test]
#[ignore = "requires a running Docker daemon"]
async fn sibling_resolves_peer_by_resource_alias() {
    if !common::docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = common::unique_project("dns");
    let _guard = common::ProjectCleanup::new(project.clone());

    let plan = plan_from(&dns_stack(&project));
    let runtime = lightshuttle_runtime::DockerRuntime::connect().expect("Docker daemon reachable");
    let (manager, mut events) = LifecycleManager::new(plan, runtime);

    manager.start_all().await.expect("stack boots");

    common::wait_for_healthy(&mut events, "client", HEALTH_DEADLINE)
        .await
        .expect("client should resolve `server` by alias and report healthy");

    manager.stop_all(STOP_GRACE).await.expect("stack stops");
}

/// A long-lived `server` and a `client` whose healthcheck resolves the server
/// by alias.
///
/// The healthcheck command uses busybox `ping`, which exercises both name
/// resolution and L3 reachability on the shared network; the exact command is
/// confirmed against the running `alpine:3.20` image during the Building phase.
fn dns_stack(project: &str) -> String {
    format!(
        r#"
project:
  name: {project}
resources:
  server:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
  client:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 60"]
      depends_on: [server]
      healthcheck:
        test: ["CMD-SHELL", "ping -c1 -W2 server || exit 1"]
        interval: "1s"
        timeout: "3s"
        retries: 10
        start_period: "1s"
"#
    )
}
