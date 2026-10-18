//! Lifecycle of a native `process` resource: started, logging, and stopped
//! whole, down to whatever it spawned itself.
//!
//! Every test here launches a real operating-system process through
//! external tools (`sh`, `kill`) rather than a mock. A test suite that
//! quietly skips when such a tool is missing reports green on a machine
//! where the behaviour was never exercised at all, which is worse than an
//! honest failure: each test below therefore checks the tool it needs is on
//! `PATH` and fails loudly, not silently, when it is not.
//!
//! This is also the only file in this change where `#[ignore]` is used, and
//! only for that reason: these tests spawn and kill real processes, which
//! is unsuitable for a default `cargo test` run but must still exist and
//! fail honestly when run deliberately with `--ignored`.
//!
//! Run locally with:
//! `cargo test -p lightshuttle --test process_lifecycle -- --ignored --nocapture`

#![cfg(unix)]

use std::net::{IpAddr, Ipv4Addr};
use std::process::Command;
use std::time::{Duration, Instant};

use futures::StreamExt;

// `ProcessSpec` is used here as `lightshuttle_runtime::ProcessSpec`. Today
// it is only reachable as `lightshuttle_spec::ProcessSpec`, and this crate
// (`lightshuttle`) does not depend on `lightshuttle-spec` directly, only on
// `lightshuttle-runtime`; unlike `ContainerSpec`, `Argument` and
// `ImageSource`, `lightshuttle-runtime`'s crate root does not currently
// re-export `ProcessSpec` (see its `pub use lightshuttle_spec::{...}` list
// in `src/lib.rs`). This file therefore does not compile until that
// re-export is added; nothing is added to production code by this change.
use lightshuttle_runtime::{
    ContainerStatus, ProcessRegistry, ProcessRuntime, ProcessSpec, ResourceId,
};

/// Fails loudly when `sh` is not on `PATH`.
fn require_sh() {
    let available = Command::new("sh")
        .args(["-c", "true"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    assert!(
        available,
        "these tests spawn processes through `sh -c` and require it on PATH; \
         none was found"
    );
}

/// Fails loudly when `kill` is not on `PATH`. Checked against this test
/// process's own pid, which must always answer.
fn require_kill() {
    let own_pid = std::process::id().to_string();
    let available = Command::new("kill")
        .args(["-0", &own_pid])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    assert!(
        available,
        "these tests check process liveness through `kill -0` and require \
         it on PATH; none was found"
    );
}

/// Returns `true` when `pid` names a live process, checked through `kill
/// -0` rather than this crate's own bookkeeping, so a bug in that
/// bookkeeping cannot also hide the process it claims to have killed.
fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Drains every chunk currently retained plus whatever the process still
/// has to print, up to a short timeout, and returns it decoded as UTF-8.
async fn read_all_logs(runtime: &ProcessRuntime, id: &ResourceId) -> String {
    let mut stream = runtime
        .logs(id, false)
        .await
        .expect("logs are available for a process this runtime started");
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk.expect("a log chunk reads without error").bytes);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn loopback() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

#[tokio::test]
#[ignore = "spawns a real process"]
async fn a_started_process_prints_its_logs_and_stops_with_the_operating_system_process() {
    require_sh();
    require_kill();

    let tmp = tempfile::tempdir().expect("temp state root is created");
    let runtime = ProcessRuntime::new(tmp.path());

    let spec = ProcessSpec::new(
        "proj_echo".to_owned(),
        "proj".to_owned(),
        "echo".to_owned(),
        vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "echo hello-from-process; sleep 30".to_owned(),
        ],
    );

    let id = runtime
        .start(&spec, loopback())
        .await
        .expect("process starts");

    // Read the pid this runtime recorded, so termination can be checked
    // against the real operating-system process rather than this crate's
    // own bookkeeping.
    let registry = ProcessRegistry::for_project(tmp.path(), "proj");
    let records = registry.records().expect("registry reads back");
    let pid = records
        .iter()
        .find(|r| r.resource == "echo")
        .expect("the process was recorded")
        .pid;

    // Give the process a moment to print before reading its logs.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let logs = read_all_logs(&runtime, &id).await;
    assert!(
        logs.contains("hello-from-process"),
        "the process's own output must reach the log buffer; got: {logs:?}"
    );

    assert!(
        process_is_alive(pid),
        "precondition: the process must still be running before it is stopped"
    );

    runtime
        .stop(&id, Duration::from_secs(5))
        .await
        .expect("stop succeeds");

    assert!(
        !process_is_alive(pid),
        "stopping the resource must end the real operating-system process, \
         pid {pid}"
    );
}

/// The central test of this lot: stopping a `process` resource must end the
/// whole tree it grew, not just the one process this runtime launched
/// directly.
///
/// A program with no descendant of its own would prove nothing here: killing
/// the single launched process already looks like complete success in that
/// case. This spawns a shell (the resource this runtime starts directly)
/// which itself backgrounds a further child of its own, prints that child's
/// pid, and waits on it. Relative to this test, that background process is
/// a grandchild: exactly the process a supervisor that kills only what it
/// directly launched would leave running.
#[tokio::test]
#[ignore = "spawns a real process tree"]
async fn stopping_a_process_kills_its_whole_tree() {
    require_sh();
    require_kill();

    let tmp = tempfile::tempdir().expect("temp state root is created");
    let runtime = ProcessRuntime::new(tmp.path());

    let spec = ProcessSpec::new(
        "proj_tree".to_owned(),
        "proj".to_owned(),
        "tree".to_owned(),
        vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "sleep 300 & echo GRANDCHILD_PID=$!; wait".to_owned(),
        ],
    );

    let id = runtime
        .start(&spec, loopback())
        .await
        .expect("process starts");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut grandchild_pid = None;
    while grandchild_pid.is_none() && Instant::now() < deadline {
        let logs = read_all_logs(&runtime, &id).await;
        grandchild_pid = logs
            .lines()
            .find_map(|line| line.strip_prefix("GRANDCHILD_PID="))
            .and_then(|value| value.trim().parse::<u32>().ok());
        if grandchild_pid.is_none() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    let grandchild_pid =
        grandchild_pid.expect("the grandchild's pid is printed to stdout within 5 seconds");

    assert!(
        process_is_alive(grandchild_pid),
        "precondition: the grandchild (pid {grandchild_pid}) must be alive \
         before the resource is stopped"
    );

    runtime
        .stop(&id, Duration::from_secs(5))
        .await
        .expect("stop succeeds");

    assert!(
        !process_is_alive(grandchild_pid),
        "stopping the resource must kill its whole process tree, including \
         the grandchild (pid {grandchild_pid}), not only the process this \
         runtime launched directly"
    );
}

/// A `process` resource is a long-running service: a dependent does not
/// wait for it to finish, it waits for it to stand up. Its own exit, even a
/// clean zero status, therefore means it stopped serving, and that fact
/// must be reported rather than swallowed: this checks the exact exit code
/// reaches the diagnostic, including the zero case a naive implementation
/// would be tempted to treat as success.
#[tokio::test]
#[ignore = "spawns a real process"]
async fn any_exit_including_a_zero_status_is_reported_with_its_exact_code() {
    require_sh();

    let tmp = tempfile::tempdir().expect("temp state root is created");
    let runtime = ProcessRuntime::new(tmp.path());

    let spec = ProcessSpec::new(
        "proj_quits".to_owned(),
        "proj".to_owned(),
        "quits".to_owned(),
        vec!["sh".to_owned(), "-c".to_owned(), "exit 0".to_owned()],
    );

    let id = runtime
        .start(&spec, loopback())
        .await
        .expect("process starts");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut observed_exit_code = None;
    while observed_exit_code.is_none() && Instant::now() < deadline {
        match runtime.inspect(&id).await.expect("inspect succeeds") {
            ContainerStatus::Stopped { exit_code } => observed_exit_code = Some(exit_code),
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    let observed_exit_code =
        observed_exit_code.expect("the process is observed to stop within 5 seconds");

    assert_eq!(
        observed_exit_code,
        Some(0),
        "a zero exit is still a failure for a long-running service and its \
         exact code must reach the diagnostic, not be swallowed into a bare \
         \"stopped\""
    );
}
