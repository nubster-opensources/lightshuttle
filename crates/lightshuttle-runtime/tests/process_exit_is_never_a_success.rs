//! A `process` resource is a long-running service, so every way it can end is
//! a failure, an exit with status zero included.
//!
//! A dependent does not wait for its antecedent to finish, it waits for it to
//! stand up. A process that returned zero has stopped serving, and reporting
//! that as success would let its dependents start against nothing and fail
//! later, somewhere else, for a reason that no longer points here.
//!
//! # Why this file exists beside `lightshuttle/tests/process_lifecycle.rs`
//!
//! That file covers the same rule, and more thoroughly, but it carries
//! `#![cfg(unix)]`: on Windows the whole file disappears, the test binary
//! starts empty, and nothing says so. The rule would then be enforced by the
//! code and checked by nobody on the platform this is most often developed on.
//!
//! These tests run everywhere, by spawning the shortest possible program that
//! the platform can run, and by asking the runtime what it made of its exit.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use lightshuttle_runtime::{ContainerStatus, ProcessRuntime};
use lightshuttle_spec::ProcessSpec;

/// A command that ends immediately with `code`, on either platform.
#[cfg(unix)]
fn exit_with(code: i32) -> Vec<String> {
    vec!["sh".to_owned(), "-c".to_owned(), format!("exit {code}")]
}

#[cfg(windows)]
fn exit_with(code: i32) -> Vec<String> {
    vec!["cmd".to_owned(), "/C".to_owned(), format!("exit {code}")]
}

/// Starts a process that ends with `code` and reports how the runtime saw it.
async fn status_after_exiting_with(code: i32) -> ContainerStatus {
    let root = tempfile::tempdir().expect("temp state root is created");
    let runtime = ProcessRuntime::new(root.path());
    let spec = ProcessSpec::new(
        "exits_probe".to_owned(),
        "exits".to_owned(),
        "probe".to_owned(),
        exit_with(code),
    );

    let id = runtime
        .start(&spec, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("the probe starts");

    // The probe is a one-shot: give the operating system a moment to reap it
    // before asking what became of it.
    for _ in 0..50 {
        let status = runtime
            .inspect(&id)
            .await
            .expect("the probe is inspectable");
        if matches!(status, ContainerStatus::Stopped { .. }) {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    runtime
        .inspect(&id)
        .await
        .expect("the probe is inspectable")
}

/// The case the arbitration turns on. Nothing else in this suite would notice
/// a runtime that quietly treated zero as "still fine".
#[tokio::test]
async fn an_exit_with_status_zero_is_reported_as_stopped_not_running() {
    let status = status_after_exiting_with(0).await;

    assert_eq!(
        status,
        ContainerStatus::Stopped { exit_code: Some(0) },
        "a process that returned zero has stopped serving; for a long-running \
         service that is a failure, not a success"
    );
}

/// The contrast, so the assertion above cannot be satisfied by a runtime that
/// calls everything stopped regardless of what happened.
#[tokio::test]
async fn a_non_zero_exit_is_reported_with_its_exact_code() {
    let status = status_after_exiting_with(3).await;

    assert_eq!(status, ContainerStatus::Stopped { exit_code: Some(3) });
}
