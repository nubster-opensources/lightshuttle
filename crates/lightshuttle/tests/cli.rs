//! End-to-end CLI tests that do not require a Docker daemon.

use std::io::Write;

use assert_cmd::Command;
use predicates::prelude::*;

const VALID_MANIFEST: &str = r#"
project:
  name: app
resources:
  db:
    postgres:
      version: "16"
"#;

const INVALID_MANIFEST: &str = r"
project:
  name: app
resources:
  '1bad':
    container:
      image: alpine
";

fn write_temp_manifest(content: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(".yml")
        .tempfile()
        .expect("temp file");
    file.write_all(content.as_bytes()).expect("write manifest");
    file
}

#[test]
fn validate_accepts_a_valid_manifest() {
    let manifest = write_temp_manifest(VALID_MANIFEST);
    Command::cargo_bin("lightshuttle")
        .expect("binary exists")
        .arg("--file")
        .arg(manifest.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("ok: project `app`"));
}

#[test]
fn validate_rejects_an_invalid_manifest_with_exit_code_1() {
    let manifest = write_temp_manifest(INVALID_MANIFEST);
    Command::cargo_bin("lightshuttle")
        .expect("binary exists")
        .arg("--file")
        .arg(manifest.path())
        .arg("validate")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn manifest_dumps_resolved_yaml() {
    let manifest = write_temp_manifest(VALID_MANIFEST);
    Command::cargo_bin("lightshuttle")
        .expect("binary exists")
        .arg("--file")
        .arg(manifest.path())
        .arg("manifest")
        .assert()
        .success()
        .stdout(predicate::str::contains("project:"))
        .stdout(predicate::str::contains("resources:"));
}

#[test]
fn missing_manifest_reports_user_error() {
    Command::cargo_bin("lightshuttle")
        .expect("binary exists")
        .arg("--file")
        .arg("/path/that/does/not/exist/lightshuttle.yml")
        .arg("validate")
        .assert()
        .failure()
        .code(1);
}
