//! Tests for issue #283: the Compose emitter must refuse to emit a
//! `depends_on` edge that points at a resource this export disables.
//!
//! `export.compose.resources.<name>.enabled: false` drops a resource from
//! the rendered `services:` block, but a service that still `depends_on`
//! it would reference a name Compose never defines, which
//! `docker compose config` rejects. Kubernetes and Helm carry no such
//! `depends_on` field, so the same exclusion is harmless for them.

use lightshuttle_export::{
    ComposeEmitter, DisabledDependency, Emitter, ExportError, HelmEmitter, KubernetesEmitter, lower,
};
use lightshuttle_manifest::Manifest;

mod common;

/// Manifest where `api` depends on `db` and only `api` is disabled for
/// Compose, so the export still succeeds. Reused by the tests that check
/// what an accepted export looks like.
const DEPENDENT_DISABLED_STACK: &str = r"
project:
  name: shop
export:
  compose:
    resources:
      api:
        enabled: false
resources:
  db:
    container:
      image: postgres:16
  api:
    container:
      image: alpine:3.20
      depends_on: [db]
";

fn compose_output(yaml: &str) -> String {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    let artifacts = ComposeEmitter.emit(&model).expect("compose emit succeeds");
    artifacts.files[0].contents.clone()
}

fn compose_error(yaml: &str) -> ExportError {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    ComposeEmitter
        .emit(&model)
        .expect_err("compose emission should refuse a disabled dependency")
}

fn disabled_dependencies(error: &ExportError) -> (&'static str, &[DisabledDependency]) {
    match error {
        ExportError::DisabledDependencies {
            target,
            dependencies,
        } => (target, dependencies),
        other => panic!("expected ExportError::DisabledDependencies, got: {other:?}"),
    }
}

#[test]
fn direct_dependency_on_a_disabled_service_is_refused() {
    let error = compose_error(
        r"
project:
  name: shop
export:
  compose:
    resources:
      db:
        enabled: false
resources:
  db:
    postgres:
      version: '16'
      password: devsecret
      volume: dbdata
  api:
    container:
      image: alpine:3.20
      depends_on: [db]
",
    );

    let (target, dependencies) = disabled_dependencies(&error);
    assert_eq!(target, "compose");
    assert_eq!(dependencies, [DisabledDependency::new("api", "db")]);

    let message = error.to_string();
    assert!(message.contains("api"), "got: {message}");
    assert!(message.contains("db"), "got: {message}");
}

#[test]
fn transitive_dependency_through_an_enabled_service_refuses_the_inner_edge() {
    let error = compose_error(
        r"
project:
  name: shop
export:
  compose:
    resources:
      db:
        enabled: false
resources:
  db:
    container:
      image: postgres:16
  worker:
    container:
      image: alpine:3.20
      depends_on: [db]
  api:
    container:
      image: alpine:3.20
      depends_on: [worker]
",
    );

    let (target, dependencies) = disabled_dependencies(&error);
    assert_eq!(target, "compose");
    assert_eq!(dependencies, [DisabledDependency::new("worker", "db")]);
}

#[test]
fn transitive_dependency_through_a_disabled_service_refuses_the_outer_edge() {
    let error = compose_error(
        r"
project:
  name: shop
export:
  compose:
    resources:
      db:
        enabled: false
resources:
  cache:
    container:
      image: redis:7
  db:
    container:
      image: postgres:16
      depends_on: [cache]
  api:
    container:
      image: alpine:3.20
      depends_on: [db]
",
    );

    let (target, dependencies) = disabled_dependencies(&error);
    assert_eq!(target, "compose");
    assert_eq!(dependencies, [DisabledDependency::new("api", "db")]);
}

#[test]
fn every_dangling_edge_is_reported_at_once_in_sorted_order() {
    let error = compose_error(
        r"
project:
  name: shop
export:
  compose:
    resources:
      cache:
        enabled: false
      db:
        enabled: false
resources:
  worker:
    container:
      image: alpine:3.20
      depends_on: [db]
  cache:
    container:
      image: redis:7
  api:
    container:
      image: alpine:3.20
      depends_on: [db, cache]
  db:
    container:
      image: postgres:16
",
    );

    let (target, dependencies) = disabled_dependencies(&error);
    assert_eq!(target, "compose");
    assert_eq!(
        dependencies,
        [
            DisabledDependency::new("api", "cache"),
            DisabledDependency::new("api", "db"),
            DisabledDependency::new("worker", "db"),
        ]
    );
}

#[test]
fn disabling_a_dependent_service_still_exports() {
    let out = compose_output(DEPENDENT_DISABLED_STACK);
    assert!(out.contains("db"), "got:\n{out}");
    assert!(!out.contains("api"), "got:\n{out}");
}

#[test]
fn a_dependency_disabled_on_kubernetes_or_helm_does_not_refuse_those_exports() {
    let manifest = Manifest::parse(
        r"
project:
  name: shop
export:
  kubernetes:
    resources:
      db:
        enabled: false
  helm:
    resources:
      db:
        enabled: false
resources:
  db:
    container:
      image: postgres:16
  api:
    container:
      image: alpine:3.20
      depends_on: [db]
",
    )
    .expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");

    KubernetesEmitter
        .emit(&model)
        .expect("kubernetes emit succeeds even though db is disabled for kubernetes");
    HelmEmitter
        .emit(&model)
        .expect("helm emit succeeds even though db is disabled for helm");
}

/// Exclusions are per target, so one target's exclusion says nothing about
/// what another target emits: a dependency dropped from the Kubernetes
/// export is still part of the Compose one, and refusing there would reject
/// a manifest that exports perfectly well.
#[test]
fn a_dependency_disabled_only_on_another_target_does_not_refuse_compose() {
    let out = compose_output(
        r"
project:
  name: shop
export:
  kubernetes:
    resources:
      db:
        enabled: false
resources:
  db:
    container:
      image: postgres:16
  api:
    container:
      image: alpine:3.20
      depends_on: [db]
",
    );

    assert!(out.contains("depends_on"), "got:\n{out}");
    assert!(out.contains("db"), "got:\n{out}");
}

/// [`DEPENDENT_DISABLED_STACK`] excludes `api`, the only service that named
/// a dependency, so the accepted export carries no `depends_on` entry at
/// all: parsing it back is what proves the emitted YAML stays internally
/// consistent, not merely that it happens to look right as text.
#[test]
fn exported_depends_on_only_names_exported_services() {
    let out = compose_output(DEPENDENT_DISABLED_STACK);
    let parsed: serde_norway::Value =
        serde_norway::from_str(&out).expect("compose output is valid YAML");
    let services = parsed["services"]
        .as_mapping()
        .expect("services is a mapping");
    let service_names: Vec<&str> = services
        .keys()
        .map(|key| key.as_str().expect("service name is a string"))
        .collect();

    for name in &service_names {
        let Some(depends_on) = parsed["services"][*name]["depends_on"].as_mapping() else {
            continue;
        };
        for dependency in depends_on.keys() {
            let dependency = dependency.as_str().expect("dependency name is a string");
            assert!(
                service_names.contains(&dependency),
                "`{name}` depends_on names `{dependency}`, which `services:` never defines"
            );
        }
    }
}

/// Validates the accepted case with the real `docker compose` CLI. Ignored
/// by default: it needs Docker Compose on the host.
#[test]
#[ignore = "requires docker compose on the host"]
fn accepted_output_passes_docker_compose_config() {
    use std::io::Write;

    if !common::tool_available("docker") {
        eprintln!("skipping: docker not found on PATH");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("docker-compose.yml");
    let mut file = std::fs::File::create(&path).expect("write compose");
    file.write_all(compose_output(DEPENDENT_DISABLED_STACK).as_bytes())
        .expect("write bytes");

    let output = std::process::Command::new("docker")
        .args(["compose", "-f"])
        .arg(&path)
        .arg("config")
        .output()
        .expect("docker compose runs");

    assert!(
        output.status.success(),
        "docker compose config rejected the output:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
