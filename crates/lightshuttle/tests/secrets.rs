//! Integration tests for `lightshuttle secrets`.

use std::io::Write;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

fn write_temp_manifest(content: &str) -> NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(".yml")
        .tempfile()
        .expect("temp file");
    file.write_all(content.as_bytes()).expect("write manifest");
    file
}

fn write_temp_env(content: &str) -> NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(".env")
        .tempfile()
        .expect("temp env file");
    file.write_all(content.as_bytes()).expect("write env file");
    file
}

/// No `${env.*}` references anywhere in the manifest.
const MANIFEST_NO_REFS: &str = r#"
project:
  name: app
resources:
  db:
    postgres:
      version: "16"
"#;

/// Single required variable, in a real interpolation site (`env`).
const MANIFEST_REQUIRED_VAR: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: myapp:latest
      env:
        API_TOKEN: "${env.API_TOKEN}"
"#;

/// Single optional variable with a fallback.
const MANIFEST_OPTIONAL_VAR: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: myapp:latest
      env:
        LOG_LEVEL: "${env.LOG_LEVEL:-info}"
"#;

/// Mixed: one required (`API_TOKEN`) and one optional (`LOG_LEVEL:-info`).
const MANIFEST_MIXED_VARS: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: myapp:latest
      env:
        API_TOKEN: "${env.API_TOKEN}"
        LOG_LEVEL: "${env.LOG_LEVEL:-info}"
"#;

/// A reference in an image tag: not an interpolated site, so it must not be
/// reported by `secrets check`.
const MANIFEST_IMAGE_REF: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: "myapp:${env.APP_VERSION}"
"#;

// ── No references ────────────────────────────────────────────────────────────

#[test]
fn secrets_check_no_refs_succeeds() {
    let manifest = write_temp_manifest(MANIFEST_NO_REFS);
    let empty = write_temp_env("");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(empty.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("no `${env.*}` references found"));
}

// ── Scope: image references are not checked (regression for F3) ──────────────

#[test]
fn secrets_check_ignores_references_in_image_tag() {
    let manifest = write_temp_manifest(MANIFEST_IMAGE_REF);
    let empty = write_temp_env("");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(empty.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("no `${env.*}` references found"));
}

// ── Required variable ────────────────────────────────────────────────────────

#[test]
fn secrets_check_required_var_missing_fails() {
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_VAR);
    let empty = write_temp_env("");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .env_remove("API_TOKEN")
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(empty.path())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("missing"));
}

#[test]
fn secrets_check_required_var_set_via_env_file_succeeds() {
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_VAR);
    let env_file = write_temp_env("API_TOKEN=s3cr3t\n");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(env_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("set (.env)"))
        .stdout(predicate::str::contains("all required secrets are set"));
}

// ── Source distinction: resolution from the process environment (F1) ─────────

#[test]
fn secrets_check_resolves_required_var_from_process_env() {
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_VAR);
    // Not in the .env file, but present in the ambient environment: `up`
    // would resolve it, so `secrets check` must report it set, not missing.
    let empty = write_temp_env("");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .env("API_TOKEN", "from-shell")
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(empty.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("set (env)"));
}

// ── Empty value counts as unset (regression for F2) ──────────────────────────

#[test]
fn secrets_check_empty_value_is_missing() {
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_VAR);
    // An empty value overrides the ambient environment and is treated as
    // unset by the interpolator, so the required var must be reported missing.
    let env_file = write_temp_env("API_TOKEN=\n");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(env_file.path())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("missing"));
}

// ── Optional variable ────────────────────────────────────────────────────────

#[test]
fn secrets_check_optional_var_shows_default_without_env() {
    let manifest = write_temp_manifest(MANIFEST_OPTIONAL_VAR);
    let empty = write_temp_env("");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .env_remove("LOG_LEVEL")
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(empty.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("default"));
}

#[test]
fn secrets_check_optional_var_set_shows_set() {
    let manifest = write_temp_manifest(MANIFEST_OPTIONAL_VAR);
    let env_file = write_temp_env("LOG_LEVEL=debug\n");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(env_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("set (.env)"));
}

// ── Mixed variables ──────────────────────────────────────────────────────────

#[test]
fn secrets_check_mixed_fails_when_required_var_absent() {
    let manifest = write_temp_manifest(MANIFEST_MIXED_VARS);
    // Provide only the optional var; the required one stays missing.
    let env_file = write_temp_env("LOG_LEVEL=debug\n");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .env_remove("API_TOKEN")
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(env_file.path())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("missing"))
        .stdout(predicate::str::contains("set (.env)"));
}

#[test]
fn secrets_check_mixed_succeeds_when_all_vars_provided() {
    let manifest = write_temp_manifest(MANIFEST_MIXED_VARS);
    let env_file = write_temp_env("API_TOKEN=s3cr3t\nLOG_LEVEL=debug\n");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(env_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("all required secrets are set"));
}

// ── --env-file errors ────────────────────────────────────────────────────────

#[test]
fn secrets_check_explicit_env_file_not_found_fails() {
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_VAR);
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file", "/nonexistent/path.env"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("failed to load --env-file"));
}
