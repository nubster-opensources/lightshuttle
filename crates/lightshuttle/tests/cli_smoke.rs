//! Smoke test for the `lightshuttle up` / `lightshuttle down` CLI commands.
//!
//! Strategy: `up` is a blocking supervisor (it runs until a signal), so it is
//! spawned as a child process. `down` is independent and reads the same
//! manifest to tear the stack down by label. Linux-only because the test uses
//! `docker ps` polling to detect the running container without relying on a
//! fixed sleep.
//!
//! Run locally with:
//! `cargo test -p lightshuttle --test cli_smoke -- --ignored --nocapture`

#![cfg(target_os = "linux")]

use std::io::Write as _;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use predicates::str::contains;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Inline helpers (the lightshuttle crate has no access to the runtime common)
// ---------------------------------------------------------------------------

/// Returns `true` when a Docker daemon answers `docker info`.
fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Returns a collision-free project name built from `prefix`, the current
/// process id, and a per-process atomic counter.
///
/// The result is restricted to `[a-z0-9-]` and kept under 32 characters so it
/// is a valid DNS label and a valid Docker network suffix.
fn unique_project(prefix: &str) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ls-smoke-{prefix}-{}-{seq}", std::process::id())
}

/// RAII guard that removes containers and the project network on drop.
struct ProjectCleanup {
    project: String,
}

impl ProjectCleanup {
    fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
        }
    }
}

impl Drop for ProjectCleanup {
    fn drop(&mut self) {
        let label = format!("label=lightshuttle.project={}", self.project);
        // Remove all containers bearing the project label (best effort).
        if let Ok(listed) = Command::new("docker")
            .args(["ps", "-aq", "--filter", &label])
            .output()
        {
            let ids = String::from_utf8_lossy(&listed.stdout);
            for id in ids.split_whitespace() {
                let _ = Command::new("docker").args(["rm", "-f", id]).output();
            }
        }
        // Remove the project network (best effort).
        let network = format!("lightshuttle-{}", self.project);
        let _ = Command::new("docker")
            .args(["network", "rm", &network])
            .output();
    }
}

/// Write `content` to a temporary YAML file and return the handle.
///
/// The caller must keep the handle alive for the duration of the test;
/// dropping it deletes the file.
fn write_temp_manifest(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("temp file created");
    f.write_all(content.as_bytes())
        .expect("manifest written to temp file");
    f
}

/// Poll `docker ps` until at least one container labelled with `project` is
/// running, or until `deadline` elapses.
///
/// A short sleep between polls is acceptable here: this is a black-box CLI
/// integration test where there is no event channel to drive. The poll
/// interval (300 ms) keeps the outer timeout meaningful.
fn wait_for_running(project: &str, deadline: Duration) {
    let label = format!("label=lightshuttle.project={project}");
    let start = Instant::now();
    loop {
        let output = Command::new("docker")
            .args(["ps", "-q", "--filter", &label, "--filter", "status=running"])
            .output()
            .expect("docker ps runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            return;
        }
        assert!(
            start.elapsed() < deadline,
            "timed out waiting for project `{project}` containers to reach status=running",
        );
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Returns `true` when at least one container (running or stopped) still
/// carries the project's label.
fn any_container_exists(project: &str) -> bool {
    let label = format!("label=lightshuttle.project={project}");
    let output = Command::new("docker")
        .args(["ps", "-aq", "--filter", &label])
        .output()
        .expect("docker ps runs");
    !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

/// Returns `true` when the `lightshuttle-<project>` bridge network exists.
fn network_exists(project: &str) -> bool {
    Command::new("docker")
        .args(["network", "inspect", &format!("lightshuttle-{project}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Force-remove every container labelled with `project`, leaving the network
/// untouched. Used to simulate a manager killed hard after its containers were
/// already reaped, so only an orphaned network remains.
fn remove_project_containers(project: &str) {
    let label = format!("label=lightshuttle.project={project}");
    if let Ok(listed) = Command::new("docker")
        .args(["ps", "-aq", "--filter", &label])
        .output()
    {
        for id in String::from_utf8_lossy(&listed.stdout).split_whitespace() {
            let _ = Command::new("docker").args(["rm", "-f", id]).output();
        }
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// `up` boots the stack (observed via `docker ps` polling); `down` tears it
/// down independently by reading the same manifest and matching by label.
#[test]
#[ignore = "requires a running Docker daemon"]
fn up_boots_then_down_tears_down() {
    if !docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = unique_project("up");
    let _guard = ProjectCleanup::new(project.clone());

    // Minimal manifest: one alpine container, no healthcheck.
    let manifest = format!(
        r#"
project:
  name: {project}
resources:
  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 120"]
"#
    );
    let manifest_file = write_temp_manifest(&manifest);
    let path = manifest_file.path().to_str().expect("path is UTF-8");

    // `up` is a blocking supervisor: spawn it as a child process.
    let mut up = Command::new(cargo_bin("lightshuttle"))
        .args(["--file", path, "up"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("lightshuttle up spawns");

    // Wait until Docker reports at least one running container for this project.
    wait_for_running(&project, Duration::from_secs(60));

    // `down` reads the manifest and stops the stack independently.
    assert_cmd::Command::new(cargo_bin("lightshuttle"))
        .args(["--file", path, "down"])
        .assert()
        .success()
        .stdout(contains("stopped:"));

    // Shutdown must leave no managed container and no project network behind.
    assert!(
        !any_container_exists(&project),
        "down must remove every managed container, one still exists for `{project}`"
    );
    assert!(
        !network_exists(&project),
        "down must remove the project network `lightshuttle-{project}`"
    );

    // Stop the supervisor child after teardown.
    let _ = up.kill();
    let _ = up.wait();
}

/// `down` reclaims a project network left orphaned by a manager that was
/// killed hard after its containers were already gone. With no container to
/// stop, `down` must still remove the network instead of returning early.
#[test]
#[ignore = "requires a running Docker daemon"]
fn down_reclaims_an_orphaned_network_with_no_containers() {
    if !docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let project = unique_project("orph");
    let _guard = ProjectCleanup::new(project.clone());

    let manifest = format!(
        r#"
project:
  name: {project}
resources:
  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "sleep 120"]
"#
    );
    let manifest_file = write_temp_manifest(&manifest);
    let path = manifest_file.path().to_str().expect("path is UTF-8");

    // Boot the stack so lightshuttle creates the labelled project network.
    let mut up = Command::new(cargo_bin("lightshuttle"))
        .args(["--file", path, "up"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("lightshuttle up spawns");
    wait_for_running(&project, Duration::from_secs(60));

    // Hard-kill the supervisor (no graceful teardown), then reap the
    // containers manually. The labelled network is now orphaned.
    let _ = up.kill();
    let _ = up.wait();
    remove_project_containers(&project);
    assert!(
        network_exists(&project),
        "precondition: the orphaned network should still exist"
    );

    // `down` finds no container yet must still reclaim the network.
    assert_cmd::Command::new(cargo_bin("lightshuttle"))
        .args(["--file", path, "down"])
        .assert()
        .success()
        .stdout(contains("nothing to stop"));

    assert!(
        !network_exists(&project),
        "down must reclaim the orphaned network `lightshuttle-{project}`"
    );
}
