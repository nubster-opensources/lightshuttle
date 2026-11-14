//! The reference-rendering table of the `process` networking design, as the
//! lifecycle manager actually applies it.
//!
//! # Why this file exists separately from the unit tests
//!
//! `lightshuttle-spec` already pins the four cases of the table, by calling
//! `outputs_for_consumer` directly. Those tests would stay green if nothing in
//! the product ever called that function: they prove the rendering is correct,
//! not that it is used.
//!
//! That gap is not hypothetical. Dependency outputs used to be computed once
//! by each producer and broadcast unchanged to every dependent, so a `process`
//! depending on a database received the address a *container* would use: a
//! Docker DNS name a native process cannot resolve. The process would start,
//! then fail to connect, with nothing in the run to say why.
//!
//! These tests drive the real [`LifecycleManager`] over a mixed project and
//! read the environment each resource was actually started with.

use std::collections::HashMap;
use std::time::Duration;

use lightshuttle_manifest::Manifest;
use lightshuttle_runtime::testkit::MockRuntime;
use lightshuttle_runtime::{LifecycleManager, LifecyclePlan};

/// A database, a container that consumes it, and a native process that
/// consumes it too. The same reference, read from both sides.
const MIXED_PROJECT: &str = r#"
project:
  name: acme
resources:
  db:
    postgres:
      version: "16"
  api:
    container:
      image: alpine
      ports: ["8080:80"]
      env:
        DB_HOST: "${resources.db.host}"
  worker:
    process:
      command: ["node", "worker.js"]
      port: 5173
      env:
        DB_HOST: "${resources.db.host}"
        API_HOST: "${resources.api.host}"
"#;

/// Starts the whole manifest on a mock runtime and returns the environment
/// each resource was started with, by resource name.
async fn started_environments(yaml: &str) -> HashMap<String, HashMap<String, String>> {
    let manifest = Manifest::parse(yaml).expect("the manifest under test must parse");
    let plan = LifecyclePlan::from_manifest(&manifest).expect("the plan must build");
    let runtime = MockRuntime::new();
    let observer = runtime.clone();

    let (manager, _events) = LifecycleManager::new(plan, runtime);
    manager.start_all().await.expect("the stack must start");

    // Give the mock's healthcheck window time to elapse so every dependent has
    // been through its own start, which is where the rendering happens.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut environments = HashMap::new();
    for spec in observer.started_specs() {
        environments.insert(spec.resource.clone(), spec.env.clone());
    }
    for spec in observer.started_processes() {
        environments.insert(spec.resource.clone(), spec.env.clone());
    }
    environments
}

fn environment_of<'a>(
    environments: &'a HashMap<String, HashMap<String, String>>,
    resource: &str,
) -> &'a HashMap<String, String> {
    environments
        .get(resource)
        .unwrap_or_else(|| panic!("`{resource}` was never started; started: {environments:?}"))
}

#[tokio::test]
async fn a_container_reaches_a_dependency_by_its_container_name() {
    let environments = started_environments(MIXED_PROJECT).await;
    let api = environment_of(&environments, "api");

    assert_eq!(
        api.get("DB_HOST").map(String::as_str),
        Some("acme_db"),
        "a container reaches a sibling across the project network, by name"
    );
}

/// The test the whole wiring exists for. Before the manager rendered per
/// consumer, this value was `acme_db`: a name resolved by the project
/// network's DNS, which a process running on the developer's machine is not
/// attached to and cannot resolve. Nothing failed at start; the process simply
/// never reached its database.
#[tokio::test]
async fn a_process_reaches_a_dependency_on_the_loopback_not_by_its_docker_name() {
    let environments = started_environments(MIXED_PROJECT).await;
    let worker = environment_of(&environments, "worker");

    assert_eq!(
        worker.get("DB_HOST").map(String::as_str),
        Some("127.0.0.1"),
        "a native process is not on the project network and must be given the \
         loopback, never the container name"
    );
}

/// The same manifest, the same reference, two different values. Stated on its
/// own because it is the property the design turns on, and because each of the
/// two tests above could be satisfied by an implementation that ignored the
/// consumer entirely and happened to pick that side's answer.
#[tokio::test]
async fn the_same_reference_renders_differently_for_each_consumer() {
    let environments = started_environments(MIXED_PROJECT).await;

    let from_container = environment_of(&environments, "api").get("DB_HOST").cloned();
    let from_process = environment_of(&environments, "worker")
        .get("DB_HOST")
        .cloned();

    assert!(
        from_container.is_some() && from_process.is_some(),
        "both consumers must have resolved the reference"
    );
    assert_ne!(
        from_container, from_process,
        "a reference that rendered identically on both sides would mean one of \
         the two cannot reach the service"
    );
}

/// A process reaching a container gets the port published on the host, not the
/// port inside the container. `api` maps host 8080 onto container 80, and 80 is
/// the number a process must never be handed: nothing on the developer's
/// machine listens there.
#[tokio::test]
async fn a_process_reaching_a_container_gets_the_published_host_port() {
    let environments = started_environments(MIXED_PROJECT).await;
    let worker = environment_of(&environments, "worker");

    assert_eq!(
        worker.get("API_HOST").map(String::as_str),
        Some("127.0.0.1"),
        "a process reaches a container through the host, not the bridge"
    );
    assert_eq!(
        worker.get("LSH_API_PORT").map(String::as_str),
        Some("8080"),
        "the automatic variable must carry the published host port, not the \
         container port; got env: {worker:?}"
    );
}

/// The automatic `LSH_*` variables and the `${resources.*}` interpolations must
/// come from one rendering, not two. Two renderings would eventually disagree,
/// and a service handed two different addresses for one dependency fails in a
/// way that looks like a network problem.
#[tokio::test]
async fn the_automatic_variables_agree_with_the_interpolated_ones() {
    let environments = started_environments(MIXED_PROJECT).await;
    let worker = environment_of(&environments, "worker");

    assert_eq!(
        worker.get("DB_HOST"),
        worker.get("LSH_DB_HOST"),
        "the interpolated reference and the automatic variable must name the \
         same address; got env: {worker:?}"
    );
}
