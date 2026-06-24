//! Cross-OS smoke test for the Docker-free command surface.
//!
//! Runs on every desktop OS, with no Docker daemon required. It proves the
//! binary boots natively and that the pure-logic paths (manifest parsing,
//! strict validation, artifact emission) work cross-platform. The export
//! step also catches path-separator bugs on Windows.
//!
//! Run locally with:
//! `cargo test -p lightshuttle --test cli_offline -- --ignored --nocapture`

use std::io::Write as _;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;
use predicates::str::contains;
use tempfile::{NamedTempFile, TempDir};

/// Write `content` to a temporary YAML file and return the handle.
///
/// The caller keeps the handle alive for the test; dropping it deletes the
/// file.
fn write_temp_manifest(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("temp manifest created");
    file.write_all(content.as_bytes())
        .expect("manifest written to temp file");
    file
}

/// `--version` runs, `validate --strict` accepts a minimal manifest, and
/// `export compose` writes a compose file. None of this needs Docker.
#[test]
#[ignore = "smoke: cross-OS CLI job"]
fn validate_and_export_run_without_docker() {
    // The binary boots natively.
    Command::new(cargo_bin("lightshuttle"))
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("lightshuttle"));

    // A minimal, daemon-free manifest: one container with an image and a
    // command, no healthcheck and no dependency.
    let manifest = r#"
project:
  name: ls-offline-smoke
resources:
  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 1"]
"#;
    let manifest_file = write_temp_manifest(manifest);
    let path = manifest_file.path().to_str().expect("path is UTF-8");

    // Strict validation parses and accepts the manifest.
    Command::new(cargo_bin("lightshuttle"))
        .args(["--file", path, "validate", "--strict"])
        .assert()
        .success();

    // Export emits a compose artifact into a chosen directory.
    let out = TempDir::new().expect("temp output dir created");
    Command::new(cargo_bin("lightshuttle"))
        .args(["--file", path, "export", "compose", "--output"])
        .arg(out.path())
        .assert()
        .success();

    let compose = out.path().join("docker-compose.yml");
    assert!(
        compose.is_file(),
        "expected a compose file at {}",
        compose.display(),
    );
}
