//! End-to-end tests for `lightshuttle export`, driven through the built
//! binary against a temporary manifest and output directory.

use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

const MANIFEST: &str = r"
project:
  name: shop
resources:
  db:
    postgres:
      version: '16'
      password: devsecret
  api:
    container:
      image: alpine:3.20
      ports:
        - 8080:80
      depends_on: [db]
";

fn write_manifest(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("lightshuttle.yml");
    std::fs::write(&path, MANIFEST).expect("write manifest");
    path
}

#[test]
fn export_compose_writes_docker_compose_file() {
    let home = TempDir::new().expect("temp dir");
    let manifest = write_manifest(home.path());
    let out = home.path().join("out");

    Command::cargo_bin("lightshuttle")
        .expect("binary builds")
        .arg("-f")
        .arg(&manifest)
        .args(["export", "compose", "--output"])
        .arg(&out)
        .assert()
        .success()
        .stdout(predicates::str::contains("wrote 1 file(s)"));

    let compose = std::fs::read_to_string(out.join("docker-compose.yml")).expect("compose written");
    assert!(compose.contains("services:"), "got:\n{compose}");
    assert!(compose.contains("postgres:16-alpine"), "got:\n{compose}");
    assert!(
        compose.contains("127.0.0.1:8080:80"),
        "ports should bind loopback, got:\n{compose}"
    );
}

#[test]
fn refuses_non_empty_output_without_force() {
    let home = TempDir::new().expect("temp dir");
    let manifest = write_manifest(home.path());
    let out = home.path().join("out");
    std::fs::create_dir_all(&out).expect("create out");
    std::fs::write(out.join("keep.txt"), "important").expect("seed file");

    // Without --force the command refuses to touch a non-empty directory.
    Command::cargo_bin("lightshuttle")
        .expect("binary builds")
        .arg("-f")
        .arg(&manifest)
        .args(["export", "compose", "--output"])
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicates::str::contains("not empty"));

    // The pre-existing file is untouched.
    assert_eq!(
        std::fs::read_to_string(out.join("keep.txt")).expect("keep readable"),
        "important"
    );

    // With --force it proceeds.
    Command::cargo_bin("lightshuttle")
        .expect("binary builds")
        .arg("-f")
        .arg(&manifest)
        .args(["export", "compose", "--force", "--output"])
        .arg(&out)
        .assert()
        .success();
    assert!(out.join("docker-compose.yml").exists());
}

#[test]
fn default_output_dir_is_export_target() {
    let home = TempDir::new().expect("temp dir");
    let manifest = write_manifest(home.path());

    // No --output: the command writes under ./export/compose relative to
    // the working directory.
    Command::cargo_bin("lightshuttle")
        .expect("binary builds")
        .current_dir(home.path())
        .arg("-f")
        .arg(&manifest)
        .args(["export", "compose"])
        .assert()
        .success();

    assert!(
        home.path()
            .join("export")
            .join("compose")
            .join("docker-compose.yml")
            .exists(),
        "default output should be ./export/compose/docker-compose.yml"
    );
}

#[test]
fn export_kubernetes_writes_manifests() {
    let home = TempDir::new().expect("temp dir");
    let manifest = write_manifest(home.path());
    let out = home.path().join("k8s");

    Command::cargo_bin("lightshuttle")
        .expect("binary builds")
        .arg("-f")
        .arg(&manifest)
        .args(["export", "kubernetes", "--output"])
        .arg(&out)
        .assert()
        .success();

    assert!(out.join("namespace.yaml").exists());
    let db = std::fs::read_to_string(out.join("db.yaml")).expect("db manifest written");
    assert!(db.contains("kind: Deployment"), "got:\n{db}");
    assert!(db.contains("kind: Service"), "got:\n{db}");
}

#[test]
fn export_helm_writes_chart() {
    let home = TempDir::new().expect("temp dir");
    let manifest = write_manifest(home.path());
    let out = home.path().join("chart");

    Command::cargo_bin("lightshuttle")
        .expect("binary builds")
        .arg("-f")
        .arg(&manifest)
        .args(["export", "helm", "--output"])
        .arg(&out)
        .assert()
        .success();

    assert!(out.join("Chart.yaml").exists());
    assert!(out.join("values.yaml").exists());
    assert!(out.join("templates").join("db.yaml").exists());
}

#[test]
fn unknown_target_is_rejected() {
    let home = TempDir::new().expect("temp dir");
    let manifest = write_manifest(home.path());

    Command::cargo_bin("lightshuttle")
        .expect("binary builds")
        .arg("-f")
        .arg(&manifest)
        .args(["export", "nomad"])
        .assert()
        .failure();
}
