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

/// Single required variable: `${env.APP_VERSION}`.
const MANIFEST_REQUIRED_VAR: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: "myapp:${env.APP_VERSION}"
"#;

/// Single optional variable with a fallback: `${env.LOG_LEVEL:-info}`.
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

/// Mixed: one required (`APP_VERSION`) and one optional (`LOG_LEVEL:-info`).
const MANIFEST_MIXED_VARS: &str = r#"
project:
  name: app
resources:
  app:
    container:
      image: "myapp:${env.APP_VERSION}"
      env:
        LOG_LEVEL: "${env.LOG_LEVEL:-info}"
"#;

// ── No references ────────────────────────────────────────────────────────────

#[test]
fn secrets_check_no_refs_succeeds() {
    let manifest = write_temp_manifest(MANIFEST_NO_REFS);
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no `${env.*}` references found"));
}

// ── Required variable ────────────────────────────────────────────────────────

#[test]
fn secrets_check_required_var_missing_fails() {
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_VAR);
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("missing"));
}

#[test]
fn secrets_check_required_var_set_via_env_file_succeeds() {
    let manifest = write_temp_manifest(MANIFEST_REQUIRED_VAR);
    let env_file = write_temp_env("APP_VERSION=1.2.3\n");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(env_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("all required secrets are set"));
}

// ── Optional variable ────────────────────────────────────────────────────────

#[test]
fn secrets_check_optional_var_shows_default_without_env() {
    let manifest = write_temp_manifest(MANIFEST_OPTIONAL_VAR);
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check"])
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
        .stdout(predicate::str::contains("set"));
}

// ── Mixed variables ──────────────────────────────────────────────────────────

#[test]
fn secrets_check_mixed_fails_when_required_var_absent() {
    let manifest = write_temp_manifest(MANIFEST_MIXED_VARS);
    // Provide only the optional var; the required one stays missing.
    let env_file = write_temp_env("LOG_LEVEL=debug\n");
    Command::cargo_bin("lightshuttle")
        .unwrap()
        .arg("--file")
        .arg(manifest.path())
        .args(["secrets", "check", "--env-file"])
        .arg(env_file.path())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("missing"))
        .stdout(predicate::str::contains("set"));
}

#[test]
fn secrets_check_mixed_succeeds_when_all_vars_provided() {
    let manifest = write_temp_manifest(MANIFEST_MIXED_VARS);
    let env_file = write_temp_env("APP_VERSION=1.2.3\nLOG_LEVEL=debug\n");
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
