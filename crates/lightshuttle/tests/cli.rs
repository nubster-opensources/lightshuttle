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

/// A required `${env.*}` reference with no default. Used to lock the lazy
/// `validate` behaviour decided in issue #200: static validation never
/// resolves environment references, so a missing secret is not a validation
/// error. The fail-fast guard lives in `up`, and `secrets check` is the audit.
const MANIFEST_REQUIRED_ENV: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: myapp:latest
      env:
        API_TOKEN: "${env.API_TOKEN}"
"#;

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
fn validate_passes_with_an_unresolvable_required_env_reference() {
    // Issue #200: `validate` is intentionally lazy on `${env.*}`. It has no
    // `--env-file` flag and cannot see a dotenv file, so failing here would
    // be a false negative for the normal secrets workflow. A missing required
    // secret must therefore pass `validate` and only be caught by `up` or
    // `secrets check`.
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_ENV);
    Command::cargo_bin("lightshuttle")
        .expect("binary exists")
        .env_remove("API_TOKEN")
        .arg("--file")
        .arg(manifest.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("ok: project `app`"));
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

#[test]
fn up_explicit_env_file_not_found_fails_before_docker() {
    // load_env is called before DockerRuntime::connect, so a missing
    // --env-file path produces a clear error without requiring a daemon.
    let manifest = write_temp_manifest(VALID_MANIFEST);
    Command::cargo_bin("lightshuttle")
        .expect("binary exists")
        .arg("--file")
        .arg(manifest.path())
        .args(["up", "--env-file", "/nonexistent/path.env"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("failed to load --env-file"));
}
