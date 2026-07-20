//! Tests for the docker-compose emitter.

use lightshuttle_export::{ComposeEmitter, Emitter, lower};
use lightshuttle_manifest::Manifest;

mod common;

const STACK: &str = r"
project:
  name: shop
resources:
  db:
    postgres:
      version: '16'
      password: devsecret
      volume: dbdata
  cache:
    redis:
      version: '7'
  api:
    container:
      image: alpine:3.20
      ports:
        - 8080:80
      env:
        LOG_LEVEL: info
        APP_NAME: shop
      depends_on: [db, cache]
  builder:
    dockerfile:
      context: ./app
      dockerfile: Dockerfile
      target: runtime
  worker:
    container:
      image: alpine:3.20
";

fn emit(yaml: &str) -> String {
    let manifest = Manifest::parse(yaml).expect("manifest parses");
    let model = lower(&manifest).expect("lowering succeeds");
    let artifacts = ComposeEmitter.emit(&model).expect("emit succeeds");
    assert_eq!(artifacts.files.len(), 1);
    assert_eq!(artifacts.files[0].path.to_str(), Some("docker-compose.yml"));
    artifacts.files[0].contents.clone()
}

#[test]
fn matches_golden_file() {
    let golden = include_str!("golden/docker-compose.yml");
    assert_eq!(
        emit(STACK),
        golden,
        "generated compose drifted from the golden file; \
         re-inspect the change and update tests/golden/docker-compose.yml if intended"
    );
}

/// Validates the emitted file with the real `docker compose` CLI.
/// Ignored by default: it needs Docker Compose on the host.
#[test]
#[ignore = "requires docker compose on the host"]
fn output_passes_docker_compose_config() {
    use std::io::Write;

    if !common::tool_available("docker") {
        eprintln!("skipping: docker not found on PATH");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("docker-compose.yml");
    let mut file = std::fs::File::create(&path).expect("write compose");
    file.write_all(emit(STACK).as_bytes()).expect("write bytes");

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

#[test]
fn ports_default_to_loopback() {
    let out = emit(STACK);
    assert!(
        out.contains("127.0.0.1:8080:80"),
        "api port should bind loopback by default, got:\n{out}"
    );
    assert!(
        out.contains("127.0.0.1:5432:5432"),
        "postgres port should bind loopback by default, got:\n{out}"
    );
}

#[test]
fn dependencies_use_service_healthy_condition() {
    let out = emit(STACK);
    assert!(out.contains("depends_on:"), "got:\n{out}");
    assert!(
        out.contains("condition: service_healthy"),
        "depends_on should gate on health, got:\n{out}"
    );
}

#[test]
fn named_volumes_are_declared_top_level() {
    let out = emit(STACK);
    // The top-level `volumes:` block declares the named volume used by db.
    assert!(out.contains("\nvolumes:\n"), "got:\n{out}");
    assert!(out.contains("dbdata:"), "got:\n{out}");
}

#[test]
fn environment_is_sorted() {
    let out = emit(STACK);
    let app = out.find("APP_NAME").expect("APP_NAME present");
    let log = out.find("LOG_LEVEL").expect("LOG_LEVEL present");
    assert!(app < log, "environment keys should be sorted, got:\n{out}");
}

#[test]
fn explicit_secrets_are_runtime_placeholders_not_plaintext() {
    let out = emit(
        r"
project:
  name: secure
resources:
  api:
    container:
      image: alpine
      secrets:
        DATABASE_URL: postgres://user:real-password@db/app
",
    );
    assert!(!out.contains("real-password"), "got:\n{out}");
    assert!(out.contains("DATABASE_URL: ${DATABASE_URL}"), "got:\n{out}");
}

/// Compose names both concepts as the manifest does, so this is a
/// pass-through: `entrypoint` is the executable, `command` its arguments.
#[test]
fn emits_entrypoint_and_command_separately() {
    let yaml = r"
project:
  name: shop
resources:
  svc:
    container:
      image: alpine:3.20
      entrypoint: ['sh', '-c']
      command: ['echo hi']
";
    let out = emit(yaml);
    assert!(
        out.contains("entrypoint:\n    - sh\n    - -c\n"),
        "got:\n{out}"
    );
    assert!(out.contains("command:\n    - echo hi\n"), "got:\n{out}");
}
